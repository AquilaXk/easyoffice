//! Displayed text of non-text Pages table cells: decimal numbers (fixed decimals, thousands
//! separators, percent, currency symbols), dates (pattern subset) and checkboxes. Digits are
//! produced from a stack buffer with a power-of-ten lookup table; only the final cell string is
//! allocated. Format kind numbers live in [`crate::pages_schema::cell_format`].

use std::fmt::Write;

use super::packed::{CellScalar, Decimal, PackedCell};
use crate::pages_proto::Message;
use crate::pages_schema::{cell, cell_format, DATE_EPOCH_DAYS_FROM_UNIX};

/// Largest number of decimal places that is rendered.
const MAX_PLACES: u32 = 20;
/// Decimal digits of `u128::MAX`.
const U128_DIGITS: usize = 39;
const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_HOUR: i64 = 3_600;
const SECONDS_PER_MINUTE: i64 = 60;
/// Largest absolute date value (seconds) that is converted (about 31,000 years).
const MAX_DATE_SECONDS: f64 = 1e12;
const DEFAULT_DATE_PATTERN: &str = "yyyy-MM-dd";
const THOUSANDS_GROUP: usize = 3;
const PERCENT_SHIFT: i32 = 2;
/// Offset of the civil calendar epoch (0000-03-01) from 1970-01-01, in days.
const CIVIL_EPOCH_SHIFT_DAYS: i64 = 719_468;
const DAYS_PER_ERA: i64 = 146_097;

const POW10: [u128; U128_DIGITS] = {
    let mut table = [1u128; U128_DIGITS];
    let mut i = 1;
    while i < U128_DIGITS {
        table[i] = table[i - 1] * 10;
        i += 1;
    }
    table
};

fn pow10(exp: u32) -> Option<u128> {
    POW10.get(usize::try_from(exp).ok()?).copied()
}

/// Format of one cell (an entry of the table's format list), borrowing the decoded buffers.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FormatSpec<'a> {
    kind: u64,
    pattern: Option<&'a str>,
    decimals: Option<u32>,
    currency: Option<&'a str>,
    thousands: bool,
    duration_style: u64,
    duration_largest: u64,
    duration_smallest: u64,
}

impl<'a> FormatSpec<'a> {
    /// Reads both layouts: the older one (decimals in field 3, currency code in field 4, kind 256 =
    /// currency, 257 = number) and the Pages 14.4 one (decimals in field 2, currency code in field
    /// 3, kind 256 = number, 257 = currency). A currency code string tells them apart; the kind is
    /// normalised to the older numbering.
    pub fn parse(m: &Message<'a>) -> Self {
        let text = |n: u32| m.bytes(n).and_then(|b| std::str::from_utf8(b).ok());
        let currency = text(cell_format::CURRENCY).or_else(|| text(cell_format::CURRENCY_V2));
        let raw_kind = m.varint(cell_format::KIND).unwrap_or(0);
        let kind = match raw_kind {
            cell_format::KIND_CURRENCY | cell_format::KIND_NUMBER if currency.is_some() => {
                cell_format::KIND_CURRENCY
            }
            cell_format::KIND_CURRENCY | cell_format::KIND_NUMBER => cell_format::KIND_NUMBER,
            other => other,
        };
        let decimals = m
            .varint(cell_format::DECIMALS)
            .or_else(|| m.varint(cell_format::DECIMALS_V2))
            .filter(|v| *v <= cell_format::MAX_DECIMALS)
            .and_then(|v| u32::try_from(v).ok());
        Self {
            kind,
            pattern: text(cell_format::PATTERN),
            decimals,
            currency,
            thousands: m.varint(cell_format::THOUSANDS).is_some_and(|v| v != 0),
            duration_style: m.varint(cell_format::DURATION_STYLE).unwrap_or(0),
            duration_largest: m.varint(cell_format::DURATION_LARGEST).unwrap_or(0),
            duration_smallest: m.varint(cell_format::DURATION_SMALLEST).unwrap_or(0),
        }
    }
}

/// How a number is written around its digits.
struct NumberStyle<'s> {
    shift: i32,
    decimals: Option<u32>,
    thousands: bool,
    prefix: &'s str,
    gap: bool,
    suffix: &'s str,
}

fn currency_symbol(code: &str) -> Option<&'static str> {
    match code {
        "KRW" => Some("\u{20A9}"),
        "USD" => Some("$"),
        "EUR" => Some("\u{20AC}"),
        "JPY" | "CNY" => Some("\u{A5}"),
        "GBP" => Some("\u{A3}"),
        "AUD" => Some("A$"),
        "CAD" => Some("CA$"),
        "HKD" => Some("HK$"),
        "SGD" => Some("S$"),
        "TWD" => Some("NT$"),
        "INR" => Some("\u{20B9}"),
        "BRL" => Some("R$"),
        "MXN" => Some("MX$"),
        "VND" => Some("\u{20AB}"),
        "THB" => Some("\u{0E3F}"),
        "RUB" => Some("\u{20BD}"),
        "TRY" => Some("\u{20BA}"),
        _ => None,
    }
}

fn number_style<'s>(spec: Option<&'s FormatSpec<'_>>) -> NumberStyle<'s> {
    let mut style = NumberStyle {
        shift: 0,
        decimals: None,
        thousands: false,
        prefix: "",
        gap: false,
        suffix: "",
    };
    let Some(s) = spec else { return style };
    if !matches!(
        s.kind,
        cell_format::KIND_CURRENCY | cell_format::KIND_NUMBER | cell_format::KIND_PERCENT
    ) {
        return style;
    }
    style.decimals = s.decimals;
    style.thousands = s.thousands;
    match s.kind {
        cell_format::KIND_PERCENT => {
            style.shift = PERCENT_SHIFT;
            style.suffix = "%";
        }
        cell_format::KIND_CURRENCY => {
            let code = s.currency.unwrap_or("");
            match currency_symbol(code) {
                Some(symbol) => style.prefix = symbol,
                None => {
                    style.prefix = code;
                    style.gap = !code.is_empty();
                }
            }
        }
        _ => {}
    }
    style
}

/// `round(coefficient * 10^shift)` with half-up rounding for negative shifts.
fn scale(coefficient: u64, shift: i32) -> Option<u128> {
    let value = u128::from(coefficient);
    if shift >= 0 {
        return value.checked_mul(pow10(shift.unsigned_abs())?);
    }
    match pow10(shift.unsigned_abs()) {
        Some(div) => Some((value + div / 2) / div),
        None => Some(0),
    }
}

fn write_number(out: &mut String, d: Decimal, st: &NumberStyle) -> Option<()> {
    let exp = d.exponent.checked_add(st.shift)?;
    let places = st
        .decimals
        .unwrap_or_else(|| exp.min(0).unsigned_abs())
        .min(MAX_PLACES);
    let scaled = scale(d.coefficient, exp.checked_add(i32::try_from(places).ok()?)?)?;
    let mut rev = [0u8; U128_DIGITS];
    let mut count = 0usize;
    let mut rest = scaled;
    loop {
        rev[count] = b'0' + (rest % 10) as u8;
        rest /= 10;
        count += 1;
        if rest == 0 {
            break;
        }
    }
    let places = places as usize;
    let total = count.max(places.checked_add(1)?);
    if d.negative && scaled != 0 {
        out.push('-');
    }
    out.push_str(st.prefix);
    if st.gap {
        out.push(' ');
    }
    let mut buf = [0u8; 64];
    let mut pos = 0usize;
    let digit = |i: usize| if i < count { rev[i] } else { b'0' };
    for i in (places..total).rev() {
        buf[pos] = digit(i);
        pos += 1;
        if st.thousands && i > places && (i - places) % THOUSANDS_GROUP == 0 {
            buf[pos] = b',';
            pos += 1;
        }
    }
    if places > 0 {
        buf[pos] = b'.';
        pos += 1;
        for i in (0..places).rev() {
            buf[pos] = digit(i);
            pos += 1;
        }
    }
    out.push_str(std::str::from_utf8(&buf[..pos]).ok()?);
    out.push_str(st.suffix);
    Some(())
}

struct DateParts {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
}

/// Civil date of a day count since 1970-01-01 (proleptic Gregorian).
fn civil_from_days(days: i64) -> Option<(i64, i64, i64)> {
    let z = days.checked_add(CIVIL_EPOCH_SHIFT_DAYS)?;
    let era = z.div_euclid(DAYS_PER_ERA);
    let doe = z.rem_euclid(DAYS_PER_ERA);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era.checked_mul(400)? + i64::from(month <= 2);
    Some((year, month, day))
}

fn date_parts(seconds: f64) -> Option<DateParts> {
    if seconds.abs() >= MAX_DATE_SECONDS {
        return None;
    }
    let whole = seconds.floor() as i64;
    let days = whole
        .div_euclid(SECONDS_PER_DAY)
        .checked_add(DATE_EPOCH_DAYS_FROM_UNIX)?;
    let in_day = whole.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days)?;
    Some(DateParts {
        year,
        month,
        day,
        hour: in_day / SECONDS_PER_HOUR,
        minute: in_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        second: in_day % SECONDS_PER_MINUTE,
    })
}

/// Writes one pattern field (`yyyy`, `yy`, `M`/`MM`, `d`/`dd`, `H`/`HH`, `h`/`hh`, `m`/`mm`, `s`/`ss`, `a`/`aa`).
fn write_field(out: &mut String, letter: char, run: usize, t: &DateParts) -> Option<()> {
    if matches!((letter, run), ('a', 1..=2)) {
        out.push_str(if t.hour < 12 { "AM" } else { "PM" });
        return Some(());
    }
    let (value, width) = match (letter, run) {
        ('y', 4) => (t.year, 4),
        ('y', 2) => (t.year.rem_euclid(100), 2),
        ('M', 1 | 2) => (t.month, run),
        ('d', 1 | 2) => (t.day, run),
        ('H', 1 | 2) => (t.hour, run),
        ('h', 1 | 2) => ((t.hour + 11).rem_euclid(12) + 1, run),
        ('m', 1 | 2) => (t.minute, run),
        ('s', 1 | 2) => (t.second, run),
        _ => return None,
    };
    write!(out, "{value:0width$}").ok()
}

fn write_pattern(out: &mut String, pattern: &str, t: &DateParts) -> Option<()> {
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.next_if_eq(&'\'').is_some() {
                out.push('\'');
                continue;
            }
            loop {
                let q = chars.next()?;
                if q == '\'' {
                    if chars.next_if_eq(&'\'').is_some() {
                        out.push('\'');
                    } else {
                        break;
                    }
                } else {
                    out.push(q);
                }
            }
            continue;
        }
        if !c.is_ascii_alphabetic() {
            out.push(c);
            continue;
        }
        let mut run = 1usize;
        while chars.next_if_eq(&c).is_some() {
            run += 1;
        }
        write_field(out, c, run, t)?;
    }
    Some(())
}

fn format_date(seconds: f64, pattern: Option<&str>) -> Option<String> {
    let parts = date_parts(seconds)?;
    let mut out = String::with_capacity(pattern.map_or(DEFAULT_DATE_PATTERN.len(), str::len) + 8);
    let written = pattern.and_then(|p| write_pattern(&mut out, p, &parts));
    if written.is_none() {
        out.clear();
        write_pattern(&mut out, DEFAULT_DATE_PATTERN, &parts)?;
    }
    Some(out)
}

/// Duration units, largest first: `(format unit code, seconds, display suffix)`.
const DURATION_UNITS: [(u64, u64, &str); 5] = [
    (
        cell_format::UNIT_WEEK,
        7 * SECONDS_PER_DAY as u64,
        "\u{C8FC}",
    ),
    (cell_format::UNIT_DAY, SECONDS_PER_DAY as u64, "\u{C77C}"),
    (
        cell_format::UNIT_HOUR,
        SECONDS_PER_HOUR as u64,
        "\u{C2DC}\u{AC04}",
    ),
    (
        cell_format::UNIT_MINUTE,
        SECONDS_PER_MINUTE as u64,
        "\u{BD84}",
    ),
    (cell_format::UNIT_SECOND, 1, "\u{CD08}"),
];

/// Compact duration in the Korean unit words of the Pages screen (`1시간 30분`, `2일 3시간`): every unit from the largest to the smallest of the format
/// is written (zero parts included), the smallest unit is rounded half up. Only the compact style
/// and non-negative values are shown.
fn format_duration(seconds: f64, spec: Option<&FormatSpec>) -> Option<String> {
    let spec = spec.filter(|s| {
        s.kind == cell_format::KIND_DURATION
            && s.duration_style == cell_format::DURATION_STYLE_COMPACT
    })?;
    if !(0.0..MAX_DATE_SECONDS).contains(&seconds) {
        return None;
    }
    let first = DURATION_UNITS
        .iter()
        .position(|u| u.0 == spec.duration_largest)?;
    let last = DURATION_UNITS
        .iter()
        .position(|u| u.0 == spec.duration_smallest)?;
    if first > last {
        return None;
    }
    let smallest = DURATION_UNITS[last].1;
    let mut rest = (seconds / smallest as f64 + 0.5).floor() as u64;
    let mut parts = [0u64; DURATION_UNITS.len()];
    for i in (first..=last).rev() {
        let per_larger = if i == first {
            u64::MAX
        } else {
            DURATION_UNITS[i - 1].1 / DURATION_UNITS[i].1
        };
        parts[i] = rest % per_larger;
        rest /= per_larger;
    }
    let mut out = String::with_capacity(DURATION_UNITS.len() * 5);
    for i in first..=last {
        if i > first {
            out.push(' ');
        }
        write!(out, "{}{}", parts[i], DURATION_UNITS[i].2).ok()?;
    }
    Some(out)
}

fn format_bool(value: f64, spec: Option<&FormatSpec>) -> &'static str {
    let on = value != 0.0;
    if spec.is_some_and(|s| s.kind == cell_format::KIND_CHECKBOX) {
        return if on { "\u{2611}" } else { "\u{2610}" };
    }
    if on {
        "TRUE"
    } else {
        "FALSE"
    }
}

/// Displayed text of a number, date or checkbox cell; `None` when the value cannot be shown.
pub(crate) fn render_value(packed: &PackedCell, spec: Option<&FormatSpec>) -> Option<String> {
    match (packed.kind, packed.value) {
        (cell::TYPE_NUMBER | cell::TYPE_CURRENCY, CellScalar::Decimal(d)) => {
            let mut out = String::with_capacity(U128_DIGITS);
            write_number(&mut out, d, &number_style(spec))?;
            Some(out)
        }
        (cell::TYPE_DATE, CellScalar::Double(v)) => format_date(v, spec.and_then(|s| s.pattern)),
        (cell::TYPE_BOOL, CellScalar::Double(v)) => Some(format_bool(v, spec).to_string()),
        (cell::TYPE_DURATION, CellScalar::Double(v)) => format_duration(v, spec),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(coefficient: u64, exponent: i32, negative: bool) -> PackedCell {
        PackedCell {
            kind: cell::TYPE_NUMBER,
            rich_text_key: None,
            string_key: None,
            format_key: None,
            value: CellScalar::Decimal(Decimal {
                negative,
                coefficient,
                exponent,
            }),
        }
    }

    fn spec(kind: u64, decimals: Option<u32>, thousands: bool, code: &str) -> FormatSpec<'_> {
        FormatSpec {
            kind,
            pattern: None,
            decimals,
            currency: Some(code),
            thousands,
            ..FormatSpec::default()
        }
    }

    #[test]
    fn plain_and_fixed_numbers() {
        assert_eq!(
            render_value(&number(12345, -1, false), None).unwrap(),
            "1234.5"
        );
        assert_eq!(render_value(&number(100, 0, false), None).unwrap(), "100");
        assert_eq!(render_value(&number(5, 2, false), None).unwrap(), "500");
        assert_eq!(render_value(&number(0, 0, true), None).unwrap(), "0");
        let fixed = spec(cell_format::KIND_NUMBER, Some(2), true, "");
        assert_eq!(
            render_value(&number(123456789, -1, true), Some(&fixed)).unwrap(),
            "-12,345,678.90"
        );
        assert_eq!(
            render_value(
                &number(5, -3, false),
                Some(&spec(cell_format::KIND_NUMBER, Some(2), false, ""))
            )
            .unwrap(),
            "0.01"
        );
    }

    #[test]
    fn percent_and_currency() {
        let pct = spec(cell_format::KIND_PERCENT, Some(0), true, "");
        assert_eq!(
            render_value(&number(5, -1, false), Some(&pct)).unwrap(),
            "50%"
        );
        let krw = spec(cell_format::KIND_CURRENCY, Some(0), true, "KRW");
        assert_eq!(
            render_value(&number(10000, 0, false), Some(&krw)).unwrap(),
            "\u{20A9}10,000"
        );
        for (code, expected) in [
            ("CNY", "\u{A5}25.50"),
            ("AUD", "A$25.50"),
            ("CAD", "CA$25.50"),
            ("HKD", "HK$25.50"),
            ("SGD", "S$25.50"),
            ("TWD", "NT$25.50"),
            ("INR", "\u{20B9}25.50"),
            ("BRL", "R$25.50"),
            ("MXN", "MX$25.50"),
            ("VND", "\u{20AB}25.50"),
            ("THB", "\u{0E3F}25.50"),
            ("RUB", "\u{20BD}25.50"),
            ("TRY", "\u{20BA}25.50"),
        ] {
            let cur = spec(cell_format::KIND_CURRENCY, Some(2), false, code);
            assert_eq!(
                render_value(&number(255, -1, false), Some(&cur)).unwrap(),
                expected
            );
        }
        let chf = spec(cell_format::KIND_CURRENCY, Some(2), false, "CHF");
        assert_eq!(
            render_value(&number(1, 0, false), Some(&chf)).unwrap(),
            "CHF 1.00"
        );
    }

    #[test]
    fn extreme_exponents_fail_closed_or_round_to_zero() {
        assert!(render_value(&number(1, i32::MAX, false), None).is_none());
        assert!(render_value(&number(1, 60, false), None).is_none());
        assert_eq!(
            render_value(
                &number(1, -60, false),
                Some(&spec(cell_format::KIND_NUMBER, Some(2), false, ""))
            )
            .unwrap(),
            "0.00"
        );
    }

    #[test]
    fn dates_use_the_pattern_or_a_default() {
        let at = |secs: f64, pattern: Option<&str>| {
            let packed = PackedCell {
                kind: cell::TYPE_DATE,
                rich_text_key: None,
                string_key: None,
                format_key: None,
                value: CellScalar::Double(secs),
            };
            let spec = FormatSpec {
                pattern,
                ..FormatSpec::default()
            };
            render_value(&packed, Some(&spec))
        };
        // 2001-01-01 00:00:00 is the epoch; 86400 * 31 + 3661 is 2001-02-01 01:01:01.
        assert_eq!(at(0.0, None).unwrap(), "2001-01-01");
        assert_eq!(at(0.0, Some("hh:mm a")).unwrap(), "12:00 AM");
        assert_eq!(
            at(
                13.0 * 3_600.0 + 300.0,
                Some("yyyy'년' M'월' d'일' a h'시' mm'분' '''o''clock'''")
            )
            .unwrap(),
            "2001년 1월 1일 PM 1시 05분 'o'clock'"
        );
        let feb = 86_400.0 * 31.0 + 3_661.0;
        assert_eq!(
            at(feb, Some("yyyy.M.d HH:mm:ss")).unwrap(),
            "2001.2.1 01:01:01"
        );
        assert_eq!(at(feb, Some("EEEE")).unwrap(), "2001-02-01");
        assert_eq!(at(-86_400.0, Some("yy/MM/dd")).unwrap(), "00/12/31");
        assert!(at(f64::from_bits(0x41c8_4ea8_0000_0000), None)
            .unwrap()
            .starts_with("2026-11-"));
        assert!(at(1e15, None).is_none());
    }

    #[test]
    fn checkbox_and_bool() {
        let packed = |v: f64| PackedCell {
            kind: cell::TYPE_BOOL,
            rich_text_key: None,
            string_key: None,
            format_key: None,
            value: CellScalar::Double(v),
        };
        let checkbox = FormatSpec {
            kind: cell_format::KIND_CHECKBOX,
            ..FormatSpec::default()
        };
        assert_eq!(
            render_value(&packed(1.0), Some(&checkbox)).unwrap(),
            "\u{2611}"
        );
        assert_eq!(
            render_value(&packed(0.0), Some(&checkbox)).unwrap(),
            "\u{2610}"
        );
        assert_eq!(render_value(&packed(1.0), None).unwrap(), "TRUE");
    }

    fn push_varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push((value & 0x7F) as u8 | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn format_message(fields: &[(u32, u64)], code: Option<(u32, &str)>) -> Vec<u8> {
        let mut out = Vec::new();
        for (number, value) in fields {
            push_varint(&mut out, u64::from(*number) << 3);
            push_varint(&mut out, *value);
        }
        if let Some((number, text)) = code {
            push_varint(&mut out, u64::from(number << 3) | 2);
            push_varint(&mut out, text.len() as u64);
            out.extend_from_slice(text.as_bytes());
        }
        out
    }

    #[test]
    fn both_format_layouts_give_the_same_number_kinds() {
        // Pages 14.4: 256 = number (decimals in field 2), 257 = currency (code in field 3).
        let buf = format_message(&[(1, 256), (2, 1), (5, 1)], None);
        let spec = FormatSpec::parse(&Message::parse(&buf).unwrap());
        assert_eq!(
            (spec.kind, spec.decimals, spec.thousands),
            (cell_format::KIND_NUMBER, Some(1), true)
        );
        let buf = format_message(&[(1, 257), (2, 0), (5, 0)], Some((3, "KRW")));
        let spec = FormatSpec::parse(&Message::parse(&buf).unwrap());
        assert_eq!(
            (spec.kind, spec.currency),
            (cell_format::KIND_CURRENCY, Some("KRW"))
        );
        // Older layout: 256 = currency (code in field 4), 257 = number (decimals in field 3).
        let buf = format_message(&[(1, 256), (3, 0)], Some((4, "USD")));
        let spec = FormatSpec::parse(&Message::parse(&buf).unwrap());
        assert_eq!(
            (spec.kind, spec.currency),
            (cell_format::KIND_CURRENCY, Some("USD"))
        );
        let buf = format_message(&[(1, 257), (3, 2), (5, 1)], None);
        let spec = FormatSpec::parse(&Message::parse(&buf).unwrap());
        assert_eq!(
            (spec.kind, spec.decimals),
            (cell_format::KIND_NUMBER, Some(2))
        );
        // "Automatic" decimals (a large marker value) are not a place count.
        let buf = format_message(&[(1, 256), (2, 21)], None);
        assert_eq!(
            FormatSpec::parse(&Message::parse(&buf).unwrap()).decimals,
            None
        );
    }

    fn duration_spec(style: u64, largest: u64, smallest: u64) -> FormatSpec<'static> {
        FormatSpec {
            kind: cell_format::KIND_DURATION,
            duration_style: style,
            duration_largest: largest,
            duration_smallest: smallest,
            ..FormatSpec::default()
        }
    }

    #[test]
    fn compact_durations_cover_the_unit_range_and_fail_closed() {
        let hm = duration_spec(1, cell_format::UNIT_HOUR, cell_format::UNIT_MINUTE);
        assert_eq!(
            format_duration(5400.0, Some(&hm)).as_deref(),
            Some("1시간 30분")
        );
        assert_eq!(
            format_duration(3600.0, Some(&hm)).as_deref(),
            Some("1시간 0분")
        );
        assert_eq!(
            format_duration(5429.0, Some(&hm)).as_deref(),
            Some("1시간 30분")
        );
        assert_eq!(
            format_duration(5430.0, Some(&hm)).as_deref(),
            Some("1시간 31분")
        );
        let dh = duration_spec(1, cell_format::UNIT_DAY, cell_format::UNIT_HOUR);
        assert_eq!(
            format_duration(183_600.0, Some(&dh)).as_deref(),
            Some("2일 3시간")
        );
        let wds = duration_spec(1, cell_format::UNIT_WEEK, cell_format::UNIT_SECOND);
        assert_eq!(
            format_duration(694_861.0, Some(&wds)).as_deref(),
            Some("1주 1일 1시간 1분 1초")
        );
        assert_eq!(format_duration(-1.0, Some(&hm)), None);
        assert_eq!(format_duration(f64::NAN, Some(&hm)), None);
        assert_eq!(format_duration(60.0, None), None);
        assert_eq!(format_duration(60.0, Some(&duration_spec(0, 4, 8))), None);
        assert_eq!(format_duration(60.0, Some(&duration_spec(1, 8, 4))), None);
        assert_eq!(format_duration(60.0, Some(&duration_spec(1, 3, 8))), None);
    }
}

//! Excel-compatible number format codes for the `TEXT` worksheet function.
//!
//! Supports literal text (quoted, backslash-escaped and bare characters such as `원`, `₩`, `$`),
//! `0`/`#`/`?` digit placeholders with zero padding, thousands grouping and `,` scaling, `%`,
//! scientific `E+`/`E-`, up to four `;` sections (positive;negative;zero;text), `@` text
//! substitution, `General`, date/time codes (`yyyy`, `yy`, `m`..`mmmmm`, `d`..`dddd`, `h`/`hh`,
//! minute `m` after hours or before seconds, `ss`, fractional seconds, `AM/PM`, `A/P`) and
//! elapsed `[h]`, `[m]`, `[s]`. Month and weekday names use the English (en-US) locale.

use crate::coercion::format_general_number;
use crate::types::FormulaError;

const MAX_FRACTION_DIGITS: usize = 10;
const MAX_DATE_SERIAL: f64 = 2_958_466.0;
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Lit(String),
    Digit(char),
    Dot,
    Comma,
    Percent,
    At,
    General,
    Exp(bool),
    Year(usize),
    Month(usize),
    Day(usize),
    Hour(usize),
    Second(usize),
    AmPm(bool, bool),
    Elapsed(char, usize),
    Section,
}

fn run_len(chars: &[char], start: usize, target: char) -> usize {
    chars[start..]
        .iter()
        .take_while(|c| c.to_ascii_lowercase() == target)
        .count()
}

#[inline]
fn starts_with_ci(chars: &[char], start: usize, pat: &str) -> bool {
    start + pat.len() <= chars.len()
        && pat
            .bytes()
            .zip(&chars[start..])
            .all(|(p, &c)| c.is_ascii() && (c as u8).eq_ignore_ascii_case(&p))
}

fn parse_bracket(content: &str) -> Option<Tok> {
    let lower = content.to_ascii_lowercase();
    for code in ['h', 'm', 's'] {
        if !lower.is_empty() && lower.chars().all(|c| c == code) {
            return Some(Tok::Elapsed(code, lower.len()));
        }
    }
    if let Some(rest) = content.strip_prefix('$') {
        let symbol = rest.split('-').next().unwrap_or("");
        return Some(Tok::Lit(symbol.to_string()));
    }
    None
}

fn tokenize(fmt: &str) -> Vec<Tok> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '"' => {
                let mut lit = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    lit.push(chars[i]);
                    i += 1;
                }
                i += 1;
                toks.push(Tok::Lit(lit));
                continue;
            }
            '\\' => {
                if let Some(&next) = chars.get(i + 1) {
                    toks.push(Tok::Lit(next.to_string()));
                }
                i += 2;
                continue;
            }
            '_' => {
                toks.push(Tok::Lit(" ".to_string()));
                i += 2;
                continue;
            }
            '*' => {
                i += 2;
                continue;
            }
            '[' => {
                let close = chars[i + 1..].iter().position(|&c| c == ']');
                if let Some(off) = close {
                    let content: String = chars[i + 1..i + 1 + off].iter().collect();
                    if let Some(tok) = parse_bracket(&content) {
                        toks.push(tok);
                    }
                    i += off + 2;
                } else {
                    toks.push(Tok::Lit("[".to_string()));
                    i += 1;
                }
                continue;
            }
            ';' => toks.push(Tok::Section),
            '0' | '#' | '?' => toks.push(Tok::Digit(ch)),
            '.' => toks.push(Tok::Dot),
            ',' => toks.push(Tok::Comma),
            '%' => toks.push(Tok::Percent),
            '@' => toks.push(Tok::At),
            'E' | 'e' if matches!(chars.get(i + 1), Some('+') | Some('-')) => {
                toks.push(Tok::Exp(chars[i + 1] == '+'));
                i += 2;
                continue;
            }
            _ if starts_with_ci(&chars, i, "am/pm") => {
                toks.push(Tok::AmPm(true, ch.is_ascii_lowercase()));
                i += 5;
                continue;
            }
            _ if starts_with_ci(&chars, i, "a/p") => {
                toks.push(Tok::AmPm(false, ch.is_ascii_lowercase()));
                i += 3;
                continue;
            }
            _ if starts_with_ci(&chars, i, "general") => {
                toks.push(Tok::General);
                i += 7;
                continue;
            }
            'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' => {
                let lower = ch.to_ascii_lowercase();
                let n = run_len(&chars, i, lower);
                toks.push(match lower {
                    'y' => Tok::Year(n),
                    'm' => Tok::Month(n),
                    'd' => Tok::Day(n),
                    'h' => Tok::Hour(n),
                    _ => Tok::Second(n),
                });
                i += n;
                continue;
            }
            _ => toks.push(Tok::Lit(ch.to_string())),
        }
        i += 1;
    }
    toks
}

fn is_date_tok(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Year(_)
            | Tok::Month(_)
            | Tok::Day(_)
            | Tok::Hour(_)
            | Tok::Second(_)
            | Tok::AmPm(..)
            | Tok::Elapsed(..)
    )
}

/// Formats `num` with an Excel number format code. Invalid date serials fall back to
/// `General` so callers that need a plain string never fail.
pub fn format_text_pattern(num: f64, fmt_str: &str) -> String {
    format_number_with_code(num, fmt_str).unwrap_or_else(|_| format_general_number(num))
}

/// Applies the text section (`@`) of a format code to a text value, mirroring `TEXT("abc", fmt)`.
pub fn format_text_value_with_code(text: &str, fmt_str: &str) -> String {
    let sections = split_sections(tokenize(fmt_str));
    let section = if sections.len() >= 4 {
        Some(&sections[3])
    } else {
        sections.iter().find(|s| s.contains(&Tok::At))
    };
    let Some(sec) = section else {
        return text.to_string();
    };
    let mut out = String::new();
    for t in sec {
        match t {
            Tok::At => out.push_str(text),
            Tok::Lit(s) => out.push_str(s),
            _ => {}
        }
    }
    out
}

fn split_sections(toks: Vec<Tok>) -> Vec<Vec<Tok>> {
    let mut sections = vec![Vec::new()];
    for t in toks {
        if t == Tok::Section {
            sections.push(Vec::new());
        } else if let Some(last) = sections.last_mut() {
            last.push(t);
        }
    }
    sections
}

/// Formats `num` with an Excel number format code, returning `#VALUE!` for date/time codes
/// applied to serials outside `0..=2958465.99999`.
pub fn format_number_with_code(num: f64, fmt_str: &str) -> Result<String, FormulaError> {
    if !num.is_finite() {
        return Err(FormulaError::Num);
    }
    let sections = split_sections(tokenize(fmt_str));
    let (section, value, auto_minus) = match sections.len() {
        1 => (&sections[0], num, num < 0.0),
        2 => {
            if num < 0.0 {
                (&sections[1], -num, false)
            } else {
                (&sections[0], num, false)
            }
        }
        _ => {
            if num < 0.0 {
                (&sections[1], -num, false)
            } else if num == 0.0 {
                (&sections[2], num, false)
            } else {
                (&sections[0], num, false)
            }
        }
    };
    if section.iter().any(is_date_tok) {
        if !(0.0..MAX_DATE_SERIAL).contains(&value) {
            return Err(FormulaError::Value);
        }
        return Ok(format_date_section(section, value));
    }
    let body = format_number_section(section, value.abs());
    Ok(if auto_minus { format!("-{body}") } else { body })
}

fn round_half_away(v: f64, decimals: usize) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    let q = v * factor;
    if !q.is_finite() {
        return v;
    }
    let nudge = q.abs() * f64::EPSILON * 2.0;
    (q + nudge.copysign(q)).round() / factor
}

fn group_thousands(digits: &str) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::with_capacity(chars.len() + chars.len() / 3);
    for (idx, ch) in chars.iter().enumerate() {
        let remaining = chars.len() - idx;
        if idx > 0 && remaining % 3 == 0 && chars[idx - 1].is_ascii_digit() {
            out.push(',');
        }
        out.push(*ch);
    }
    out
}

fn pad_char(ph: char) -> Option<char> {
    match ph {
        '0' => Some('0'),
        '?' => Some(' '),
        _ => None,
    }
}

/// Digit placeholder characters (`0`, `#`, `?`) of the given tokens.
fn digit_placeholders(tokens: &[Tok]) -> Vec<char> {
    tokens
        .iter()
        .filter_map(|t| match t {
            Tok::Digit(c) => Some(*c),
            _ => None,
        })
        .collect()
}

/// Formats a section without digit placeholders (or with `General`).
fn format_general_section(section: &[Tok], value: f64) -> String {
    let mut out = String::new();
    for t in section {
        match t {
            Tok::General | Tok::At => out.push_str(&format_general_number(value)),
            Tok::Lit(s) => out.push_str(s),
            Tok::Percent => out.push('%'),
            Tok::Dot => out.push('.'),
            Tok::Comma => out.push(','),
            _ => {}
        }
    }
    out
}

/// Classifies commas in the numeric part: `(thousands grouping, trailing scale commas)`.
fn classify_commas(
    section: &[Tok],
    first_digit: usize,
    scan_end: usize,
    dot_pos: Option<usize>,
) -> (bool, i32) {
    let mut grouping = false;
    let mut scale_commas = 0i32;
    for (idx, t) in section.iter().enumerate().take(scan_end) {
        if *t != Tok::Comma || idx < first_digit {
            continue;
        }
        let next_is_digit = section[idx + 1..scan_end]
            .iter()
            .find(|t| !matches!(t, Tok::Comma))
            .is_some_and(|t| matches!(t, Tok::Digit(_)));
        if next_is_digit && dot_pos.is_none_or(|d| idx < d) {
            grouping = true;
        } else if !next_is_digit {
            scale_commas += 1;
        }
    }
    (grouping, scale_commas)
}

/// Normalizes `v` to scientific notation; returns `(mantissa, exponent)`.
fn scientific_parts(v: f64, int_placeholders: usize, frac_len: usize) -> (f64, i32) {
    let int_digits = int_placeholders.max(1) as i32;
    let mut exponent = v.log10().floor() as i32 - (int_digits - 1);
    let mut mantissa = round_half_away(v / 10f64.powi(exponent), frac_len);
    if mantissa >= 10f64.powi(int_digits) {
        exponent += 1;
        mantissa = round_half_away(v / 10f64.powi(exponent), frac_len);
    }
    (mantissa, exponent)
}

/// Distributes integer digits (right-aligned) over the integer placeholders.
fn layout_int_slots(int_phs: &[char], int_digits: &[char]) -> Vec<String> {
    let n_int = int_phs.len();
    let mut int_slots: Vec<String> = vec![String::new(); n_int];
    for k in 0..n_int {
        let slot = n_int - 1 - k;
        if k < int_digits.len() {
            int_slots[slot].push(int_digits[int_digits.len() - 1 - k]);
        } else if let Some(p) = pad_char(int_phs[slot]) {
            int_slots[slot].push(p);
        }
    }
    if int_digits.len() > n_int && n_int > 0 {
        let overflow: String = int_digits[..int_digits.len() - n_int].iter().collect();
        int_slots[0].insert_str(0, &overflow);
    }
    int_slots
}

/// Fraction digit slots with optional trailing zeros blanked or space-padded per placeholder.
fn layout_frac_slots(frac_raw: &str, frac_phs: &[char]) -> Vec<String> {
    let mut frac_slots: Vec<String> = frac_raw
        .chars()
        .zip(frac_phs.iter())
        .map(|(d, _)| d.to_string())
        .collect();
    for idx in (0..frac_slots.len()).rev() {
        if frac_phs[idx] == '0' || frac_slots[idx] != "0" {
            break;
        }
        frac_slots[idx] = pad_char(frac_phs[idx]).map_or_else(String::new, |c| c.to_string());
    }
    frac_slots
}

/// Precomputed digit layout used while emitting a numeric section.
struct NumberLayout {
    int_digits: Vec<char>,
    int_slots: Vec<String>,
    int_block: Option<String>,
    frac_slots: Vec<String>,
    exponent: i32,
    exp_min_len: usize,
    dot_pos: Option<usize>,
}

/// Appends the exponent part (`E+05` style) to `out`.
fn push_exponent(out: &mut String, plus: bool, exponent: i32, min_len: usize) {
    out.push('E');
    if exponent < 0 {
        out.push('-');
    } else if plus {
        out.push('+');
    }
    let digits = exponent.unsigned_abs().to_string();
    for _ in digits.len()..min_len {
        out.push('0');
    }
    out.push_str(&digits);
}

fn emit_number_section(section: &[Tok], layout: &NumberLayout, value: f64) -> String {
    let mut out = String::new();
    let mut int_idx = 0usize;
    let mut frac_idx = 0usize;
    let mut in_exponent = false;
    for (idx, t) in section.iter().enumerate() {
        match t {
            Tok::Digit(_) if in_exponent => {}
            Tok::Digit(_) if layout.dot_pos.is_none_or(|d| idx < d) => {
                if let Some(ref block) = layout.int_block {
                    if int_idx == 0 {
                        out.push_str(block);
                    }
                } else if let Some(slot) = layout.int_slots.get(int_idx) {
                    out.push_str(slot);
                }
                int_idx += 1;
            }
            Tok::Digit(_) => {
                if let Some(slot) = layout.frac_slots.get(frac_idx) {
                    out.push_str(slot);
                }
                frac_idx += 1;
            }
            Tok::Dot if Some(idx) == layout.dot_pos => {
                if layout.int_slots.is_empty() {
                    out.extend(layout.int_digits.iter());
                }
                out.push('.');
            }
            Tok::Dot => out.push('.'),
            Tok::Percent => out.push('%'),
            Tok::Lit(s) => out.push_str(s),
            Tok::Exp(plus) => {
                in_exponent = true;
                push_exponent(&mut out, *plus, layout.exponent, layout.exp_min_len);
            }
            Tok::General | Tok::At => out.push_str(&format_general_number(value)),
            _ => {}
        }
    }
    out
}

fn format_number_section(section: &[Tok], value: f64) -> String {
    if section.is_empty() {
        return String::new();
    }
    if section.contains(&Tok::General) || !section.iter().any(|t| matches!(t, Tok::Digit(_))) {
        return format_general_section(section, value);
    }

    let first_digit = section.iter().position(|t| matches!(t, Tok::Digit(_)));
    let last_digit = section.iter().rposition(|t| matches!(t, Tok::Digit(_)));
    let (Some(first_digit), Some(last_digit)) = (first_digit, last_digit) else {
        return String::new();
    };
    let exp_pos = section.iter().position(|t| matches!(t, Tok::Exp(_)));
    let mantissa_end = exp_pos.unwrap_or(last_digit + 1);
    let dot_pos = section[..mantissa_end].iter().position(|t| *t == Tok::Dot);
    let int_end = dot_pos.unwrap_or(mantissa_end);
    let scan_end = exp_pos.unwrap_or(section.len());

    let int_phs = digit_placeholders(&section[..int_end]);
    let frac_phs: Vec<char> = dot_pos.map_or_else(Vec::new, |d| {
        let mut phs = digit_placeholders(&section[d + 1..mantissa_end]);
        phs.truncate(MAX_FRACTION_DIGITS);
        phs
    });
    let (grouping, scale_commas) = classify_commas(section, first_digit, scan_end, dot_pos);
    let percent_count = section.iter().filter(|t| **t == Tok::Percent).count() as i32;

    let mut v = value * 100f64.powi(percent_count) / 1000f64.powi(scale_commas);
    let mut exponent = 0i32;
    if exp_pos.is_some() && v != 0.0 {
        (v, exponent) = scientific_parts(v, int_phs.len(), frac_phs.len());
    }
    let prec = frac_phs.len();
    let raw = format!("{:.prec$}", round_half_away(v, prec));
    let (int_raw, frac_raw) = raw.split_once('.').unwrap_or((raw.as_str(), ""));
    let int_digits: Vec<char> = if int_raw == "0" {
        Vec::new()
    } else {
        int_raw.chars().collect()
    };

    let int_slots = layout_int_slots(&int_phs, &int_digits);
    let int_block = grouping.then(|| group_thousands(&int_slots.concat()));
    let exp_min_len = exp_pos.map_or(0, |e| {
        digit_placeholders(&section[e + 1..])
            .iter()
            .filter(|&&c| c == '0')
            .count()
    });
    let layout = NumberLayout {
        int_digits,
        int_slots,
        int_block,
        frac_slots: layout_frac_slots(frac_raw, &frac_phs),
        exponent,
        exp_min_len,
        dot_pos,
    };
    emit_number_section(section, &layout, value)
}

fn civil_from_serial_display(days: i64) -> (i64, i64, i64) {
    if days == 0 {
        (1900, 1, 0)
    } else {
        crate::functions_date_fin::civil_from_serial(days)
    }
}

/// Fraction-of-second digits requested by a `s.00`-style token pair (at most three).
fn fraction_digits(section: &[Tok]) -> usize {
    section
        .iter()
        .enumerate()
        .find_map(|(idx, t)| {
            let is_seconds = matches!(t, Tok::Second(_) | Tok::Elapsed('s', _));
            (is_seconds && section.get(idx + 1) == Some(&Tok::Dot)).then(|| {
                section[idx + 2..]
                    .iter()
                    .take_while(|t| **t == Tok::Digit('0'))
                    .count()
                    .min(3)
            })
        })
        .unwrap_or(0)
}

/// Broken-down date / time of a serial value used while rendering a date section.
struct DateParts {
    y: i64,
    mo: i64,
    d: i64,
    weekday: usize,
    hour: i64,
    minute: i64,
    second: i64,
    total_secs: i64,
    sub_units: i64,
    frac_digits: usize,
    has_ampm: bool,
}

impl DateParts {
    fn new(section: &[Tok], value: f64) -> Self {
        let frac_digits = fraction_digits(section);
        let unit = 10i64.pow(frac_digits as u32);
        let total_units = (value * 86_400.0 * unit as f64).round() as i64;
        let total_secs = total_units / unit;
        let days = total_secs / 86_400;
        let day_secs = total_secs % 86_400;
        let (y, mo, d) = civil_from_serial_display(days);
        Self {
            y,
            mo,
            d,
            weekday: ((days + 6) % 7) as usize,
            hour: day_secs / 3600,
            minute: (day_secs % 3600) / 60,
            second: day_secs % 60,
            total_secs,
            sub_units: total_units % unit,
            frac_digits,
            has_ampm: section.iter().any(|t| matches!(t, Tok::AmPm(..))),
        }
    }

    fn display_hour(&self) -> i64 {
        if !self.has_ampm {
            return self.hour;
        }
        match self.hour % 12 {
            0 => 12,
            h12 => h12,
        }
    }
}

fn push_year(out: &mut String, n: usize, y: i64) {
    if n <= 2 {
        out.push_str(&format!("{:02}", y % 100));
    } else {
        out.push_str(&format!("{y:04}"));
    }
}

fn push_month_name(out: &mut String, n: usize, mo: i64) {
    let name = MONTH_NAMES[((mo - 1).clamp(0, 11)) as usize];
    match n {
        1 | 2 => push_padded(out, mo, n == 2),
        3 => out.push_str(&name[..3]),
        4 => out.push_str(name),
        _ => out.push_str(&name[..1]),
    }
}

fn push_elapsed(out: &mut String, kind: char, width: usize, total_secs: i64) {
    let amount = match kind {
        'h' => total_secs / 3600,
        'm' => total_secs / 60,
        _ => total_secs,
    };
    let s = amount.to_string();
    for _ in s.len()..width {
        out.push('0');
    }
    out.push_str(&s);
}

fn push_ampm(out: &mut String, full: bool, lower: bool, hour: i64) {
    let text = match (full, hour >= 12) {
        (true, false) => "AM",
        (true, true) => "PM",
        (false, false) => "A",
        (false, true) => "P",
    };
    if lower {
        out.push_str(&text.to_ascii_lowercase());
    } else {
        out.push_str(text);
    }
}

fn format_date_section(section: &[Tok], value: f64) -> String {
    let parts = DateParts::new(section, value);
    let prev_sig = |idx: usize| {
        section[..idx]
            .iter()
            .rev()
            .find(|t| !matches!(t, Tok::Lit(_)))
    };
    let next_sig = |idx: usize| {
        section[idx + 1..]
            .iter()
            .find(|t| !matches!(t, Tok::Lit(_) | Tok::Dot | Tok::Digit(_)))
    };

    let mut out = String::new();
    let mut skip_frac = false;
    for (idx, t) in section.iter().enumerate() {
        match t {
            Tok::Year(n) => push_year(&mut out, *n, parts.y),
            Tok::Month(n) => {
                let is_minute = matches!(prev_sig(idx), Some(Tok::Hour(_) | Tok::Elapsed('h', _)))
                    || matches!(next_sig(idx), Some(Tok::Second(_) | Tok::Elapsed('s', _)));
                if is_minute {
                    push_padded(&mut out, parts.minute, *n >= 2);
                } else {
                    push_month_name(&mut out, *n, parts.mo);
                }
            }
            Tok::Day(n) => match n {
                1 | 2 => push_padded(&mut out, parts.d, *n == 2),
                3 => out.push_str(&DAY_NAMES[parts.weekday][..3]),
                _ => out.push_str(DAY_NAMES[parts.weekday]),
            },
            Tok::Hour(n) => push_padded(&mut out, parts.display_hour(), *n >= 2),
            Tok::Second(n) => {
                push_padded(&mut out, parts.second, *n >= 2);
                skip_frac = parts.frac_digits > 0;
            }
            Tok::Elapsed(kind, n) => {
                push_elapsed(&mut out, *kind, *n, parts.total_secs);
                skip_frac = *kind == 's' && parts.frac_digits > 0;
            }
            Tok::AmPm(full, lower) => push_ampm(&mut out, *full, *lower, parts.hour),
            Tok::Dot if skip_frac => {
                out.push('.');
                out.push_str(&format!(
                    "{:0width$}",
                    parts.sub_units,
                    width = parts.frac_digits
                ));
            }
            Tok::Digit('0') if skip_frac => {}
            Tok::Dot => out.push('.'),
            Tok::Digit(c) => out.push(*c),
            Tok::Comma => out.push(','),
            Tok::Percent => out.push('%'),
            Tok::Lit(s) => {
                skip_frac = false;
                out.push_str(s);
            }
            Tok::General | Tok::At | Tok::Exp(_) | Tok::Section => {}
        }
    }
    out
}

fn push_padded(out: &mut String, n: i64, pad: bool) {
    if pad {
        out.push_str(&format!("{n:02}"));
    } else {
        out.push_str(&n.to_string());
    }
}

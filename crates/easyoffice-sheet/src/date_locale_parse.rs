//! OS local wall-clock time (`TODAY`/`NOW`) and Korean-locale `DATEVALUE`/`TIMEVALUE` parsers
//! (PD-330).
//!
//! Uses direct OS time APIs (`localtime_r` on 64-bit Unix, `GetLocalTime` on Windows) without
//! extra crate dependencies, validates all FFI outputs, and falls back to UTC on failure.

use std::mem::MaybeUninit;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::functions_date_fin::{days_in_month, max_serial, serial_from_civil};

const SECONDS_PER_DAY: i64 = 86_400;
const FALLBACK_UNIX_SECS: u64 = 1_704_067_200;
/// Defensive byte ceiling (`128 KiB`, covering 32,767 UTF-8 chars) for untrusted
/// `DATEVALUE`/`TIMEVALUE`/date-text inputs.
pub(crate) const MAX_DATE_TIME_TEXT_BYTES: usize = 131_072;

#[inline]
fn valid_ymd_hms(
    y: i64,
    m: i64,
    d: i64,
    h: i64,
    min: i64,
    s: i64,
) -> Option<(i64, i64, i64, i64, i64, i64)> {
    if (1900..=9999).contains(&y)
        && (1..=12).contains(&m)
        && (1..=days_in_month(y, m)).contains(&d)
        && (0..=23).contains(&h)
        && (0..=59).contains(&min)
        && (0..=60).contains(&s)
    {
        Some((y, m, d, h, min, s.min(59)))
    } else {
        None
    }
}

fn utc_ymd_hms() -> (i64, i64, i64, i64, i64, i64) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(FALLBACK_UNIX_SECS);
    let unix_days = (secs / SECONDS_PER_DAY as u64) as i64;
    let day_secs = (secs % SECONDS_PER_DAY as u64) as i64;
    let z = unix_days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (
        year,
        m,
        d,
        day_secs / 3600,
        (day_secs % 3600) / 60,
        day_secs % 60,
    )
}

#[cfg(all(unix, target_pointer_width = "64"))]
fn try_local_ymd_hms() -> Option<(i64, i64, i64, i64, i64, i64)> {
    #[repr(C)]
    struct LibcTm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        _pad: [i64; 8],
    }

    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut LibcTm) -> *mut LibcTm;
    }

    let secs_u64 = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let time_t = i64::try_from(secs_u64).ok()?;
    let mut tm = MaybeUninit::<LibcTm>::zeroed();
    // SAFETY: `time_t` is a valid initialized `i64` (`time_t` on 64-bit Unix) and `tm` is an
    // adequately sized and padded `#[repr(C)]` `struct tm` buffer; we check the return pointer for
    // null before calling `assume_init`, and validate all field ranges afterwards.
    let ptr = unsafe { localtime_r(&time_t, tm.as_mut_ptr()) };
    if ptr.is_null() {
        return None;
    }
    let out = unsafe { tm.assume_init() };
    valid_ymd_hms(
        i64::from(out.tm_year).checked_add(1900)?,
        i64::from(out.tm_mon).checked_add(1)?,
        i64::from(out.tm_mday),
        i64::from(out.tm_hour),
        i64::from(out.tm_min),
        i64::from(out.tm_sec),
    )
}

#[cfg(windows)]
fn try_local_ymd_hms() -> Option<(i64, i64, i64, i64, i64, i64)> {
    #[repr(C)]
    struct SystemTimeWin {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(lp_system_time: *mut SystemTimeWin);
    }

    let mut st = MaybeUninit::<SystemTimeWin>::zeroed();
    // SAFETY: `st` points to a zero-initialized `SYSTEMTIME` buffer of exact Win32 ABI layout.
    unsafe { GetLocalTime(st.as_mut_ptr()) };
    let out = unsafe { st.assume_init() };
    valid_ymd_hms(
        i64::from(out.w_year),
        i64::from(out.w_month),
        i64::from(out.w_day),
        i64::from(out.w_hour),
        i64::from(out.w_minute),
        i64::from(out.w_second),
    )
}

#[cfg(not(any(all(unix, target_pointer_width = "64"), windows)))]
fn try_local_ymd_hms() -> Option<(i64, i64, i64, i64, i64, i64)> {
    None
}

/// Local `(year, month, day, hour, minute, second)`, falling back to UTC on platform failure.
pub(crate) fn local_ymd_hms() -> (i64, i64, i64, i64, i64, i64) {
    try_local_ymd_hms().unwrap_or_else(utc_ymd_hms)
}

#[inline]
fn parse_ascii_uint(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() || t.len() > 6 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

#[inline]
fn parse_ascii_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    if t.is_empty() || t.len() > 5 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

#[inline]
fn parse_nonneg_sec_allow_60(s: &str) -> Option<f64> {
    let t = s.trim();
    let bytes = t.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) || t.ends_with('.') {
        return None;
    }
    let mut dots = 0u8;
    for &b in bytes {
        if b == b'.' {
            dots += 1;
            if dots > 1 {
                return None;
            }
        } else if !b.is_ascii_digit() {
            return None;
        }
    }
    let val: f64 = t.parse().ok()?;
    (val.is_finite() && (0.0..=60.0).contains(&val)).then_some(val)
}

fn take_leading_uint(s: &str) -> Option<(i64, usize, &str)> {
    let trimmed = s.trim_start();
    let end = trimmed
        .bytes()
        .take_while(u8::is_ascii_digit)
        .take(7)
        .count();
    if end == 0 || end > 6 {
        return None;
    }
    let val = trimmed[..end].parse::<i64>().ok()?;
    Some((val, end, &trimmed[end..]))
}

fn parse_korean_year(s: &str) -> Option<i64> {
    let t = s.trim();
    if !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match t.len() {
        2 => {
            let yy: i64 = t.parse().ok()?;
            Some(if yy < 30 { 2000 + yy } else { 1900 + yy })
        }
        4 => t.parse().ok(),
        _ => None,
    }
}

fn parse_korean_date_prefix(s: &str) -> Option<((i64, i64, i64), &str, bool)> {
    let (y_part, after_y) = s.split_once('년')?;
    let (m_part, after_m) = after_y.split_once('월')?;
    let (d_part, after_d) = after_m.split_once('일')?;
    let y = parse_korean_year(y_part)?;
    let m = parse_ascii_uint(m_part)?;
    let d = parse_ascii_uint(d_part)?;
    if !after_d.is_empty() && !after_d.starts_with(char::is_whitespace) {
        return None;
    }
    Some(((y, m, d), after_d, false))
}

fn parse_sep_date_prefix(s: &str) -> Option<((i64, i64, i64), &str, bool)> {
    let (y, y_len, rest) = take_leading_uint(s)?;
    if y_len != 4 {
        return None;
    }
    let rest = rest.trim_start();
    let sep = rest
        .chars()
        .next()
        .filter(|c| matches!(c, '-' | '/' | '.'))?;
    let (m, m_len, rest) = take_leading_uint(&rest[1..])?;
    if !(1..=2).contains(&m_len) {
        return None;
    }
    let rest = rest.trim_start();
    if !rest.starts_with(sep) {
        return None;
    }
    let (d, d_len, mut after) = take_leading_uint(&rest[1..])?;
    if !(1..=2).contains(&d_len) {
        return None;
    }
    if sep == '.' {
        let trimmed_after = after.trim_start();
        if let Some(after_dot) = trimmed_after.strip_prefix('.') {
            after = after_dot;
        }
    }
    if sep == '-' && after.starts_with(['T', 't']) {
        let after_t = &after[1..];
        if after_t.is_empty() || after_t.starts_with(char::is_whitespace) {
            return None;
        }
        return Some(((y, m, d), after_t, true));
    }
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some(((y, m, d), after, false))
}

fn parse_date_prefix(s: &str) -> Option<(i64, &str, bool)> {
    let ((y, m, d), rest, must_have_time) = if s.contains('년') {
        parse_korean_date_prefix(s)?
    } else {
        parse_sep_date_prefix(s)?
    };
    if !(1900..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || !(1..=days_in_month(y, m)).contains(&d)
    {
        return None;
    }
    let serial = serial_from_civil(y, m, d);
    if !(0..=max_serial()).contains(&serial) {
        return None;
    }
    Some((serial, rest, must_have_time))
}

fn split_date_time_tail(s: &str, allow_hour_rollover: bool) -> Option<(i64, i64, f64)> {
    let (base_serial, rest, must_have_time) = parse_date_prefix(s)?;
    let tail = rest.trim();
    let (extra_days, frac) = if tail.is_empty() {
        if must_have_time {
            return None;
        }
        (0, 0.0)
    } else {
        parse_time_parts(tail, allow_hour_rollover)?
    };
    let serial = base_serial.checked_add(extra_days)?;
    if !(0..=max_serial()).contains(&serial) {
        return None;
    }
    Some((serial, extra_days, frac))
}

/// Parses ISO (`YYYY-MM-DD`, `YYYY/MM/DD`) and Korean (`2026년 3월 15일`, `26년 3월 15일`,
/// `2026. 3. 15.`, `2026.03.15`) date strings into the current date system's serial number,
/// rolling `24:00:00` and overflow colon time prefixes forward onto the calendar serial.
pub(crate) fn parse_date_serial_locale(s: &str) -> Option<f64> {
    if s.len() > MAX_DATE_TIME_TEXT_BYTES {
        return None;
    }
    let (serial, _, _) = split_date_time_tail(s.trim(), true)?;
    Some(serial as f64)
}

fn strip_prefix_ampm(s: &str) -> Option<(&str, bool, bool)> {
    for (tok, is_pm) in [("오전", false), ("오후", true)] {
        if let Some(rest) = s.strip_prefix(tok) {
            let has_gap = rest.starts_with(char::is_whitespace);
            return Some((rest.trim(), is_pm, has_gap));
        }
    }
    if let Some((head, tail)) = s.get(..2).zip(s.get(2..)) {
        if head.eq_ignore_ascii_case("AM") || head.eq_ignore_ascii_case("PM") {
            let has_gap = tail.starts_with(char::is_whitespace);
            if has_gap || tail.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                return Some((tail.trim(), head.eq_ignore_ascii_case("PM"), has_gap));
            }
        }
    }
    None
}

fn strip_suffix_ampm(s: &str) -> Option<(&str, bool, bool)> {
    for (tok, is_pm) in [("오전", false), ("오후", true)] {
        if let Some(head) = s.strip_suffix(tok) {
            let has_gap = head.chars().last().is_some_and(char::is_whitespace);
            return Some((head.trim(), is_pm, has_gap));
        }
    }
    let base = if let Some(without_dot) = s.strip_suffix('.') {
        let before_m_idx = without_dot.len().saturating_sub(2);
        let is_ampm = without_dot
            .get(before_m_idx..)
            .is_some_and(|t| t.eq_ignore_ascii_case("AM") || t.eq_ignore_ascii_case("PM"));
        let dot_before = before_m_idx > 0 && without_dot.as_bytes()[before_m_idx - 1] == b'.';
        if is_ampm && !dot_before {
            without_dot
        } else {
            s
        }
    } else {
        s
    };
    if base.len() >= 2 {
        let split = base.len() - 2;
        if base.is_char_boundary(split) {
            let (head, tail) = base.split_at(split);
            if tail.eq_ignore_ascii_case("AM") || tail.eq_ignore_ascii_case("PM") {
                let has_gap = head.chars().last().is_some_and(char::is_whitespace);
                return Some((head.trim(), tail.eq_ignore_ascii_case("PM"), has_gap));
            }
        }
    }
    None
}

fn parse_colon_time_body(body: &str) -> Option<(u32, u32, f64)> {
    let mut parts = body.split(':');
    let h = parse_ascii_u32(parts.next()?)?;
    let m = parse_ascii_u32(parts.next()?)?;
    let sec = match parts.next() {
        Some(sp) => parse_nonneg_sec_allow_60(sp)?,
        None => 0.0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((h, m, sec))
}

fn parse_korean_time_body(body: &str) -> Option<(u32, u32, f64)> {
    let (h_part, mut rest) = body.split_once('시')?;
    let h = parse_ascii_u32(h_part)?;
    rest = rest.trim();
    let m = if let Some((m_part, after_m)) = rest.split_once('분') {
        rest = after_m.trim();
        parse_ascii_u32(m_part)?
    } else {
        0
    };
    let sec = if let Some(s_part) = rest.strip_suffix('초') {
        rest = "";
        parse_nonneg_sec_allow_60(s_part)?
    } else {
        0.0
    };
    if !rest.is_empty() || sec >= 60.0 {
        return None;
    }
    Some((h, m, sec))
}

#[inline]
fn ampm_hour_24(h: u32, is_pm: bool) -> Option<u32> {
    (1..=12)
        .contains(&h)
        .then_some((h % 12) + if is_pm { 12 } else { 0 })
}

fn parse_time_parts(s: &str, allow_hour_rollover: bool) -> Option<(i64, f64)> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (body, ampm_info) = if let Some((rest, is_pm, gap)) = strip_prefix_ampm(t) {
        (rest, Some((is_pm, gap)))
    } else if let Some((rest, is_pm, gap)) = strip_suffix_ampm(t) {
        (rest, Some((is_pm, gap)))
    } else {
        (t, None)
    };
    if body.contains(':') {
        let (h, m, sec) = parse_colon_time_body(body)?;
        if let Some((is_pm, _)) = ampm_info {
            if m >= 60 || sec >= 60.0 {
                return None;
            }
            let h24 = ampm_hour_24(h, is_pm)?;
            let total = f64::from(h24) * 3600.0 + f64::from(m) * 60.0 + sec;
            return Some((0, total / SECONDS_PER_DAY as f64));
        }
        let standard = h <= 23 && m <= 59 && sec < 60.0;
        let boundary = (h == 24 && m <= 59 && sec < 60.0)
            || (h == 23 && m == 60 && sec < 60.0)
            || (h == 23 && m == 59 && sec == 60.0);
        let rollover = allow_hour_rollover && h <= 32_767 && m <= 59 && sec < 60.0;
        if !(standard || boundary || rollover) {
            return None;
        }
        let total = f64::from(h) * 3600.0 + f64::from(m) * 60.0 + sec;
        let day_f = SECONDS_PER_DAY as f64;
        let extra_days = (total / day_f).floor() as i64;
        let frac = total.rem_euclid(day_f) / day_f;
        return Some((extra_days, frac));
    }
    if body.contains('시') {
        let (h, m, sec) = parse_korean_time_body(body)?;
        if m >= 60 || sec >= 60.0 {
            return None;
        }
        let h24 = match ampm_info {
            Some((is_pm, _)) => ampm_hour_24(h, is_pm)?,
            None if h < 24 => h,
            None => return None,
        };
        let total = f64::from(h24) * 3600.0 + f64::from(m) * 60.0 + sec;
        return Some((0, total / SECONDS_PER_DAY as f64));
    }
    if let Some((is_pm, true)) = ampm_info {
        let h = parse_ascii_u32(body)?;
        let h24 = ampm_hour_24(h, is_pm)?;
        return Some((0, (f64::from(h24) * 3600.0) / SECONDS_PER_DAY as f64));
    }
    None
}

/// Parses standard `HH:MM[:SS] [AM|PM]`, single-hour `"2 PM"`, and Korean (`오전`/`오후`,
/// `9시 30분`, `14시 30분 5초`) time strings (with an optional leading date) into a day fraction
/// in `0.0..1.0`, normalizing `24:00:00`, `23:60:00`, and `23:59:60` to `0.0`.
pub(crate) fn parse_time_serial_locale(s: &str) -> Option<f64> {
    if s.len() > MAX_DATE_TIME_TEXT_BYTES {
        return None;
    }
    let t = s.trim();
    let tail = if let Some((_, rest, _)) = parse_date_prefix(t) {
        let after = rest.trim();
        if after.is_empty() {
            return None;
        }
        after
    } else {
        t
    };
    let (_, frac) = parse_time_parts(tail, false)?;
    Some(frac)
}

/// Parses a combined date (with optional rolled-over time) or a standalone time string in the
/// current workbook date system for `VALUE`, `DATEDIF`, and numeric coercion.
pub(crate) fn parse_datetime_or_time_locale(s: &str) -> Option<f64> {
    if s.len() > MAX_DATE_TIME_TEXT_BYTES {
        return None;
    }
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('년') || parse_sep_date_prefix(trimmed).is_some() {
        if let Some((serial, _, frac)) = split_date_time_tail(trimmed, true) {
            return Some(serial as f64 + frac);
        }
        if parse_date_prefix(trimmed).is_some() {
            return None;
        }
    }
    let (_, frac) = parse_time_parts(trimmed, false)?;
    Some(frac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_clock_and_utc_fallback_stay_in_calendar_bounds() {
        let (y, m, d, h, min, s) = local_ymd_hms();
        assert!((1900..=9999).contains(&y));
        assert!((1..=12).contains(&m));
        assert!((1..=days_in_month(y, m)).contains(&d));
        assert!((0..=23).contains(&h));
        assert!((0..=59).contains(&min));
        assert!((0..=59).contains(&s));

        let (uy, um, ud, uh, umin, us) = utc_ymd_hms();
        assert!(valid_ymd_hms(uy, um, ud, uh, umin, us).is_some());
    }

    #[test]
    fn hostile_and_multibyte_inputs_fail_closed_without_panic() {
        assert_eq!(
            parse_date_serial_locale(&format!(
                "2026-{}01-01",
                " ".repeat(MAX_DATE_TIME_TEXT_BYTES)
            )),
            None
        );
        assert_eq!(
            parse_time_serial_locale(&format!("4{}:35", " ".repeat(MAX_DATE_TIME_TEXT_BYTES))),
            None
        );
        assert_eq!(
            parse_datetime_or_time_locale(&"2026년 3월 15일 ".repeat(10_000)),
            None
        );
        for raw in [
            "",
            "오",
            "시",
            "9시",
            "9시x",
            "한글날짜",
            "am",
            "pm",
            "1시 2분 3초 4",
        ] {
            let _ = parse_date_serial_locale(raw);
            let _ = parse_time_serial_locale(raw);
            let _ = parse_datetime_or_time_locale(raw);
        }
        assert_eq!(parse_time_serial_locale("한글문자"), None);
        assert_eq!(parse_date_serial_locale("2026-+3-15"), None);
    }
}

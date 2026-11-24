//! OS local wall-clock time (`TODAY`/`NOW`) and Korean-locale `DATEVALUE`/`TIMEVALUE` parsers
//! (PD-330).
//!
//! Uses direct OS time APIs (`localtime_r` on 64-bit Unix, `GetLocalTime` on Windows) without
//! extra crate dependencies, validates all FFI outputs, and falls back to UTC on failure.

use std::mem::MaybeUninit;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::functions_date_fin::{days_in_month, serial_from_civil};

const SECONDS_PER_DAY: i64 = 86_400;
const FALLBACK_UNIX_SECS: u64 = 1_704_067_200;
/// Defensive byte ceiling for untrusted `DATEVALUE`/`TIMEVALUE` text.
const MAX_LOCALE_TEXT_BYTES: usize = 128;

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
    if t.is_empty() || t.len() > 4 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

#[inline]
fn parse_nonneg_sec(s: &str) -> Option<f64> {
    let t = s.trim();
    let first = t.as_bytes().first()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let val: f64 = t.parse().ok()?;
    (val.is_finite() && (0.0..60.0).contains(&val)).then_some(val)
}

fn take_leading_uint(s: &str) -> Option<(i64, &str)> {
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
    Some((val, &trimmed[end..]))
}

fn parse_korean_date_prefix(s: &str) -> Option<((i64, i64, i64), &str)> {
    let (y_part, after_y) = s.split_once('년')?;
    let (m_part, after_m) = after_y.split_once('월')?;
    let (d_part, after_d) = after_m.split_once('일')?;
    let y = parse_ascii_uint(y_part)?;
    let m = parse_ascii_uint(m_part)?;
    let d = parse_ascii_uint(d_part)?;
    if !after_d.is_empty() && !after_d.starts_with(char::is_whitespace) {
        return None;
    }
    Some(((y, m, d), after_d))
}

fn parse_sep_date_prefix(s: &str) -> Option<((i64, i64, i64), &str)> {
    let (y, rest) = take_leading_uint(s)?;
    let rest = rest.trim_start();
    let sep = rest
        .chars()
        .next()
        .filter(|c| matches!(c, '-' | '/' | '.'))?;
    let (m, rest) = take_leading_uint(&rest[1..])?;
    let rest = rest.trim_start();
    if !rest.starts_with(sep) {
        return None;
    }
    let (d, mut after) = take_leading_uint(&rest[1..])?;
    if sep == '.' {
        let trimmed_after = after.trim_start();
        if let Some(after_dot) = trimmed_after.strip_prefix('.') {
            after = after_dot;
        }
    }
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some(((y, m, d), after))
}

fn parse_date_prefix(s: &str) -> Option<((i64, i64, i64), &str)> {
    let ((y, m, d), rest) = if s.contains('년') {
        parse_korean_date_prefix(s)?
    } else {
        parse_sep_date_prefix(s)?
    };
    if (1900..=9999).contains(&y) && (1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d)
    {
        Some(((y, m, d), rest))
    } else {
        None
    }
}

/// Parses ISO (`YYYY-MM-DD`, `YYYY/MM/DD`) and Korean (`2026년 3월 15일`, `2026. 3. 15.`,
/// `2026.03.15`) date strings into the current date system's serial number.
pub(crate) fn parse_date_serial_locale(s: &str) -> Option<f64> {
    if s.len() > MAX_LOCALE_TEXT_BYTES {
        return None;
    }
    let trimmed = s.trim();
    let ((y, m, d), rest) = parse_date_prefix(trimmed)?;
    let tail = rest.trim();
    if !tail.is_empty() && parse_time_only(tail).is_none() {
        return None;
    }
    Some(serial_from_civil(y, m, d) as f64)
}

fn strip_prefix_ampm(s: &str) -> Option<(&str, bool)> {
    if let Some(rest) = s.strip_prefix("오전") {
        return Some((rest.trim(), false));
    }
    if let Some(rest) = s.strip_prefix("오후") {
        return Some((rest.trim(), true));
    }
    if let Some((head, tail)) = s.get(..2).zip(s.get(2..)) {
        if (head.eq_ignore_ascii_case("AM") || head.eq_ignore_ascii_case("PM"))
            && (tail.starts_with(char::is_whitespace)
                || tail.as_bytes().first().is_some_and(u8::is_ascii_digit))
        {
            return Some((tail.trim(), head.eq_ignore_ascii_case("PM")));
        }
    }
    None
}

fn strip_suffix_ampm(s: &str) -> Option<(&str, bool)> {
    if let Some(head) = s.strip_suffix("오전") {
        return Some((head.trim(), false));
    }
    if let Some(head) = s.strip_suffix("오후") {
        return Some((head.trim(), true));
    }
    if s.len() >= 2 {
        let split = s.len() - 2;
        if s.is_char_boundary(split) {
            let (head, tail) = s.split_at(split);
            if tail.eq_ignore_ascii_case("AM") {
                return Some((head.trim(), false));
            }
            if tail.eq_ignore_ascii_case("PM") {
                return Some((head.trim(), true));
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
        Some(sp) => parse_nonneg_sec(sp)?,
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
        parse_nonneg_sec(s_part)?
    } else {
        0.0
    };
    if !rest.is_empty() {
        return None;
    }
    Some((h, m, sec))
}

fn parse_time_only(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (body, ampm) = if let Some((rest, is_pm)) = strip_prefix_ampm(t) {
        (rest, Some(is_pm))
    } else if let Some((rest, is_pm)) = strip_suffix_ampm(t) {
        (rest, Some(is_pm))
    } else {
        (t, None)
    };
    let (mut h, m, sec) = if body.contains(':') {
        parse_colon_time_body(body)?
    } else if body.contains('시') {
        parse_korean_time_body(body)?
    } else {
        return None;
    };
    if m >= 60 || !sec.is_finite() || !(0.0..60.0).contains(&sec) {
        return None;
    }
    if let Some(is_pm) = ampm {
        if !(1..=12).contains(&h) {
            return None;
        }
        h = (h % 12) + if is_pm { 12 } else { 0 };
    } else if h >= 24 {
        return None;
    }
    Some((f64::from(h) * 3600.0 + f64::from(m) * 60.0 + sec) / SECONDS_PER_DAY as f64)
}

/// Parses standard `HH:MM[:SS] [AM|PM]` and Korean (`오전`/`오후`, `9시 30분`, `14시 30분 5초`)
/// time strings (with an optional leading date) into a day fraction in `0.0..1.0`.
pub(crate) fn parse_time_serial_locale(s: &str) -> Option<f64> {
    if s.len() > MAX_LOCALE_TEXT_BYTES {
        return None;
    }
    let mut t = s.trim();
    if let Some((_, rest)) = parse_date_prefix(t) {
        let tail = rest.trim();
        if tail.is_empty() {
            return None;
        }
        t = tail;
    }
    parse_time_only(t)
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
            parse_date_serial_locale(&"2026년 3월 15일 ".repeat(16)),
            None
        );
        assert_eq!(parse_time_serial_locale(&"오후 2시 15분 ".repeat(16)), None);
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
        }
        assert_eq!(parse_time_serial_locale("한글문자"), None);
        assert_eq!(parse_date_serial_locale("2026-+3-15"), None);
    }
}

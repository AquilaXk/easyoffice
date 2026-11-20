//! ISO 8601 dates and times as spreadsheet serial numbers: XLSX `t="d"` cells hold their value as
//! text (`2026-11-10`, `2026-11-10T12:00:00`, `12:00:00`) and the cell's number format shows it as a
//! date, so the cell must hold the serial the format applies to (PD-263).

/// Days from 1970-01-01 to the civil date (proleptic Gregorian), for `month` in `1..=12`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `n` digits at the start of `s` as a number, and the rest of `s`.
fn digits(s: &str, n: usize) -> Option<(i64, &str)> {
    let head = s.get(..n)?;
    if !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head.parse().ok()?, &s[n..]))
}

/// The day number of `YYYY-MM-DD` in the date system, `None` for a date the system cannot hold.
fn date_serial(text: &str, date1904: bool) -> Option<(i64, &str)> {
    let (year, rest) = digits(text, 4)?;
    let (month, rest) = digits(rest.strip_prefix('-')?, 2)?;
    let (day, rest) = digits(rest.strip_prefix('-')?, 2)?;
    if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if date1904 {
        let serial = days - days_from_civil(1904, 1, 1);
        return (serial >= 0).then_some((serial, rest));
    }
    // Serial 60 is the 1900-02-29 that never existed: dates before March 1900 sit one day lower.
    let diff = days - days_from_civil(1899, 12, 30);
    match diff {
        2..=60 => Some((diff - 1, rest)),
        61.. => Some((diff, rest)),
        _ => None,
    }
}

/// The fraction of a day of `hh:mm:ss[.fff]`, with a zone suffix (`Z`, `+09:00`) ignored.
fn day_fraction(text: &str) -> Option<f64> {
    let (hour, rest) = digits(text, 2)?;
    let (minute, rest) = digits(rest.strip_prefix(':')?, 2)?;
    let (second, rest) = digits(rest.strip_prefix(':')?, 2)?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let fraction = match rest.strip_prefix('.') {
        Some(tail) => {
            let end = tail.bytes().take_while(u8::is_ascii_digit).count();
            let frac: f64 = format!("0.{}", &tail[..end]).parse().ok()?;
            (end > 0).then_some(frac)?
        }
        None => 0.0,
    };
    Some(((hour * 3600 + minute * 60 + second) as f64 + fraction) / 86_400.0)
}

/// `text` as the serial number of the ISO 8601 date, date and time, or time it spells, written the
/// shortest way that reads back the same; `None` when it is not one.
pub(crate) fn iso_to_serial(text: &str, date1904: bool) -> Option<String> {
    let text = text.trim();
    if text.as_bytes().get(2) == Some(&b':') {
        return day_fraction(text).map(|f| f.to_string());
    }
    let (day, rest) = date_serial(text, date1904)?;
    if rest.is_empty() {
        return Some(day.to_string());
    }
    let time = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    Some((day as f64 + day_fraction(time)?).to_string())
}

#[cfg(test)]
mod tests {
    use super::iso_to_serial;

    #[test]
    fn dates_in_both_systems_and_around_the_missing_leap_day() {
        let serial = |t: &str, d1904| iso_to_serial(t, d1904);
        assert_eq!(serial("2026-11-10", false).as_deref(), Some("46336"));
        assert_eq!(serial("2026-11-10", true).as_deref(), Some("44874"));
        assert_eq!(serial("1900-01-01", false).as_deref(), Some("1"));
        assert_eq!(serial("1900-02-28", false).as_deref(), Some("59"));
        assert_eq!(serial("1900-03-01", false).as_deref(), Some("61"));
        assert_eq!(serial("1904-01-01", true).as_deref(), Some("0"));
        assert_eq!(serial("1903-12-31", true), None);
        assert_eq!(serial("1899-12-31", false), None);
    }

    #[test]
    fn times_and_rejects() {
        let serial = |t: &str| iso_to_serial(t, false);
        assert_eq!(serial("2026-11-10T12:00:00").as_deref(), Some("46336.5"));
        assert_eq!(serial("2026-11-10T06:00:00Z").as_deref(), Some("46336.25"));
        assert_eq!(serial("18:00:00").as_deref(), Some("0.75"));
        assert_eq!(serial("12:00:00.0").as_deref(), Some("0.5"));
        for bad in [
            "2026-02-30",
            "2026-13-01",
            "26-11-10",
            "2026-11-10X",
            "24:00:00",
            "",
            "not a date",
        ] {
            assert_eq!(serial(bad), None, "{bad}");
        }
    }
}

//! Date / time sections: serial to calendar fields, then token-by-token rendering with English
//! (en-US) names, or Korean names under `[$-412]`; `aaa` / `aaaa` are always Korean weekdays.

use std::fmt::Write;

use super::{FormattedText, Section, Tok};

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
const KO_DAY_SHORT: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];
const KO_DAY_SUFFIX: &str = "요일";
const KO_MONTH_SUFFIX: &str = "월";
const KO_AM: &str = "오전";
const KO_PM: &str = "오후";

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
    let _ = if n <= 2 {
        write!(out, "{:02}", y % 100)
    } else {
        write!(out, "{y:04}")
    };
}

fn push_month_name(out: &mut String, n: usize, mo: i64, korean: bool) {
    let idx = ((mo - 1).clamp(0, 11)) as usize;
    match n {
        1 | 2 => push_padded(out, mo, n == 2),
        _ if korean => {
            let _ = write!(out, "{}", idx + 1);
            if n != 5 {
                out.push_str(KO_MONTH_SUFFIX);
            }
        }
        3 => out.push_str(&MONTH_NAMES[idx][..3]),
        4 => out.push_str(MONTH_NAMES[idx]),
        _ => out.push_str(&MONTH_NAMES[idx][..1]),
    }
}

fn push_elapsed(out: &mut String, kind: char, width: usize, total_secs: i64) {
    let amount = match kind {
        'h' => total_secs / 3600,
        'm' => total_secs / 60,
        _ => total_secs,
    };
    let _ = write!(out, "{amount:0width$}");
}

fn push_ampm(out: &mut String, tok: (bool, bool), hour: i64, korean: bool) {
    let (full, lower) = tok;
    let pm = hour >= 12;
    if korean {
        out.push_str(if pm { KO_PM } else { KO_AM });
        return;
    }
    let text = match (full, pm) {
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

fn push_padded(out: &mut String, n: i64, pad: bool) {
    let _ = if pad {
        write!(out, "{n:02}")
    } else {
        write!(out, "{n}")
    };
}

pub(super) fn render(section: &Section, value: f64, out: &mut FormattedText) {
    let toks = &section.toks;
    let korean = section.korean;
    let parts = DateParts::new(toks, value);
    let prev_sig = |idx: usize| toks[..idx].iter().rev().find(|t| !matches!(t, Tok::Lit(_)));
    let next_sig = |idx: usize| {
        toks[idx + 1..]
            .iter()
            .find(|t| !matches!(t, Tok::Lit(_) | Tok::Dot | Tok::Digit(_)))
    };

    let mut fill = None;
    let text = &mut out.text;
    let mut skip_frac = false;
    for (idx, t) in toks.iter().enumerate() {
        match t {
            Tok::Year(n) => push_year(text, *n, parts.y),
            Tok::Month(n) => {
                let is_minute = matches!(prev_sig(idx), Some(Tok::Hour(_) | Tok::Elapsed('h', _)))
                    || matches!(next_sig(idx), Some(Tok::Second(_) | Tok::Elapsed('s', _)));
                if is_minute {
                    push_padded(text, parts.minute, *n >= 2);
                } else {
                    push_month_name(text, *n, parts.mo, korean);
                }
            }
            Tok::Day(n) => match n {
                1 | 2 => push_padded(text, parts.d, *n == 2),
                _ => push_day_name(text, *n, parts.weekday, korean),
            },
            Tok::KoDay(n) => push_day_name(text, *n, parts.weekday, true),
            Tok::Hour(n) => push_padded(text, parts.display_hour(), *n >= 2),
            Tok::Second(n) => {
                push_padded(text, parts.second, *n >= 2);
                skip_frac = parts.frac_digits > 0;
            }
            Tok::Elapsed(kind, n) => {
                push_elapsed(text, *kind, *n, parts.total_secs);
                skip_frac = *kind == 's' && parts.frac_digits > 0;
            }
            Tok::AmPm(full, lower) => push_ampm(text, (*full, *lower), parts.hour, korean),
            Tok::Dot if skip_frac => {
                let _ = write!(
                    text,
                    ".{:0width$}",
                    parts.sub_units,
                    width = parts.frac_digits
                );
            }
            Tok::Digit('0') if skip_frac => {}
            Tok::Dot => text.push('.'),
            Tok::Digit(c) => text.push(*c),
            Tok::Comma => text.push(','),
            Tok::Percent => text.push('%'),
            Tok::Slash => text.push('/'),
            Tok::Lit(s) => {
                skip_frac = false;
                text.push_str(s);
            }
            Tok::Fill(c) => {
                fill.get_or_insert((text.len(), *c));
            }
            _ => {}
        }
    }
    out.fill = fill;
}

/// `ddd` / `dddd` (English unless `korean`); `aaa` / `aaaa` always pass `korean`.
fn push_day_name(out: &mut String, n: usize, weekday: usize, korean: bool) {
    if korean {
        out.push_str(KO_DAY_SHORT[weekday]);
        if n >= 4 {
            out.push_str(KO_DAY_SUFFIX);
        }
    } else if n == 3 {
        out.push_str(&DAY_NAMES[weekday][..3]);
    } else {
        out.push_str(DAY_NAMES[weekday]);
    }
}

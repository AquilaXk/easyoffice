//! Number sections: digit placeholders, grouping, scaling, scientific notation and fractions.
//! The per-section analysis happens once at compile time ([`NumPlan`], [`FracPlan`]).

use super::{dbnum, FormattedText, Section, Tok, MAX_FRACTION_DIGITS};
use crate::coercion::format_general_number;

/// Largest denominator width (in placeholders) a fraction code may request.
const MAX_FRACTION_DEN_DIGITS: usize = 5;
/// Convergence threshold of the continued-fraction search (reference format-code `frac`).
const FRACTION_EPS: f64 = 0.000_000_05;
/// Iteration bound of the continued-fraction search (each step grows the denominator).
const MAX_FRACTION_STEPS: usize = 64;

pub(super) enum NumKind {
    General,
    Digits(NumPlan),
    Fraction(FracPlan),
}

pub(super) struct NumPlan {
    int_phs: Vec<char>,
    frac_phs: Vec<char>,
    grouping: bool,
    scale_commas: i32,
    percent_count: i32,
    exp_min_len: Option<usize>,
    dot_pos: Option<usize>,
}

pub(super) struct FracPlan {
    /// Index of the first numerator placeholder token.
    num_start: usize,
    /// Index of the `/` token.
    slash: usize,
    /// Index just past the denominator tokens.
    den_end: usize,
    int_phs: Vec<char>,
    num_phs: Vec<char>,
    den_phs: Vec<char>,
    fixed_den: Option<u32>,
}

pub(super) fn classify(section: &[Tok]) -> NumKind {
    if let Some(plan) = FracPlan::detect(section) {
        return NumKind::Fraction(plan);
    }
    if section.contains(&Tok::General) || !section.iter().any(|t| matches!(t, Tok::Digit(_))) {
        return NumKind::General;
    }
    NumKind::Digits(NumPlan::new(section))
}

pub(super) fn render(section: &Section, kind: &NumKind, value: f64, out: &mut FormattedText) {
    match kind {
        NumKind::General => render_general(section, value, out),
        NumKind::Digits(plan) => render_digits(section, plan, value, out),
        NumKind::Fraction(plan) => render_fraction(section, plan, value, out),
    }
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

/// Appends `s` with ASCII digits replaced by the section's `DBNum` glyphs (no-op without one).
fn push_digits(out: &mut FormattedText, s: &str, dbnum: Option<u8>) {
    match dbnum {
        Some(kind) => out.push_str(&dbnum::map_digits(s, kind)),
        None => out.push_str(s),
    }
}

/// `General` text; `DBNum` kinds spell the integer part and map the fraction digits.
fn push_general(out: &mut FormattedText, value: f64, dbnum: Option<u8>) {
    let text = format_general_number(value);
    match dbnum {
        Some(kind) => out.push_str(&dbnum::number_text(&text, kind)),
        None => out.push_str(&text),
    }
}

fn render_general(section: &Section, value: f64, out: &mut FormattedText) {
    for t in &section.toks {
        match t {
            Tok::General | Tok::At => push_general(out, value, section.dbnum),
            Tok::Lit(s) => out.push_str(s),
            Tok::Percent => out.push('%'),
            Tok::Dot => out.push('.'),
            Tok::Comma => out.push(','),
            Tok::Slash => out.push('/'),
            Tok::Fill(c) => out.set_fill(*c),
            _ => {}
        }
    }
}

impl NumPlan {
    fn new(section: &[Tok]) -> Self {
        let first_digit = section
            .iter()
            .position(|t| matches!(t, Tok::Digit(_)))
            .unwrap_or(0);
        let last_digit = section
            .iter()
            .rposition(|t| matches!(t, Tok::Digit(_)))
            .unwrap_or(0);
        let exp_pos = section.iter().position(|t| matches!(t, Tok::Exp(_)));
        let mantissa_end = exp_pos.unwrap_or(last_digit + 1);
        let dot_pos = section[..mantissa_end].iter().position(|t| *t == Tok::Dot);
        let int_end = dot_pos.unwrap_or(mantissa_end);
        let scan_end = exp_pos.unwrap_or(section.len());
        let frac_phs = dot_pos.map_or_else(Vec::new, |d| {
            let mut phs = digit_placeholders(&section[d + 1..mantissa_end]);
            phs.truncate(MAX_FRACTION_DIGITS);
            phs
        });
        let (grouping, scale_commas) = classify_commas(section, first_digit, scan_end, dot_pos);
        Self {
            int_phs: digit_placeholders(&section[..int_end]),
            frac_phs,
            grouping,
            scale_commas,
            percent_count: section.iter().filter(|t| **t == Tok::Percent).count() as i32,
            exp_min_len: exp_pos.map(|e| {
                digit_placeholders(&section[e + 1..])
                    .iter()
                    .filter(|&&c| c == '0')
                    .count()
            }),
            dot_pos,
        }
    }
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

/// The character the `p`-th integer placeholder (of `phs.len()`) shows for `digits`
/// (right-aligned ASCII digits); `None` for an unused `#`.
fn int_slot(phs: &[char], digits: &str, p: usize) -> Option<char> {
    let from_right = phs.len() - 1 - p;
    if from_right < digits.len() {
        Some(digits.as_bytes()[digits.len() - 1 - from_right] as char)
    } else {
        pad_char(phs[p])
    }
}

/// Digits beyond the placeholder count, shown in front of the first placeholder.
fn int_overflow<'a>(phs: &[char], digits: &'a str) -> &'a str {
    &digits[..digits.len().saturating_sub(phs.len())]
}

fn push_exponent(out: &mut FormattedText, plus: bool, exponent: i32, min_len: usize) {
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

/// Digits of one rendered number, shared by the emit loop.
struct Layout<'a> {
    int_digits: &'a str,
    int_block: Option<String>,
    frac_raw: &'a str,
    frac_cut: usize,
    exponent: i32,
}

fn render_digits(section: &Section, plan: &NumPlan, value: f64, out: &mut FormattedText) {
    let prec = plan.frac_phs.len();
    let mut v = value * 100f64.powi(plan.percent_count) / 1000f64.powi(plan.scale_commas);
    let mut exponent = 0i32;
    if plan.exp_min_len.is_some() && v != 0.0 {
        (v, exponent) = scientific_parts(v, plan.int_phs.len(), prec);
    }
    let raw = format!("{:.prec$}", round_half_away(v, prec));
    let (int_raw, frac_raw) = raw.split_once('.').unwrap_or((raw.as_str(), ""));
    let int_digits = if int_raw == "0" { "" } else { int_raw };
    let mut frac_cut = prec;
    while frac_cut > 0
        && plan.frac_phs[frac_cut - 1] != '0'
        && frac_raw.as_bytes()[frac_cut - 1] == b'0'
    {
        frac_cut -= 1;
    }
    let int_block = int_block(section, plan, int_digits);
    let layout = Layout {
        int_digits,
        int_block,
        frac_raw,
        frac_cut,
        exponent,
    };
    emit_digits(section, plan, &layout, value, out);
}

/// The whole integer part as one string when it cannot be emitted placeholder by placeholder:
/// thousands grouping, or unit-based `DBNum` numerals.
fn int_block(section: &Section, plan: &NumPlan, int_digits: &str) -> Option<String> {
    if let Some(kind) = section.dbnum.filter(|k| *k != dbnum::FULL_WIDTH) {
        let digits = if int_digits.is_empty() && plan.int_phs.contains(&'0') {
            "0"
        } else {
            int_digits
        };
        return Some(dbnum::integer(digits, kind));
    }
    if !plan.grouping {
        return None;
    }
    let mut slots = String::with_capacity(int_digits.len() + plan.int_phs.len());
    slots.push_str(int_overflow(&plan.int_phs, int_digits));
    (0..plan.int_phs.len())
        .filter_map(|p| int_slot(&plan.int_phs, int_digits, p))
        .for_each(|c| slots.push(c));
    Some(group_thousands(&slots))
}

fn emit_digits(
    section: &Section,
    plan: &NumPlan,
    lay: &Layout<'_>,
    value: f64,
    out: &mut FormattedText,
) {
    let dbnum = section.dbnum;
    let mut int_idx = 0usize;
    let mut frac_idx = 0usize;
    let mut in_exponent = false;
    for (idx, t) in section.toks.iter().enumerate() {
        match t {
            Tok::Digit(_) if in_exponent => {}
            Tok::Digit(_) if plan.dot_pos.is_none_or(|d| idx < d) => {
                emit_int_placeholder(plan, lay, int_idx, dbnum, out);
                int_idx += 1;
            }
            Tok::Digit(_) => {
                if frac_idx < lay.frac_cut {
                    push_digits(out, &lay.frac_raw[frac_idx..=frac_idx], dbnum);
                } else if let Some(c) = plan.frac_phs.get(frac_idx).copied().and_then(pad_char) {
                    out.push(c);
                }
                frac_idx += 1;
            }
            Tok::Dot if Some(idx) == plan.dot_pos => {
                if plan.int_phs.is_empty() {
                    push_digits(out, lay.int_digits, dbnum);
                }
                out.push('.');
            }
            Tok::Dot => out.push('.'),
            Tok::Percent => out.push('%'),
            Tok::Slash => out.push('/'),
            Tok::Lit(s) => out.push_str(s),
            Tok::Fill(c) => out.set_fill(*c),
            Tok::Exp(plus) => {
                in_exponent = true;
                push_exponent(out, *plus, lay.exponent, plan.exp_min_len.unwrap_or(0));
            }
            Tok::General | Tok::At => push_general(out, value, dbnum),
            _ => {}
        }
    }
}

fn emit_int_placeholder(
    plan: &NumPlan,
    lay: &Layout<'_>,
    p: usize,
    dbnum: Option<u8>,
    out: &mut FormattedText,
) {
    if let Some(block) = &lay.int_block {
        if p == 0 {
            out.push_str(block);
        }
        return;
    }
    if p >= plan.int_phs.len() {
        return;
    }
    if p == 0 {
        push_digits(out, int_overflow(&plan.int_phs, lay.int_digits), dbnum);
    }
    if let Some(c) = int_slot(&plan.int_phs, lay.int_digits, p) {
        let mut buf = [0u8; 4];
        push_digits(out, c.encode_utf8(&mut buf), dbnum);
    }
}

impl FracPlan {
    /// Recognizes `[int ]num/den` placeholder runs around the first `/` (denominator either
    /// placeholders such as `??` or a literal integer such as `16`).
    fn detect(section: &[Tok]) -> Option<Self> {
        let slash = section.iter().position(|t| *t == Tok::Slash)?;
        let num_start = section[..slash]
            .iter()
            .rposition(|t| !matches!(t, Tok::Digit(_)))
            .map_or(0, |i| i + 1);
        if num_start == slash {
            return None;
        }
        let rest = &section[slash + 1..];
        let den_placeholders = rest
            .iter()
            .take_while(|t| matches!(t, Tok::Digit(_)))
            .count();
        let (den_len, fixed_den) = if den_placeholders > 0 {
            (den_placeholders, None)
        } else {
            let lits: String = rest
                .iter()
                .map_while(|t| match t {
                    Tok::Lit(s) if s.len() == 1 && s.as_bytes()[0].is_ascii_digit() => {
                        Some(s.as_str())
                    }
                    _ => None,
                })
                .collect();
            let den: u32 = lits.parse().ok().filter(|d| (1..100_000).contains(d))?;
            (lits.len(), Some(den))
        };
        Some(Self {
            num_start,
            slash,
            den_end: slash + 1 + den_len,
            int_phs: digit_placeholders(&section[..num_start]),
            num_phs: digit_placeholders(&section[num_start..slash]),
            den_phs: digit_placeholders(&section[slash + 1..slash + 1 + den_len])
                .into_iter()
                .take(MAX_FRACTION_DEN_DIGITS)
                .collect(),
            fixed_den,
        })
    }

    fn max_den(&self) -> u32 {
        10u32.pow(self.den_phs.len().min(MAX_FRACTION_DEN_DIGITS) as u32) - 1
    }
}

/// Best rational approximation `(numerator, denominator)` of `x >= 0` with denominator at most
/// `max_den`: continued-fraction convergents (reference format-code `frac`), plus the closest
/// semiconvergent when the next convergent would exceed the bound.
fn best_fraction(x: f64, max_den: u32) -> (u64, u64) {
    let max_den = u64::from(max_den);
    let (mut p2, mut q2) = (0u64, 1u64);
    let (mut p1, mut q1) = (1u64, 0u64);
    let mut b = x;
    for _ in 0..MAX_FRACTION_STEPS {
        let a = b.floor();
        let step = a as u64;
        let p = step.saturating_mul(p1).saturating_add(p2);
        let q = step.saturating_mul(q1).saturating_add(q2);
        if q > max_den {
            let k = (max_den - q2) / q1.max(1);
            let semi = (k * p1 + p2, k * q1 + q2);
            let error = |(p, q): (u64, u64)| (x - p as f64 / q as f64).abs();
            return if k > 0 && error(semi) < error((p1, q1)) {
                semi
            } else {
                (p1, q1)
            };
        }
        (p2, q2, p1, q1) = (p1, q1, p, q);
        let rest = b - a;
        if rest < FRACTION_EPS {
            break;
        }
        b = 1.0 / rest;
    }
    (p1, q1.max(1))
}

fn render_fraction(section: &Section, plan: &FracPlan, value: f64, out: &mut FormattedText) {
    let mixed = !plan.int_phs.is_empty();
    let (whole, numer, denom) = match plan.fixed_den {
        Some(den) => {
            let scaled = (value * f64::from(den)).round() as u64;
            split_fraction(scaled, u64::from(den), mixed)
        }
        None => {
            let (p, q) = best_fraction(value, plan.max_den());
            split_fraction(p, q, mixed)
        }
    };
    let whole_digits = whole.to_string();
    let whole_text = if whole == 0 && numer != 0 {
        ""
    } else {
        whole_digits.as_str()
    };
    let mut int_idx = 0usize;
    for (idx, t) in section.toks.iter().enumerate() {
        match t {
            Tok::Digit(_) if idx < plan.num_start => {
                if int_idx == 0 {
                    out.push_str(int_overflow(&plan.int_phs, whole_text));
                }
                if let Some(c) = int_slot(&plan.int_phs, whole_text, int_idx) {
                    out.push(c);
                }
                int_idx += 1;
            }
            Tok::Digit(_) if idx < plan.slash => {
                if idx == plan.num_start {
                    push_padded(out, numer, &plan.num_phs, true, mixed && numer == 0);
                }
            }
            Tok::Slash if idx == plan.slash => {
                out.push(if mixed && numer == 0 { ' ' } else { '/' });
            }
            Tok::Digit(_) | Tok::Lit(_) if idx > plan.slash && idx < plan.den_end => {
                if idx == plan.slash + 1 {
                    push_den(out, plan, denom, mixed && numer == 0);
                }
            }
            Tok::Lit(s) => out.push_str(s),
            Tok::Slash => out.push('/'),
            Tok::Dot => out.push('.'),
            Tok::Comma | Tok::Percent => {}
            Tok::Fill(c) => out.set_fill(*c),
            _ => {}
        }
    }
}

/// `(whole, numerator, denominator)` of `p / q`; improper when the code has no integer part.
fn split_fraction(p: u64, q: u64, mixed: bool) -> (u64, u64, u64) {
    if mixed {
        (p / q, p % q, q)
    } else {
        (0, p, q)
    }
}

/// Numerator right-aligned / denominator left-aligned in their placeholder widths.
fn push_padded(out: &mut FormattedText, n: u64, phs: &[char], right: bool, blank: bool) {
    let digits = n.to_string();
    let pad = phs.len().saturating_sub(digits.len());
    let pad_one = |out: &mut FormattedText, ph: char| {
        if let Some(c) = pad_char(ph) {
            out.push(if blank && ph == '?' { ' ' } else { c });
        }
    };
    if blank {
        phs.iter().for_each(|&ph| pad_one(out, ph));
        return;
    }
    if right {
        phs[..pad].iter().for_each(|&ph| pad_one(out, ph));
        out.push_str(&digits);
    } else {
        out.push_str(&digits);
        phs[digits.len().min(phs.len())..]
            .iter()
            .for_each(|&ph| pad_one(out, ph));
    }
}

fn push_den(out: &mut FormattedText, plan: &FracPlan, denom: u64, blank: bool) {
    if plan.fixed_den.is_some() {
        let digits = denom.to_string();
        if blank {
            digits.chars().for_each(|_| out.push(' '));
        } else {
            out.push_str(&digits);
        }
        return;
    }
    push_padded(out, denom, &plan.den_phs, false, blank);
}

//! Spreadsheet-compatible number format codes for the `TEXT` worksheet function and cell display.
//!
//! Supports literal text (quoted, backslash-escaped and bare characters such as `원`, `₩`, `$`),
//! `0`/`#`/`?` digit placeholders with zero padding, thousands grouping and `,` scaling, `%`,
//! scientific `E+`/`E-`, fractions (`# ?/?`, `?/16`), up to four `;` sections
//! (positive;negative;zero;text) with `[>100]`-style conditions, `[Red]`/`[Color12]` colors,
//! `@` text substitution, `General`, date/time codes (`yyyy`, `m`..`mmmmm`, `d`..`dddd`, `aaa`,
//! `aaaa`, `h`/`hh`, `ss`, fractional seconds, `AM/PM`, `A/P`), elapsed `[h]`, `[m]`, `[s]`,
//! `[$sym-LCID]` locales (Korean `412` gives `오전/오후`, `1월`, `월요일`), `[DBNum1-4]`
//! numerals, `_x` width spacing and `*x` fill. Month and weekday names are English (en-US)
//! unless the Korean locale or `aaa`/`aaaa` is used.
//!
//! A code is parsed once into a [`CompiledFormat`] (thread-local cache keyed by the code text);
//! rendering then only walks pre-classified sections.
//!
//! Reference implementations (ported logic, not code): SheetJS SSF `format.js` (Apache-2.0,
//! Copyright (C) 2012-present SheetJS LLC) for section choice with conditions and the
//! continued-fraction `frac`; IronCalc `base/src/formatter` (MIT OR Apache-2.0) for the
//! section/token model. Korean names and `DBNum` glyph tables are written directly.

mod date;
mod dbnum;
mod num;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::coercion::format_general_number;
use crate::types::FormulaError;

use num::NumKind;

const MAX_FRACTION_DIGITS: usize = 10;
const MAX_DATE_SERIAL: f64 = 2_958_466.0;
const MAX_CACHED_FORMATS: usize = 256;
const KOREAN_LCID: u32 = 0x412;
const LCID_MASK: u32 = 0xFFFF;
const MAX_COLOR_INDEX: u32 = 56;
const MAX_DBNUM: u32 = 4;

/// The spreadsheet 56-color palette addressed by `[Color1]`..`[Color56]` (0xRRGGBB).
const PALETTE: [u32; 56] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000,
    0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366,
    0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00,
    0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99,
    0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900,
    0xFF6600, 0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366,
    0x333399, 0x333333,
];
const NAMED_COLORS: [(&str, u32); 8] = [
    ("black", 0x000000),
    ("blue", 0x0000FF),
    ("cyan", 0x00FFFF),
    ("green", 0x00FF00),
    ("magenta", 0xFF00FF),
    ("red", 0xFF0000),
    ("white", 0xFFFFFF),
    ("yellow", 0xFFFF00),
];

/// Section color (`[Red]`, `[Color5]`) as `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatColor(pub u32);

impl FormatColor {
    /// `#rrggbb` CSS notation.
    pub fn hex(self) -> String {
        format!("#{:06x}", self.0)
    }
}

/// Result of rendering a value: display text, section color and the `*x` fill request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormattedText {
    pub text: String,
    pub color: Option<FormatColor>,
    /// Byte offset in `text` and the character a `*x` code asks to repeat over the free width.
    pub fill: Option<(usize, char)>,
}

impl FormattedText {
    fn push(&mut self, c: char) {
        self.text.push(c);
    }

    fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
    }

    fn set_fill(&mut self, c: char) {
        if self.fill.is_none() {
            self.fill = Some((self.text.len(), c));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CondOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// `[>100]`-style section condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cond {
    op: CondOp,
    rhs: f64,
}

impl Cond {
    fn matches(self, v: f64) -> bool {
        match self.op {
            CondOp::Lt => v < self.rhs,
            CondOp::Le => v <= self.rhs,
            CondOp::Gt => v > self.rhs,
            CondOp::Ge => v >= self.rhs,
            CondOp::Eq => v == self.rhs,
            CondOp::Ne => v != self.rhs,
        }
    }

    fn parse(content: &str) -> Option<Self> {
        let (op, rest) = ["<=", ">=", "<>", "<", ">", "="]
            .iter()
            .find_map(|p| content.strip_prefix(p).map(|r| (*p, r)))?;
        let op = match op {
            "<=" => CondOp::Le,
            ">=" => CondOp::Ge,
            "<>" => CondOp::Ne,
            "<" => CondOp::Lt,
            ">" => CondOp::Gt,
            _ => CondOp::Eq,
        };
        let rhs: f64 = rest.trim().parse().ok().filter(|v: &f64| v.is_finite())?;
        Some(Self { op, rhs })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Lit(String),
    Digit(char),
    Dot,
    Comma,
    Percent,
    At,
    General,
    Exp(bool),
    Slash,
    Year(usize),
    Month(usize),
    Day(usize),
    /// `aaa` / `aaaa`: Korean weekday name (short / long).
    KoDay(usize),
    Hour(usize),
    Second(usize),
    AmPm(bool, bool),
    Elapsed(char, usize),
    Fill(char),
    Color(FormatColor),
    Cond(Cond),
    DbNum(u8),
    Locale(u32),
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

fn parse_locale(text: &str) -> Option<u32> {
    if text.eq_ignore_ascii_case("ko-kr") || text.eq_ignore_ascii_case("ko") {
        return Some(KOREAN_LCID);
    }
    u32::from_str_radix(text, 16).ok()
}

fn parse_color(lower: &str) -> Option<FormatColor> {
    if let Some((_, rgb)) = NAMED_COLORS.iter().find(|(name, _)| *name == lower) {
        return Some(FormatColor(*rgb));
    }
    let index: u32 = lower.strip_prefix("color")?.parse().ok()?;
    (1..=MAX_COLOR_INDEX)
        .contains(&index)
        .then(|| FormatColor(PALETTE[index as usize - 1]))
}

/// Translates the content of a `[...]` group into tokens; unknown groups are dropped.
fn push_bracket(content: &str, toks: &mut Vec<Tok>) {
    let lower = content.to_ascii_lowercase();
    for code in ['h', 'm', 's'] {
        if !lower.is_empty() && lower.chars().all(|c| c == code) {
            toks.push(Tok::Elapsed(code, lower.len()));
            return;
        }
    }
    if let Some(rest) = content.strip_prefix('$') {
        let (symbol, locale) = rest.split_once('-').unwrap_or((rest, ""));
        if !symbol.is_empty() {
            toks.push(Tok::Lit(symbol.to_string()));
        }
        if let Some(lcid) = parse_locale(locale) {
            toks.push(Tok::Locale(lcid));
        }
    } else if let Some(color) = parse_color(&lower) {
        toks.push(Tok::Color(color));
    } else if let Some(cond) = Cond::parse(content) {
        toks.push(Tok::Cond(cond));
    } else if let Some(kind) = lower
        .strip_prefix("dbnum")
        .and_then(|d| d.parse::<u32>().ok())
        .filter(|k| (1..=MAX_DBNUM).contains(k))
    {
        toks.push(Tok::DbNum(kind as u8));
    }
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
                if let Some(&next) = chars.get(i + 1) {
                    toks.push(Tok::Fill(next));
                }
                i += 2;
                continue;
            }
            '[' => {
                let close = chars[i + 1..].iter().position(|&c| c == ']');
                if let Some(off) = close {
                    let content: String = chars[i + 1..i + 1 + off].iter().collect();
                    push_bracket(&content, &mut toks);
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
            '/' => toks.push(Tok::Slash),
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
            'a' | 'A' if starts_with_ci(&chars, i, "aa") => {
                let n = run_len(&chars, i, 'a');
                if n >= 3 {
                    toks.push(Tok::KoDay(n));
                } else {
                    toks.push(Tok::Lit(chars[i..i + n].iter().collect()));
                }
                i += n;
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
            | Tok::KoDay(_)
            | Tok::Hour(_)
            | Tok::Second(_)
            | Tok::AmPm(..)
            | Tok::Elapsed(..)
    )
}

enum SectionKind {
    Date,
    Number(NumKind),
}

/// One `;`-separated section with its attributes and pre-computed layout.
struct Section {
    toks: Vec<Tok>,
    color: Option<FormatColor>,
    cond: Option<Cond>,
    dbnum: Option<u8>,
    korean: bool,
    has_text: bool,
    kind: SectionKind,
}

impl Section {
    fn new(raw: Vec<Tok>) -> Self {
        let mut section = Self {
            toks: Vec::with_capacity(raw.len()),
            color: None,
            cond: None,
            dbnum: None,
            korean: false,
            has_text: false,
            kind: SectionKind::Date,
        };
        for tok in raw {
            match tok {
                Tok::Color(c) => section.color = section.color.or(Some(c)),
                Tok::Cond(c) => section.cond = section.cond.or(Some(c)),
                Tok::DbNum(k) => section.dbnum = Some(k),
                Tok::Locale(l) => section.korean |= l & LCID_MASK == KOREAN_LCID,
                other => section.toks.push(other),
            }
        }
        section.has_text = section.toks.contains(&Tok::At);
        section.kind = if section.toks.iter().any(is_date_tok) {
            SectionKind::Date
        } else {
            SectionKind::Number(num::classify(&section.toks))
        };
        section
    }
}

/// A parsed format code, reusable for any number of values.
pub struct CompiledFormat {
    sections: Vec<Section>,
    /// Index of the text section (`@`), when the code has one.
    text_idx: Option<usize>,
    /// Number of sections that apply to numbers (the text section excluded).
    numeric_count: usize,
}

impl CompiledFormat {
    pub fn compile(fmt: &str) -> Self {
        let mut raw_sections = vec![Vec::new()];
        for t in tokenize(fmt) {
            if t == Tok::Section {
                raw_sections.push(Vec::new());
            } else if let Some(last) = raw_sections.last_mut() {
                last.push(t);
            }
        }
        let sections: Vec<Section> = raw_sections.into_iter().map(Section::new).collect();
        let text_idx = if sections.len() >= 4 {
            Some(3)
        } else {
            sections.iter().position(|s| s.has_text)
        };
        let numeric_count = match text_idx {
            Some(i) if i + 1 == sections.len() && i > 0 => i,
            _ => sections.len(),
        };
        Self {
            sections,
            text_idx,
            numeric_count,
        }
    }

    /// `(section index, show the absolute value)` for `num`. Conditions follow the reference format-code implementation (see the file header):
    /// the first matching explicit condition wins, otherwise the default positive / negative /
    /// zero split applies.
    fn pick_numeric(&self, num: f64) -> (usize, bool) {
        let count = self.numeric_count;
        let c0 = self.sections[0].cond;
        let c1 = self
            .sections
            .get(1)
            .filter(|_| count > 1)
            .and_then(|s| s.cond);
        if c0.is_some() || c1.is_some() {
            // The second of several sections prints negatives without a minus sign.
            let idx = self.pick_conditional(num, c0, c1);
            return (idx, idx == 1);
        }
        match count {
            1 => (0, false),
            2 => {
                if num < 0.0 {
                    (1, true)
                } else {
                    (0, false)
                }
            }
            _ => {
                if num < 0.0 {
                    (1, true)
                } else if num == 0.0 {
                    (2, false)
                } else {
                    (0, false)
                }
            }
        }
    }

    fn pick_conditional(&self, num: f64, c0: Option<Cond>, c1: Option<Cond>) -> usize {
        let count = self.numeric_count;
        if c0.is_some_and(|c| c.matches(num)) {
            return 0;
        }
        if count < 2 {
            return 0;
        }
        match (c0, c1) {
            (_, Some(c)) if c.matches(num) => 1,
            (Some(_), Some(_)) if count > 2 => 2,
            (Some(_), Some(_)) => 1,
            (Some(_), None) if count > 2 && num >= 0.0 => 2,
            (Some(_), None) => 1,
            (None, _) if count > 2 && num <= 0.0 => 2,
            (None, _) => 0,
        }
    }

    /// Renders a number; date/time codes reject serials outside `0..=2958465.99999` with `#VALUE!`.
    pub fn render_number(&self, num: f64) -> Result<FormattedText, FormulaError> {
        if !num.is_finite() {
            return Err(FormulaError::Num);
        }
        let (idx, abs) = self.pick_numeric(num);
        let section = &self.sections[idx];
        let mut out = FormattedText {
            color: section.color,
            ..FormattedText::default()
        };
        let value = if abs { num.abs() } else { num };
        match &section.kind {
            SectionKind::Date => {
                if !(0.0..MAX_DATE_SERIAL).contains(&value) {
                    return Err(FormulaError::Value);
                }
                date::render(section, value, &mut out);
            }
            SectionKind::Number(kind) => {
                if value < 0.0 {
                    out.push('-');
                }
                num::render(section, kind, value.abs(), &mut out);
            }
        }
        Ok(out)
    }

    /// Applies the text section (`@`) to a text value; without one the text is unchanged.
    pub fn render_text(&self, text: &str) -> FormattedText {
        let Some(section) = self.text_idx.map(|i| &self.sections[i]) else {
            return FormattedText {
                text: text.to_string(),
                ..FormattedText::default()
            };
        };
        let mut out = FormattedText {
            color: section.color,
            ..FormattedText::default()
        };
        for t in &section.toks {
            match t {
                Tok::At => out.push_str(text),
                Tok::Lit(s) => out.push_str(s),
                Tok::Fill(c) => out.set_fill(*c),
                _ => {}
            }
        }
        out
    }
}

thread_local! {
    static FORMAT_CACHE: RefCell<HashMap<String, Rc<CompiledFormat>>> = RefCell::new(HashMap::new());
}

/// Parsed form of `fmt`, compiled once per distinct code (bounded thread-local cache).
pub fn compiled_format(fmt: &str) -> Rc<CompiledFormat> {
    FORMAT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.get(fmt) {
            return Rc::clone(hit);
        }
        if cache.len() >= MAX_CACHED_FORMATS {
            cache.clear();
        }
        let compiled = Rc::new(CompiledFormat::compile(fmt));
        cache.insert(fmt.to_string(), Rc::clone(&compiled));
        compiled
    })
}

/// Formats `num` with a spreadsheet number format code. Invalid date serials fall back to
/// `General` so callers that need a plain string never fail.
pub fn format_text_pattern(num: f64, fmt_str: &str) -> String {
    format_number_with_code(num, fmt_str).unwrap_or_else(|_| format_general_number(num))
}

/// Applies the text section (`@`) of a format code to a text value, mirroring `TEXT("abc", fmt)`.
pub fn format_text_value_with_code(text: &str, fmt_str: &str) -> String {
    compiled_format(fmt_str).render_text(text).text
}

/// Formats `num` with a spreadsheet number format code, returning `#VALUE!` for date/time codes
/// applied to serials outside `0..=2958465.99999`.
pub fn format_number_with_code(num: f64, fmt_str: &str) -> Result<String, FormulaError> {
    format_number_styled(num, fmt_str).map(|f| f.text)
}

/// Like [`format_number_with_code`] but also reports the section color and `*x` fill request.
pub fn format_number_styled(num: f64, fmt_str: &str) -> Result<FormattedText, FormulaError> {
    compiled_format(fmt_str).render_number(num)
}

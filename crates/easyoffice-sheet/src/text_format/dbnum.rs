//! `[DBNum1]`..`[DBNum4]` numerals: 1 Hanja, 2 formal Hanja, 3 full-width digits, 4 Hangul.
//! Kinds 1, 2 and 4 spell the integer part with place units (`一千二百三十四`); every kind maps
//! the remaining digits (fraction, kind 3 integer) one to one. Glyph tables are written directly.

/// `DBNum3`: positional full-width digits, no place units.
pub(super) const FULL_WIDTH: u8 = 3;

/// Integer parts longer than this fall back to digit-by-digit glyphs (four-digit groups with a
/// named unit end at `京`).
const MAX_UNIT_DIGITS: usize = 20;
const GROUP_DIGITS: usize = 4;

struct Glyphs {
    digits: [&'static str; 10],
    small_units: [&'static str; GROUP_DIGITS],
    big_units: [&'static str; 5],
}

const HANJA: Glyphs = Glyphs {
    digits: ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"],
    small_units: ["", "十", "百", "千"],
    big_units: ["", "萬", "億", "兆", "京"],
};
const HANJA_FORMAL: Glyphs = Glyphs {
    digits: ["零", "壹", "貳", "參", "四", "伍", "六", "七", "八", "九"],
    small_units: ["", "拾", "百", "阡"],
    big_units: ["", "萬", "億", "兆", "京"],
};
const HANGUL: Glyphs = Glyphs {
    digits: ["영", "일", "이", "삼", "사", "오", "육", "칠", "팔", "구"],
    small_units: ["", "십", "백", "천"],
    big_units: ["", "만", "억", "조", "경"],
};
const FULL_WIDTH_DIGITS: [&str; 10] = ["０", "１", "２", "３", "４", "５", "６", "７", "８", "９"];
const HANJA_POSITIONAL: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];

fn glyphs(kind: u8) -> &'static Glyphs {
    match kind {
        2 => &HANJA_FORMAL,
        4 => &HANGUL,
        _ => &HANJA,
    }
}

fn positional(kind: u8) -> &'static [&'static str; 10] {
    match kind {
        1 => &HANJA_POSITIONAL,
        FULL_WIDTH => &FULL_WIDTH_DIGITS,
        _ => &glyphs(kind).digits,
    }
}

/// Replaces every ASCII digit of `s` with the kind's positional glyph.
pub(super) fn map_digits(s: &str, kind: u8) -> String {
    let table = positional(kind);
    let mut out = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c.to_digit(10).filter(|_| c.is_ascii_digit()) {
            Some(d) => out.push_str(table[d as usize]),
            None => out.push(c),
        }
    }
    out
}

/// Spells an integer (ASCII digits, may be empty) for kinds 1, 2 and 4.
pub(super) fn integer(digits: &str, kind: u8) -> String {
    if digits.len() > MAX_UNIT_DIGITS {
        return map_digits(digits, kind);
    }
    let g = glyphs(kind);
    let bytes = digits.as_bytes();
    let mut out = String::new();
    let mut pending_zero = false;
    for (i, &b) in bytes.iter().enumerate() {
        let pos = bytes.len() - 1 - i;
        let d = (b - b'0') as usize;
        let in_group = pos % GROUP_DIGITS;
        if d == 0 {
            pending_zero = !out.is_empty();
        } else {
            if pending_zero {
                out.push_str(g.digits[0]);
                pending_zero = false;
            }
            out.push_str(g.digits[d]);
            out.push_str(g.small_units[in_group]);
        }
        if in_group == 0 && group_is_nonzero(bytes, pos) {
            out.push_str(g.big_units[pos / GROUP_DIGITS]);
        }
    }
    if out.is_empty() && !digits.is_empty() {
        out.push_str(g.digits[0]);
    }
    out
}

/// Whether the four-digit group whose last digit is at `pos` (from the right) has a nonzero digit.
fn group_is_nonzero(bytes: &[u8], pos: usize) -> bool {
    let end = bytes.len() - pos;
    let start = end.saturating_sub(GROUP_DIGITS);
    bytes[start..end].iter().any(|&b| b != b'0')
}

/// A rendered plain number (`123`, `4.56`, `1E+20`): unit-spelled integer part for kinds 1, 2
/// and 4, digit-by-digit glyphs for the fraction, exponent and kind 3.
pub(super) fn number_text(text: &str, kind: u8) -> String {
    let (int_part, frac_part) = text.split_once('.').unwrap_or((text, ""));
    if kind == FULL_WIDTH || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return map_digits(text, kind);
    }
    let mut out = integer(int_part, kind);
    if !frac_part.is_empty() {
        out.push('.');
        out.push_str(&map_digits(frac_part, kind));
    }
    out
}

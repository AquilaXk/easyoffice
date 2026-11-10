//! Font names of Hangul documents -> fonts that are installed on the machine (PD-101).
//!
//! Page SVGs name the fonts of the document (Hangul word-processor faces such as 함초롬바탕, 바탕,
//! 돋움, 굴림, 궁서 and the licensed headline families). Most of them are not installed on a
//! Windows or macOS machine, so PDF text would fall back to an arbitrary face. Before an SVG is
//! parsed, every `font-family` list that names such a face gets the platform's Korean system fonts
//! appended right after it, ending in the generic family (the font database maps the generic
//! families to Noto Sans KR). The original name stays first, so an installed original still wins.
//!
//! Adapted from the reference app `apps/desktop/src-tauri/src/pdf_font_fallbacks.rs` (MIT, see
//! THIRD_PARTY_NOTICES.md): the reference app rewrites a few restricted families with string comparisons in a
//! loop; here the names live in one sorted static table that is searched with a binary search over
//! ASCII-case-folded bytes, and deduplication uses a 64-bit bitmask (`u64`) over known document and
//! fallback faces while writing directly into the output buffer without intermediate allocations.

use std::borrow::Cow;
use std::cmp::Ordering;

/// Kind of face a name stands for; it picks the replacement chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontClass {
    Serif,
    Sans,
}

use FontClass::{Sans, Serif};

/// Names sorted by their ASCII-lower-cased bytes (a unit test checks the order).
/// Indices `0..35` map to bits `1u64 << index` in the deduplication bitmask.
const FONT_CLASSES: &[(&str, FontClass)] = &[
    ("hci poppy", Sans),
    ("hygothic-extra", Sans),
    ("hygraphic-medium", Sans),
    ("hyheadline m", Sans),
    ("hyheadline medium", Sans),
    ("hymyeongjo-extra", Serif),
    ("hy견고딕", Sans),
    ("hy그래픽", Sans),
    ("hy그래픽m", Sans),
    ("hy신명조", Serif),
    ("hy중고딕", Sans),
    ("hy헤드라인m", Sans),
    ("굴림", Sans),
    ("굴림체", Sans),
    ("궁서", Serif),
    ("궁서체", Serif),
    ("돋움", Sans),
    ("돋움체", Sans),
    ("맑은 고딕", Sans),
    ("명조", Serif),
    ("바탕", Serif),
    ("바탕체", Serif),
    ("새굴림", Sans),
    ("새바탕", Serif),
    ("신명조", Serif),
    ("한양신명조", Serif),
    ("한양중고딕", Sans),
    ("함초롬돋움", Sans),
    ("함초롬바탕", Serif),
    ("휴먼고딕", Sans),
    ("휴먼둥근헤드라인", Sans),
    ("휴먼매직체", Sans),
    ("휴먼명조", Serif),
    ("휴먼옛체", Sans),
    ("휴먼편지체", Sans),
];

// Bit positions in `u64` for fallback families (document faces use `1u64 << index` in `0..35`).
const BIT_MALGUN: u64 = 1u64 << 18; // "맑은 고딕" in FONT_CLASSES[18]
const BIT_BATANG: u64 = 1u64 << 20; // "바탕" in FONT_CLASSES[20]
const BIT_APPLE_GOTHIC: u64 = 1u64 << 35;
const BIT_APPLE_MYEONGJO: u64 = 1u64 << 36;
const BIT_NOTO_SANS: u64 = 1u64 << 37;
const BIT_NOTO_SERIF: u64 = 1u64 << 38;
const BIT_SANS_SERIF: u64 = 1u64 << 39;
const BIT_SERIF: u64 = 1u64 << 40;

const SERIF_BITS_WINDOWS: &[(&str, u64)] = &[
    ("바탕", BIT_BATANG),
    ("Noto Serif KR", BIT_NOTO_SERIF),
    ("AppleMyungjo", BIT_APPLE_MYEONGJO),
    ("맑은 고딕", BIT_MALGUN),
    ("serif", BIT_SERIF),
];
const SERIF_BITS_MACOS: &[(&str, u64)] = &[
    ("AppleMyungjo", BIT_APPLE_MYEONGJO),
    ("바탕", BIT_BATANG),
    ("Noto Serif KR", BIT_NOTO_SERIF),
    ("serif", BIT_SERIF),
];
const SANS_BITS_WINDOWS: &[(&str, u64)] = &[
    ("맑은 고딕", BIT_MALGUN),
    ("Noto Sans KR", BIT_NOTO_SANS),
    ("Apple SD Gothic Neo", BIT_APPLE_GOTHIC),
    ("sans-serif", BIT_SANS_SERIF),
];
const SANS_BITS_MACOS: &[(&str, u64)] = &[
    ("Apple SD Gothic Neo", BIT_APPLE_GOTHIC),
    ("맑은 고딕", BIT_MALGUN),
    ("Noto Sans KR", BIT_NOTO_SANS),
    ("sans-serif", BIT_SANS_SERIF),
];

const ATTRIBUTE: &str = "font-family=\"";
const STACK_FAMILY_SLOTS: usize = 16;

/// Byte-wise comparison with ASCII letters folded to lower case (no allocation).
#[inline]
fn cmp_folded(a: &str, b: &str) -> Ordering {
    let left = a.bytes().map(|byte| byte.to_ascii_lowercase());
    let right = b.bytes().map(|byte| byte.to_ascii_lowercase());
    left.cmp(right)
}

#[inline]
fn trim_quotes_and_spaces(name: &str) -> &str {
    name.trim().trim_matches(|c| c == '\'' || c == '"').trim()
}

/// Looks up `(bitmask, Option<FontClass>)` in a single pass.
#[inline]
fn lookup_family(name: &str) -> (u64, Option<FontClass>) {
    let stripped = trim_quotes_and_spaces(name);
    if stripped.is_empty() {
        return (0, None);
    }
    if let Ok(index) = FONT_CLASSES.binary_search_by(|(key, _)| cmp_folded(key, stripped)) {
        return (1u64 << index, Some(FONT_CLASSES[index].1));
    }
    let bit = match stripped.as_bytes()[0] | 0x20 {
        b'a' if stripped.eq_ignore_ascii_case("Apple SD Gothic Neo") => BIT_APPLE_GOTHIC,
        b'a' if stripped.eq_ignore_ascii_case("AppleMyungjo") => BIT_APPLE_MYEONGJO,
        b'n' if stripped.eq_ignore_ascii_case("Noto Sans KR") => BIT_NOTO_SANS,
        b'n' if stripped.eq_ignore_ascii_case("Noto Serif KR") => BIT_NOTO_SERIF,
        b's' if stripped.eq_ignore_ascii_case("sans-serif") => BIT_SANS_SERIF,
        b's' if stripped.eq_ignore_ascii_case("serif") => BIT_SERIF,
        _ => 0,
    };
    (bit, None)
}

/// Face class of a family name (surrounding quotes and spaces are ignored), if it is one of the
/// document faces in the table.
#[cfg(test)]
pub fn classify(name: &str) -> Option<FontClass> {
    lookup_family(name).1
}

/// Installed faces to try after a face of `class`, best first for the running platform.
#[cfg(test)]
pub fn chain(class: FontClass) -> Vec<&'static str> {
    chain_bits(class).iter().map(|&(name, _)| name).collect()
}

#[inline]
fn chain_bits(class: FontClass) -> &'static [(&'static str, u64)] {
    let macos = cfg!(target_os = "macos");
    match (class, macos) {
        (Serif, false) => SERIF_BITS_WINDOWS,
        (Serif, true) => SERIF_BITS_MACOS,
        (Sans, false) => SANS_BITS_WINDOWS,
        (Sans, true) => SANS_BITS_MACOS,
    }
}

#[inline]
fn push_csv_item(out: &mut String, first: &mut bool, item: &str) {
    if !*first {
        out.push_str(", ");
    }
    *first = false;
    out.push_str(item);
}

/// Appends the expanded `font-family` list for `value` after `prefix` when at least one family in
/// `value` is a document face; returns `false` without touching `output` when nothing changes.
fn expand_into(
    svg: &str,
    copied: usize,
    start: usize,
    value: &str,
    output: &mut Option<String>,
) -> bool {
    // Fast path for single-family attributes (no comma): 1 binary search, 0 intermediate buffers.
    if !value.as_bytes().contains(&b',') {
        let family = value.trim();
        let (bit, Some(class)) = lookup_family(family) else {
            return false;
        };
        let out = output.get_or_insert_with(|| String::with_capacity(svg.len() + 128));
        out.push_str(&svg[copied..start]);
        let mut first = true;
        push_csv_item(out, &mut first, family);
        for &(fallback, fb_bit) in chain_bits(class) {
            if (bit & fb_bit) == 0 {
                push_csv_item(out, &mut first, fallback);
            }
        }
        return true;
    }

    let mut slots: [(&str, u64, Option<FontClass>); STACK_FAMILY_SLOTS] =
        [("", 0, None); STACK_FAMILY_SLOTS];
    let mut len = 0usize;
    let mut input_mask = 0u64;
    let mut has_doc_face = false;

    for raw in value.split(',') {
        let family = raw.trim();
        if family.is_empty() {
            continue;
        }
        if len >= STACK_FAMILY_SLOTS {
            return expand_many_into(svg, copied, start, value, output);
        }
        let (bit, class) = lookup_family(family);
        input_mask |= bit;
        has_doc_face |= class.is_some();
        slots[len] = (family, bit, class);
        len += 1;
    }
    if !has_doc_face {
        return false;
    }

    let out = output.get_or_insert_with(|| String::with_capacity(svg.len() + 128));
    out.push_str(&svg[copied..start]);
    let mut emitted_mask = 0u64;
    let mut first = true;
    for i in 0..len {
        let (family, bit, class) = slots[i];
        if bit != 0 {
            if (emitted_mask & bit) == 0 {
                emitted_mask |= bit;
                push_csv_item(out, &mut first, family);
            }
        } else if !slots[..i]
            .iter()
            .any(|(prev, _, _)| cmp_folded(prev, family) == Ordering::Equal)
        {
            push_csv_item(out, &mut first, family);
        }
        let Some(class) = class else {
            continue;
        };
        for &(fallback, fb_bit) in chain_bits(class) {
            if ((input_mask | emitted_mask) & fb_bit) == 0 {
                emitted_mask |= fb_bit;
                push_csv_item(out, &mut first, fallback);
            }
        }
    }
    true
}

fn expand_many_into(
    svg: &str,
    copied: usize,
    start: usize,
    value: &str,
    output: &mut Option<String>,
) -> bool {
    let entries: Vec<(&str, u64, Option<FontClass>)> = value
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| {
            let (bit, class) = lookup_family(f);
            (f, bit, class)
        })
        .collect();
    if !entries.iter().any(|(_, _, class)| class.is_some()) {
        return false;
    }
    let input_mask = entries.iter().fold(0u64, |acc, &(_, bit, _)| acc | bit);
    let out = output.get_or_insert_with(|| String::with_capacity(svg.len() + 128));
    out.push_str(&svg[copied..start]);
    let mut emitted_mask = 0u64;
    let mut first = true;
    for (i, &(family, bit, class)) in entries.iter().enumerate() {
        if bit != 0 {
            if (emitted_mask & bit) == 0 {
                emitted_mask |= bit;
                push_csv_item(out, &mut first, family);
            }
        } else if !entries[..i]
            .iter()
            .any(|(prev, _, _)| cmp_folded(prev, family) == Ordering::Equal)
        {
            push_csv_item(out, &mut first, family);
        }
        let Some(class) = class else {
            continue;
        };
        for &(fallback, fb_bit) in chain_bits(class) {
            if ((input_mask | emitted_mask) & fb_bit) == 0 {
                emitted_mask |= fb_bit;
                push_csv_item(out, &mut first, fallback);
            }
        }
    }
    true
}

/// Rewrites every `font-family="…"` attribute that names a document face. The result is the input
/// itself when nothing needs a fallback.
pub fn add_font_fallbacks(svg: &str) -> Cow<'_, str> {
    let mut output: Option<String> = None;
    let mut copied = 0usize;
    let mut search = 0usize;
    while let Some(found) = svg[search..].find(ATTRIBUTE) {
        let start = search + found + ATTRIBUTE.len();
        let Some(length) = svg[start..].find('"') else {
            break;
        };
        let end = start + length;
        search = end;
        if expand_into(svg, copied, start, &svg[start..end], &mut output) {
            copied = end;
        }
    }
    match output {
        Some(mut out) => {
            out.push_str(&svg[copied..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(svg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_free_of_duplicates() {
        assert!(
            FONT_CLASSES.len() <= 35,
            "document face indices fit below fallback bits 35..=40"
        );
        for pair in FONT_CLASSES.windows(2) {
            assert_eq!(
                cmp_folded(pair[0].0, pair[1].0),
                Ordering::Less,
                "{} < {}",
                pair[0].0,
                pair[1].0
            );
        }
        for (name, _) in FONT_CLASSES {
            assert_eq!(
                *name,
                name.to_ascii_lowercase(),
                "keys are stored lower-cased"
            );
        }
        assert_eq!(FONT_CLASSES[18].0, "맑은 고딕");
        assert_eq!(FONT_CLASSES[20].0, "바탕");
    }

    #[test]
    fn every_listed_name_is_found_and_unknown_names_are_not() {
        for (name, class) in FONT_CLASSES {
            assert_eq!(classify(name), Some(*class), "{name}");
        }
        assert_eq!(classify("함초롬바탕"), Some(Serif));
        assert_eq!(classify("'HY헤드라인M'"), Some(Sans));
        assert_eq!(
            classify("HyHeadLine M"),
            Some(Sans),
            "ASCII case is ignored"
        );
        assert_eq!(classify("  굴림체 "), Some(Sans));
        assert_eq!(classify("Noto Sans KR"), None);
        assert_eq!(classify("serif"), None);
        assert_eq!(classify(""), None);
    }

    #[test]
    fn documents_without_listed_faces_are_returned_unchanged_without_a_copy() {
        let svg = r#"<text font-family="Noto Sans KR, sans-serif">A</text><text>B</text>"#;
        assert!(matches!(add_font_fallbacks(svg), Cow::Borrowed(_)));
    }

    #[test]
    fn a_serif_face_gets_the_installed_serif_faces_after_it() {
        let svg = r#"<text font-family="휴먼명조">A</text>"#;
        let out = add_font_fallbacks(svg);
        let first = chain(Serif)[0];
        assert!(out.starts_with(r#"<text font-family="휴먼명조, "#), "{out}");
        assert!(out.contains(&format!(", {first}, ")), "{out}");
        assert!(out.ends_with(r#", serif">A</text>"#), "{out}");
    }

    fn first_family_list(svg: &str) -> String {
        let start = svg.find(ATTRIBUTE).unwrap() + ATTRIBUTE.len();
        svg[start..start + svg[start..].find('"').unwrap()].to_string()
    }

    #[test]
    fn existing_fallbacks_are_not_repeated_and_the_order_of_the_list_is_kept() {
        let svg = r#"<text font-family="'바탕체', 바탕, serif">A</text><g font-family="Noto Sans KR, 돋움">B</g>"#;
        let out = add_font_fallbacks(svg);
        let list = first_family_list(&out);
        let names: Vec<&str> = list.split(", ").collect();
        let mut seen = std::collections::HashSet::new();
        assert!(
            names.iter().all(|n| seen.insert(n.to_ascii_lowercase())),
            "no duplicates: {list}"
        );
        assert_eq!(names[0], "'바탕체'", "the original name stays first");
        assert!(
            names.contains(&"바탕") && names.contains(&"serif"),
            "{list}"
        );
        assert!(
            out.contains(r#"font-family="Noto Sans KR, 돋움, "#),
            "{out}"
        );
    }

    #[test]
    fn an_unterminated_attribute_is_left_alone() {
        let svg = r#"<text font-family="돋움>A</text>"#;
        assert_eq!(add_font_fallbacks(svg), svg);
    }

    #[test]
    fn chains_end_with_a_generic_family_and_start_with_an_installed_korean_face() {
        for class in [Serif, Sans] {
            let list = chain(class);
            assert!(["serif", "sans-serif"].contains(list.last().unwrap()));
            assert!(list.len() >= 3);
        }
    }
}

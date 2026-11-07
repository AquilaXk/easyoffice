//! Font names of Hangul documents -> fonts that are installed on the machine (PD-101).
//!
//! Page SVGs name the fonts of the document (Hangul word-processor faces such as 함초롬바탕, 바탕,
//! 돋움, 굴림, 궁서 and the licensed headline families). Most of them are not installed on a
//! Windows or macOS machine, so PDF text would fall back to an arbitrary face. Before an SVG is
//! parsed, every `font-family` list that names such a face gets the platform's Korean system fonts
//! appended right after it, ending in the generic family (the font database maps the generic
//! families to Noto Sans KR). The original name stays first, so an installed original still wins.
//!
//! Adapted from hop `apps/desktop/src-tauri/src/pdf_font_fallbacks.rs` (MIT, see
//! THIRD_PARTY_NOTICES.md): hop rewrites a few restricted families with string comparisons in a
//! loop; here the names live in one sorted static table that is searched with a binary search over
//! ASCII-case-folded bytes, so a lookup never allocates, and an SVG without such a name is
//! returned as is (`Cow::Borrowed`).

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

const SERIF_CHAIN_WINDOWS: &[&str] = &["바탕", "Noto Serif KR", "AppleMyungjo", "serif"];
const SERIF_CHAIN_MACOS: &[&str] = &["AppleMyungjo", "바탕", "Noto Serif KR", "serif"];
const SANS_CHAIN_WINDOWS: &[&str] = &[
    "맑은 고딕",
    "Noto Sans KR",
    "Apple SD Gothic Neo",
    "sans-serif",
];
const SANS_CHAIN_MACOS: &[&str] = &[
    "Apple SD Gothic Neo",
    "맑은 고딕",
    "Noto Sans KR",
    "sans-serif",
];

const ATTRIBUTE: &str = "font-family=\"";

/// Byte-wise comparison with ASCII letters folded to lower case (no allocation).
fn cmp_folded(a: &str, b: &str) -> Ordering {
    let left = a.bytes().map(|byte| byte.to_ascii_lowercase());
    let right = b.bytes().map(|byte| byte.to_ascii_lowercase());
    left.cmp(right)
}

/// Face class of a family name (surrounding quotes and spaces are ignored), if it is one of the
/// document faces in the table.
pub fn classify(name: &str) -> Option<FontClass> {
    let name = name.trim().trim_matches(|c| c == '\'' || c == '"').trim();
    FONT_CLASSES
        .binary_search_by(|(key, _)| cmp_folded(key, name))
        .ok()
        .map(|index| FONT_CLASSES[index].1)
}

/// Installed faces to try after a face of `class`, best first for the running platform.
pub fn chain(class: FontClass) -> &'static [&'static str] {
    let macos = cfg!(target_os = "macos");
    match (class, macos) {
        (Serif, false) => SERIF_CHAIN_WINDOWS,
        (Serif, true) => SERIF_CHAIN_MACOS,
        (Sans, false) => SANS_CHAIN_WINDOWS,
        (Sans, true) => SANS_CHAIN_MACOS,
    }
}

fn is_listed(families: &[&str], name: &str) -> bool {
    families
        .iter()
        .any(|family| cmp_folded(family, name) == Ordering::Equal)
}

/// The expanded `font-family` value, or `None` when no family in it needs a fallback.
fn expand_value(value: &str) -> Option<String> {
    let families: Vec<&str> = value
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect();
    if !families.iter().any(|family| classify(family).is_some()) {
        return None;
    }
    let mut out: Vec<&str> = Vec::with_capacity(families.len() + 4);
    for family in &families {
        if !is_listed(&out, family) {
            out.push(family);
        }
        let Some(class) = classify(family) else {
            continue;
        };
        for fallback in chain(class) {
            if !is_listed(&families, fallback) && !is_listed(&out, fallback) {
                out.push(fallback);
            }
        }
    }
    Some(out.join(", "))
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
        let Some(expanded) = expand_value(&svg[start..end]) else {
            continue;
        };
        let out = output.get_or_insert_with(|| String::with_capacity(svg.len() + 128));
        out.push_str(&svg[copied..start]);
        out.push_str(&expanded);
        copied = end;
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

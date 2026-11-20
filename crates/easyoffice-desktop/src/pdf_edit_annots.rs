//! Free text, ink and stamp annotations of the sources in the result (PD-266).
//!
//! The annotations that `pdf_edit_preserve.rs` writes again field by field (links, text markup,
//! notes) are the ones whose look the shell can draw itself. These three are drawn by their own
//! appearance streams, and they hold more than a short list of fields can: so the dictionary is
//! carried over as it is, key by key, with three changes. The rectangle and the points of the
//! strokes and of a callout line move by the origin of the page's crop box, which is where the
//! new page starts (as for every annotation of the page); the entries that point back at the page
//! or at other annotations (`/P`, `/Parent`, `/Popup`, `/IRT`) or run something (`/A`, `/AA`) are
//! left out; everything else, the appearance streams and the layer an annotation belongs to
//! (`/OC`) included, is copied by reference with the page's own objects (`pdf_edit_carry.rs`).
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-write` copies a
//! dictionary only as part of a page and its writer is private, so it cannot move a rectangle;
//! (2) the JavaScript PDF viewers read these annotations for display and write them with their own
//! appearance generators, which the shell does not need because the original streams are kept;
//! (3) the key table below follows ISO 32000-1 §12.5.2 (annotation dictionaries), §12.5.6.6 (free
//! text), §12.5.6.13 (ink) and §12.5.6.12 (rubber stamp); nothing was copied.
//!
//! Every other kind of annotation that is not written elsewhere is counted and reported, never
//! dropped silently (`lost_sentence`).

use crate::pdf_edit_budget::MAX_PAGE_ANNOTS;
use crate::pdf_edit_preserve::{read_shifted_rect, too_many_page_annots};
use crate::pdf_edit_value::{Budget, Value};
use hayro_syntax::object::{Array, Dict, Name};
use hayro_syntax::page::Page;
use std::collections::BTreeMap;

/// The kinds carried over as dictionaries.
const CARRIED: [&str; 3] = ["FreeText", "Ink", "Stamp"];
/// The kinds that are written elsewhere (`pdf_edit_preserve.rs`, `pdf_edit_forms.rs`), and the
/// pop-up, whose text is the `/Contents` of the annotation it belongs to.
const WRITTEN_ELSEWHERE: [&str; 8] = [
    "Link",
    "Highlight",
    "Underline",
    "StrikeOut",
    "Squiggly",
    "Text",
    "Widget",
    "Popup",
];
/// The keys of a carried annotation. Everything not listed is left out, whatever it is.
const KEYS: [&[u8]; 33] = [
    b"Type",
    b"Subtype",
    b"Rect",
    b"Contents",
    b"NM",
    b"M",
    b"F",
    b"AP",
    b"AS",
    b"Border",
    b"BS",
    b"BE",
    b"C",
    b"CA",
    b"ca",
    b"T",
    b"Subj",
    b"CreationDate",
    b"RC",
    b"DS",
    b"DA",
    b"Q",
    b"IT",
    b"RD",
    b"IC",
    b"LE",
    b"CL",
    b"InkList",
    b"Name",
    b"Intent",
    b"Lang",
    b"OC",
    b"Rotate",
];
/// Longest annotation subtype name quoted in a warning (characters); the name comes from the file.
const MAX_SUBTYPE_CHARS: usize = 24;
/// Most subtype names listed in one warning.
const MAX_LISTED_SUBTYPES: usize = 6;

/// How many annotations of each kind the result does not keep.
pub type Lost = BTreeMap<String, usize>;

/// The carried annotations of one page, moved into the page space of the result, and the count
/// of those that could not be (a rectangle that is not four numbers, strokes that are not points).
pub fn scan_page(
    page: &Page<'_>,
    budget: &mut Budget,
    lost: &mut Lost,
) -> Result<Vec<Value>, String> {
    let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") else {
        return Ok(Vec::new());
    };
    let crop = page.intersected_crop_box();
    #[allow(clippy::cast_possible_truncation)]
    let (dx, dy) = (-(crop.x0 as f32), -(crop.y0 as f32));
    let mut carried = Vec::new();
    for (idx, annot) in annots.iter::<Dict<'_>>().enumerate() {
        if idx >= MAX_PAGE_ANNOTS {
            return Err(too_many_page_annots());
        }
        let Some(subtype) = annot.get::<Name<'_>>(b"Subtype") else {
            continue;
        };
        let kind = subtype.as_str();
        if CARRIED.contains(&kind) {
            match carry_annot(&annot, kind, (dx, dy), budget)? {
                Some(value) => carried.push(value),
                None => *lost.entry(kind.to_string()).or_default() += 1,
            }
        } else if !WRITTEN_ELSEWHERE.contains(&kind) {
            *lost.entry(printable(kind)).or_default() += 1;
        }
    }
    Ok(carried)
}

/// The annotation with only the listed keys, its rectangle and points moved by (`dx`, `dy`); `None`
/// when what has to move cannot be read as numbers.
fn carry_annot(
    annot: &Dict<'_>,
    kind: &str,
    (dx, dy): (f32, f32),
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    let mut value = Value::read_dict(annot, |key| KEYS.contains(&key), budget)
        .map_err(|refused| refused.sentence("주석"))?;
    let Some(rect) = read_shifted_rect(annot, dx, dy) else {
        return Ok(None);
    };
    value.set(b"Rect", Value::Array(rect.map(Value::Real).to_vec()));
    if let Some(strokes) = value.get_mut(b"InkList") {
        if !shift_points(strokes, dx, dy) {
            return Ok(None);
        }
    } else if kind == "Ink" {
        return Ok(None);
    }
    if let Some(line) = value.get_mut(b"CL") {
        if !shift_points(line, dx, dy) {
            value.remove(b"CL");
        }
    }
    Ok(Some(value))
}

/// Moves the points in `value` (an array of numbers `x y x y ...`, or arrays of them) by
/// (`dx`, `dy`). `false` when it is not made of such pairs; `value` is then left half moved and
/// must be dropped.
fn shift_points(value: &mut Value, dx: f32, dy: f32) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
    if items.iter().all(|item| item.as_f32().is_some()) {
        if items.len() % 2 != 0 {
            return false;
        }
        for (n, item) in items.iter_mut().enumerate() {
            let at = item.as_f32().unwrap_or_default();
            let moved = at + if n % 2 == 0 { dx } else { dy };
            if !moved.is_finite() {
                return false;
            }
            *item = Value::Real(moved);
        }
        return true;
    }
    items.iter_mut().all(|item| shift_points(item, dx, dy))
}

/// A name from the file as plain text for a sentence: letters and digits only, cut short.
fn printable(name: &str) -> String {
    let text: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_SUBTYPE_CHARS)
        .collect();
    if text.is_empty() {
        "?".to_string()
    } else {
        text
    }
}

/// The sentence for the annotations that were not kept.
pub fn lost_sentence(lost: &Lost) -> String {
    let total: usize = lost.values().sum();
    let mut kinds: Vec<(&String, &usize)> = lost.iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let listed: Vec<String> = kinds
        .iter()
        .take(MAX_LISTED_SUBTYPES)
        .map(|(name, count)| format!("{name} {count}"))
        .collect();
    let more = kinds.len().saturating_sub(MAX_LISTED_SUBTYPES);
    let tail = if more > 0 {
        format!(" 외 {more}종")
    } else {
        String::new()
    };
    format!(
        "지원하지 않는 주석 {total}개는 저장하지 못했습니다 ({}{tail}).",
        listed.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentence_for_lost_annotations_lists_the_most_common_kinds() {
        let lost: Lost = [("Square", 2), ("Line", 5), ("Caret", 1)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(
            lost_sentence(&lost),
            "지원하지 않는 주석 8개는 저장하지 못했습니다 (Line 5, Square 2, Caret 1)."
        );
        let many: Lost = (0..9).map(|n| (format!("K{n}"), 1)).collect();
        assert!(lost_sentence(&many).ends_with("K5 1 외 3종)."));
        assert_eq!(printable("A/B<script>\u{0}"), "ABscript");
        assert_eq!(printable("/"), "?");
    }

    #[test]
    fn points_move_in_pairs_and_anything_else_is_refused() {
        let numbers = |list: &[f32]| Value::Array(list.iter().map(|&n| Value::Real(n)).collect());
        let mut strokes = Value::Array(vec![numbers(&[1.0, 2.0, 3.0, 4.0]), numbers(&[5.0, 6.0])]);
        assert!(shift_points(&mut strokes, -1.0, -2.0));
        assert_eq!(
            strokes,
            Value::Array(vec![numbers(&[0.0, 0.0, 2.0, 2.0]), numbers(&[4.0, 4.0])])
        );
        assert!(
            !shift_points(&mut numbers(&[1.0, 2.0, 3.0]), 0.0, 0.0),
            "odd"
        );
        assert!(!shift_points(&mut Value::Int(1), 0.0, 0.0), "not an array");
        assert!(
            !shift_points(&mut numbers(&[f32::MAX, 1.0]), f32::MAX, 0.0),
            "overflow"
        );
        let mut mixed = Value::Array(vec![Value::Int(1), Value::Name(b"x".to_vec())]);
        assert!(!shift_points(&mut mixed, 0.0, 0.0));
    }
}

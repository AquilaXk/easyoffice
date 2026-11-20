// 참조 구현: qpdf (https://github.com/qpdf/qpdf, 커밋 4eba9589). (1) `libqpdf/QPDFMatrix.cc`의 생성자(6-24줄),
// `concat`(63-78줄), `translate`(86-90줄), `transform`(111-116줄), `transformRectangle`(118-132줄)을
// 옮겼다. (2) `libqpdf/QPDFAcroFormDocumentHelper.cc` `AcroForm::transformAnnotations`의 흐름: 사전이
// 아닌 주석은 건너뛰고(1095줄) 복사한 주석의 `/Rect`를 변환 행렬로 바꾼다(1164-1165줄). Apache-2.0,
// Copyright (c) 2005-2021 Jay Berkenbilt, 2022-2026 Jay Berkenbilt and Manfred Holger. 바꾼 점: 변환은
// 크롭 상자 원점으로 옮기는 평행 이동 하나뿐이다. 원본이 외관 스트림의 `/Matrix`에도 같은 행렬을
// 곱하는 부분(1146-1160줄)은 옮기지 않았다. 외관 스트림은 쪽 복사기가 복사한 객체라 사전을 바꿀 수
// 없고, 평행 이동은 `/Rect`만 옮겨도 `/BBox`가 사각형에 맞춰지는 규칙(ISO 32000-1 §12.5.5)에서 같은
// 결과다. 원본은 `/InkList`·`/CL`을 바꾸지 않지만 여기서는 같은 행렬로 옮긴다. 출처는
// THIRD_PARTY_NOTICES.md에 있다.
//
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
use crate::pdf_edit_preserve::too_many_page_annots;
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

/// A 2x3 matrix `[a b c d e f]` as the format writes it (`QPDFMatrix`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    /// The identity.
    fn new() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `concat`: this matrix followed by `other`.
    fn concat(&mut self, other: Self) {
        let ap = self.a * other.a + self.c * other.b;
        let bp = self.b * other.a + self.d * other.b;
        let cp = self.a * other.c + self.c * other.d;
        let dp = self.b * other.c + self.d * other.d;
        let ep = self.a * other.e + self.c * other.f + self.e;
        let fp = self.b * other.e + self.d * other.f + self.f;
        *self = Self {
            a: ap,
            b: bp,
            c: cp,
            d: dp,
            e: ep,
            f: fp,
        };
    }

    fn translate(&mut self, tx: f64, ty: f64) {
        self.concat(Self {
            e: tx,
            f: ty,
            ..Self::new()
        });
    }

    fn transform(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// `transformRectangle`: the smallest rectangle that holds the four transformed corners.
    fn transform_rectangle(&self, [llx, lly, urx, ury]: [f64; 4]) -> [f64; 4] {
        let corners = [
            self.transform(llx, lly),
            self.transform(llx, ury),
            self.transform(urx, lly),
            self.transform(urx, ury),
        ];
        let fold = |pick: fn(&(f64, f64)) -> f64, join: fn(f64, f64) -> f64, start: f64| {
            corners.iter().map(pick).fold(start, join)
        };
        [
            fold(|c| c.0, f64::min, f64::INFINITY),
            fold(|c| c.1, f64::min, f64::INFINITY),
            fold(|c| c.0, f64::max, f64::NEG_INFINITY),
            fold(|c| c.1, f64::max, f64::NEG_INFINITY),
        ]
    }
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
    let mut cm = Matrix::new();
    cm.translate(f64::from(dx), f64::from(dy));
    let Some(rect) = value.get(b"Rect").and_then(rectangle) else {
        return Ok(None);
    };
    let moved = cm.transform_rectangle(rect);
    if !moved.iter().all(|n| n.is_finite()) {
        return Ok(None);
    }
    #[allow(clippy::cast_possible_truncation)]
    value.set(
        b"Rect",
        Value::Array(moved.iter().map(|&n| Value::Real(n as f32)).collect()),
    );
    if let Some(strokes) = value.get_mut(b"InkList") {
        if !transform_points(strokes, &cm) {
            return Ok(None);
        }
    } else if kind == "Ink" {
        return Ok(None);
    }
    if let Some(line) = value.get_mut(b"CL") {
        if !transform_points(line, &cm) {
            value.remove(b"CL");
        }
    }
    Ok(Some(value))
}

/// The four numbers of a rectangle value.
fn rectangle(value: &Value) -> Option<[f64; 4]> {
    let [a, b, c, d] = value.as_array()? else {
        return None;
    };
    Some([a, b, c, d].map(|n| n.as_f32().map_or(f64::NAN, f64::from)))
        .filter(|rect| rect.iter().all(|n| n.is_finite()))
}

/// Moves the points in `value` (an array of numbers `x y x y ...`, or arrays of them) with `cm`.
/// `false` when it is not made of such pairs; `value` is then left half moved and must be dropped.
fn transform_points(value: &mut Value, cm: &Matrix) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
    if items.iter().all(|item| item.as_f32().is_some()) {
        if items.len() % 2 != 0 {
            return false;
        }
        for pair in items.chunks_mut(2) {
            let [x, y] = pair else { return false };
            let (px, py) = cm.transform(
                f64::from(x.as_f32().unwrap_or_default()),
                f64::from(y.as_f32().unwrap_or_default()),
            );
            #[allow(clippy::cast_possible_truncation)]
            let (px, py) = (px as f32, py as f32);
            if !px.is_finite() || !py.is_finite() {
                return false;
            }
            *x = Value::Real(px);
            *y = Value::Real(py);
        }
        return true;
    }
    items.iter_mut().all(|item| transform_points(item, cm))
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
        let mut cm = Matrix::new();
        cm.translate(-1.0, -2.0);
        let numbers = |list: &[f32]| Value::Array(list.iter().map(|&n| Value::Real(n)).collect());
        let mut strokes = Value::Array(vec![numbers(&[1.0, 2.0, 3.0, 4.0]), numbers(&[5.0, 6.0])]);
        assert!(transform_points(&mut strokes, &cm));
        assert_eq!(
            strokes,
            Value::Array(vec![numbers(&[0.0, 0.0, 2.0, 2.0]), numbers(&[4.0, 4.0])])
        );
        assert!(
            !transform_points(&mut numbers(&[1.0, 2.0, 3.0]), &cm),
            "odd"
        );
        assert!(!transform_points(&mut Value::Int(1), &cm), "not an array");
        let mut far = Matrix::new();
        far.translate(f64::from(f32::MAX), 0.0);
        assert!(
            !transform_points(&mut numbers(&[f32::MAX, 1.0]), &far),
            "overflow"
        );
        let mut mixed = Value::Array(vec![Value::Int(1), Value::Name(b"x".to_vec())]);
        assert!(!transform_points(&mut mixed, &cm));
    }

    #[test]
    fn a_matrix_moves_a_rectangle_and_keeps_it_upright() {
        let mut cm = Matrix::new();
        cm.translate(-20.0, -30.0);
        assert_eq!(
            cm.transform_rectangle([40.0, 200.0, 140.0, 260.0]),
            [20.0, 170.0, 120.0, 230.0]
        );
        // A turn by 90 degrees (a = 0, b = 1, c = -1, d = 0) maps the corners to another rectangle.
        let mut turn = Matrix::new();
        turn.concat(Matrix {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        });
        assert_eq!(
            turn.transform_rectangle([0.0, 0.0, 10.0, 20.0]),
            [-20.0, 0.0, 0.0, 10.0]
        );
        // Corners given the wrong way round give the same rectangle.
        assert_eq!(
            cm.transform_rectangle([140.0, 260.0, 40.0, 200.0]),
            [20.0, 170.0, 120.0, 230.0]
        );
    }
}

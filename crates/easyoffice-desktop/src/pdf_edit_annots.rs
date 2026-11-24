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
//! Carried-over page annotations in the result (PD-266, PD-337).
//!
//! The annotations that `pdf_edit_preserve.rs` writes again field by field (links, text markup,
//! notes) are the ones whose look the shell can draw itself. The eight annotation kinds carried
//! here (`FreeText`, `Ink`, `Stamp`, `Square`, `Circle`, `Line`, `Polygon`, `FileAttachment`) carry
//! geometry, styles, optional appearance streams, optional content layer memberships, and (for
//! `FileAttachment`) embedded file specifications. Each dictionary is copied by allowlisted keys,
//! with `/Rect` and kind-specific coordinates (`/InkList`, `/CL`, `/L`, `/Vertices`, `/Path`)
//! translated by the page crop-box origin; entries that point back at the page or other
//! annotations (`/P`, `/Parent`, `/Popup`, `/IRT`) or run actions (`/A`, `/AA`) are left out.
//! Non-`/AP`/`/OC`/`/FS` keys safely inline non-cyclic non-structural indirect values; `/AP` and
//! `/OC` only retain validated appearance streams and layer dictionaries (`pdf_edit_annots_safe.rs`);
//! `FileAttachment` requires an embedded `/EF` stream whose stream dictionary passes safety and
//! budget checks (external path/URL file specifications are dropped and reported in `lost`).
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-write` copies a
//! dictionary only as part of a page and its writer is private, so it cannot move coordinates or
//! sanitize `/FS`; (2) the JavaScript PDF viewers read these annotations for display only;
//! (3) the key tables below follow ISO 32000-1 §12.5.2 (annotation dictionaries), §12.5.6.6 (free
//! text), §12.5.6.7 (line), §12.5.6.8 (square and circle), §12.5.6.9 (polygon), §12.5.6.12 (rubber
//! stamp), §12.5.6.13 (ink) and §12.5.6.15 (file attachment); nothing was copied.
//!
//! Every other kind of annotation that is not written elsewhere is counted and reported, never
//! dropped silently (`lost_sentence`).

use crate::pdf_edit_annots_safe::{
    is_structural_dict, sanitize_ap, sanitize_filespec, sanitize_oc,
};
use crate::pdf_edit_budget::{
    MAX_ATTACHMENTS, MAX_PAGE_ANNOTS, MAX_VALUE_INLINED_BYTES, MAX_VALUE_NODES,
};
use crate::pdf_edit_preserve::too_many_page_annots;
use crate::pdf_edit_value::{spend_bytes, spend_node, Budget, Value};
use hayro_syntax::object::{Array, Dict, Name};
use hayro_syntax::page::Page;
use hayro_syntax::Pdf as ReadPdf;
use std::collections::BTreeMap;

/// Longest annotation subtype name quoted in a warning (characters); the name comes from the file.
const MAX_SUBTYPE_CHARS: usize = 24;
/// Most subtype names listed in one warning.
const MAX_LISTED_SUBTYPES: usize = 6;

/// How many annotations of each kind the result does not keep.
pub type Lost = BTreeMap<String, usize>;

fn is_carried_kind(kind: &str) -> bool {
    matches!(
        kind,
        "FreeText" | "Ink" | "Stamp" | "Square" | "Circle" | "Line" | "Polygon" | "FileAttachment"
    )
}

fn is_written_elsewhere(kind: &str) -> bool {
    matches!(
        kind,
        "Link" | "Highlight" | "Underline" | "StrikeOut" | "Squiggly" | "Text" | "Widget" | "Popup"
    )
}

fn is_allowed_key(kind: &str, key: &[u8]) -> bool {
    if matches!(
        key,
        b"Type"
            | b"Subtype"
            | b"Rect"
            | b"Contents"
            | b"NM"
            | b"M"
            | b"F"
            | b"AP"
            | b"AS"
            | b"Border"
            | b"BS"
            | b"BE"
            | b"C"
            | b"CA"
            | b"ca"
            | b"T"
            | b"Subj"
            | b"CreationDate"
            | b"RC"
            | b"IT"
            | b"RD"
            | b"IC"
            | b"LE"
            | b"Name"
            | b"Intent"
            | b"Lang"
            | b"OC"
            | b"Rotate"
    ) {
        return true;
    }
    match kind {
        "FreeText" | "Ink" | "Stamp" => {
            matches!(key, b"DS" | b"DA" | b"Q" | b"CL" | b"InkList")
        }
        "Line" => matches!(key, b"L" | b"LL" | b"LLE" | b"LLO" | b"Cap" | b"CP" | b"CO"),
        "Polygon" => matches!(key, b"Vertices" | b"Path"),
        _ => false,
    }
}

/// The carried annotations of one page, moved into the page space of the result, and the count
/// of those that could not be (a rectangle that is not four numbers, strokes/lines/vertices that
/// are not points, or an unsafe/external file attachment).
pub fn scan_page(
    pdf: &ReadPdf,
    page: &Page<'_>,
    budget: &mut Budget,
    file_attachments: &mut usize,
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
        if is_carried_kind(kind) {
            match carry_annot(pdf, &annot, kind, (dx, dy), budget)? {
                Some(value) => {
                    if kind == "FileAttachment" {
                        *file_attachments += 1;
                        if *file_attachments > MAX_ATTACHMENTS {
                            return Err(format!(
                                "첨부 파일 주석 수가 한도({MAX_ATTACHMENTS})를 초과합니다."
                            ));
                        }
                    }
                    carried.push(value);
                }
                None => *lost.entry(kind.to_string()).or_default() += 1,
            }
        } else if !is_written_elsewhere(kind) {
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

/// Reads only allowlisted keys from `annot` into a `Value::Dict` using a shared per-annotation
/// `own` node budget and build-wide `budget`. Only validated `/AP` streams, validated `/OC`
/// layer references, and sanitized `/FS` embedded-file references retain indirect `ObjRef`s; all
/// other keys are safely inlined without cycles or structural dictionaries.
fn read_annot_entries(
    pdf: &ReadPdf,
    annot: &Dict<'_>,
    kind: &str,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
) -> Result<Value, String> {
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    let allow_dict = |dict: &Dict<'_>| !is_structural_dict(dict);
    let mut entries = Vec::new();
    for (key, raw) in annot.entries() {
        let kb = key.as_ref();
        if kb == b"FS" || !is_allowed_key(kind, kb) {
            continue;
        }
        spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
        spend_bytes(kb.len(), own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
        let val = match kb {
            b"AP" => sanitize_ap(pdf, &raw, own, own_bytes, budget)?,
            b"OC" => sanitize_oc(pdf, &raw, own, own_bytes, budget)?,
            _ => {
                let inlined =
                    Value::read_inlined(&raw, 2, own, own_bytes, budget, pdf, &allow_dict)
                        .map_err(|refused| refused.sentence("주석"))?;
                (inlined != Value::Null).then_some(inlined)
            }
        };
        if let Some(val) = val {
            entries.push((kb.to_vec(), val));
        }
    }
    Ok(Value::Dict(entries))
}

/// The annotation with only the listed keys, its rectangle and points moved by (`dx`, `dy`); `None`
/// when what has to move cannot be read as numbers or an attachment specification is unsafe.
fn carry_annot(
    pdf: &ReadPdf,
    annot: &Dict<'_>,
    kind: &str,
    (dx, dy): (f32, f32),
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    let mut own = MAX_VALUE_NODES;
    let mut own_bytes = MAX_VALUE_INLINED_BYTES;
    let mut value = read_annot_entries(pdf, annot, kind, &mut own, &mut own_bytes, budget)?;
    let mut cm = Matrix::new();
    cm.translate(f64::from(dx), f64::from(dy));
    let Some(rect) = value.get(b"Rect").and_then(rectangle) else {
        return Ok(None);
    };
    let moved = cm.transform_rectangle(rect);
    #[allow(clippy::cast_possible_truncation)]
    let moved_f32 = moved.map(|n| n as f32);
    if !moved_f32.iter().all(|n| n.is_finite()) {
        return Ok(None);
    }
    value.set(
        b"Rect",
        Value::Array(moved_f32.into_iter().map(Value::Real).collect()),
    );
    match kind {
        "Ink" => {
            let Some(strokes) = value.get_mut(b"InkList") else {
                return Ok(None);
            };
            if !transform_points(strokes, &cm) {
                return Ok(None);
            }
        }
        "FreeText" => {
            if let Some(line) = value.get_mut(b"CL") {
                if !transform_points(line, &cm) {
                    value.remove(b"CL");
                }
            }
        }
        "Line" => {
            let Some(line) = value.get_mut(b"L") else {
                return Ok(None);
            };
            if !transform_flat_points(line, &cm, Some(4)) {
                return Ok(None);
            }
        }
        "Polygon" => {
            let mut has_geometry = false;
            if let Some(vertices) = value.get_mut(b"Vertices") {
                if !transform_flat_points(vertices, &cm, None) {
                    return Ok(None);
                }
                has_geometry = true;
            }
            if let Some(path) = value.get_mut(b"Path") {
                if !transform_path(path, &cm) {
                    return Ok(None);
                }
                has_geometry = true;
            }
            if !has_geometry {
                return Ok(None);
            }
        }
        "FileAttachment" => {
            let Some(fs) = sanitize_filespec(pdf, annot, &mut own, &mut own_bytes, budget)? else {
                return Ok(None);
            };
            value.set(b"FS", fs);
        }
        _ => {}
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

fn transform_point_pairs(items: &mut [Value], cm: &Matrix) -> bool {
    if !items.len().is_multiple_of(2) || !items.iter().all(|item| item.as_f32().is_some()) {
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
    true
}

fn transform_flat_points(value: &mut Value, cm: &Matrix, exact_len: Option<usize>) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
    if items.is_empty() {
        return false;
    }
    if let Some(expected) = exact_len {
        if items.len() != expected {
            return false;
        }
    }
    transform_point_pairs(items, cm)
}

fn transform_path(value: &mut Value, cm: &Matrix) -> bool {
    let Value::Array(subpaths) = value else {
        return false;
    };
    !subpaths.is_empty()
        && subpaths
            .iter_mut()
            .all(|sub| transform_path_nested(sub, cm))
}

fn transform_path_nested(value: &mut Value, cm: &Matrix) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
    if items.is_empty() {
        return false;
    }
    if items.iter().all(|item| item.as_f32().is_some()) {
        return transform_point_pairs(items, cm);
    }
    items.iter_mut().all(|item| transform_path_nested(item, cm))
}

/// Moves the points in `value` (an array of numbers `x y x y ...`, or arrays of them) with `cm`.
/// `false` when it is not made of such pairs; `value` is then left half moved and must be dropped.
fn transform_points(value: &mut Value, cm: &Matrix) -> bool {
    let Value::Array(items) = value else {
        return false;
    };
    if items.iter().all(|item| item.as_f32().is_some()) {
        return transform_point_pairs(items, cm);
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

    #[test]
    fn carried_subtypes_keys_and_flat_geometry_rules_are_enforced() {
        for kind in [
            "FreeText",
            "Ink",
            "Stamp",
            "Square",
            "Circle",
            "Line",
            "Polygon",
            "FileAttachment",
        ] {
            assert!(is_carried_kind(kind), "{kind}");
            assert!(!is_allowed_key(kind, b"P"));
            assert!(!is_allowed_key(kind, b"Parent"));
            assert!(!is_allowed_key(kind, b"Popup"));
            assert!(!is_allowed_key(kind, b"IRT"));
            assert!(!is_allowed_key(kind, b"A"));
            assert!(!is_allowed_key(kind, b"AA"));
            assert!(!is_allowed_key(kind, b"StructParent"));
        }
        assert!(!is_carried_kind("Caret"));
        assert!(!is_carried_kind("PolyLine"));
        assert!(is_allowed_key("Line", b"L"));
        assert!(is_allowed_key("Line", b"LL"));
        assert!(is_allowed_key("Line", b"Cap"));
        assert!(!is_allowed_key("Square", b"L"));
        assert!(is_allowed_key("Polygon", b"Vertices"));
        assert!(is_allowed_key("Polygon", b"Path"));
        assert!(!is_allowed_key("Circle", b"Vertices"));
        assert!(!is_allowed_key("FileAttachment", b"FS"));

        let mut cm = Matrix::new();
        cm.translate(-20.0, -30.0);
        let nums = |list: &[f32]| Value::Array(list.iter().map(|&n| Value::Real(n)).collect());
        let mut line = nums(&[40.0, 100.0, 140.0, 180.0]);
        assert!(transform_flat_points(&mut line, &cm, Some(4)));
        assert_eq!(line, nums(&[20.0, 70.0, 120.0, 150.0]));
        assert!(!transform_flat_points(&mut nums(&[1.0, 2.0]), &cm, Some(4)));
        assert!(!transform_flat_points(&mut nums(&[]), &cm, None));
        assert!(!transform_flat_points(
            &mut nums(&[1.0, 2.0, 3.0]),
            &cm,
            None
        ));

        let mut path = Value::Array(vec![nums(&[40.0, 100.0, 140.0, 100.0, 90.0, 200.0])]);
        assert!(transform_path(&mut path, &cm));
        assert_eq!(
            path,
            Value::Array(vec![nums(&[20.0, 70.0, 120.0, 70.0, 70.0, 170.0])])
        );
        assert!(!transform_path(&mut Value::Array(Vec::new()), &cm));
        assert!(!transform_path(&mut nums(&[40.0, 100.0]), &cm));
        assert!(!transform_path(
            &mut Value::Array(vec![Value::Array(Vec::new())]),
            &cm
        ));
        let mut nested = Value::Array(vec![Value::Array(vec![nums(&[40.0, 100.0])])]);
        assert!(transform_path(&mut nested, &cm));
        assert_eq!(
            nested,
            Value::Array(vec![Value::Array(vec![nums(&[20.0, 70.0])])])
        );
    }
}

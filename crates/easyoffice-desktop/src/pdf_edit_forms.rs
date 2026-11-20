//! Interactive `/AcroForm` preservation and `applicant` / `department` field updates in the PDF
//! studio (PD-245).
//!
//! Existing `/Subtype /Widget` fields (`/Tx`, `/Btn`, `/Ch`) on kept pages are carried over into
//! the output page's `/Annots` and `/Catalog /AcroForm /Fields`. When `WireForm` sets `applicant`
//! or `department`, matching `/Tx` fields have their `/V` value and `/AP /N` appearance XObject
//! updated (drawn by the shell with bundled Noto Sans KR so Korean glyphs render in every viewer);
//! if no field matches a non-empty value, an interactive `/Tx` widget is added on the first page.

use crate::pdf_edit_budget::{Deadline, MAX_FORM_FIELDS, MAX_TOTAL_ANNOTS, MAX_WIDGET_OVERLAYS};
use crate::pdf_edit_inspect::open_pdf;
use crate::pdf_edit_overlay::upright_matrix;
use crate::pdf_edit_preserve::{decode_pdf_text, read_shifted_rect};
use crate::pdf_edit_wire::{OverlayKind, OverlayMaker, PagePlan, WireForm};
use hayro_syntax::object::{Array, Dict, Name, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::types::{AnnotationFlags, AnnotationType, FieldFlags, FieldType};
use hayro_write::pdf_writer::{Content, Finish, Name as WName, Pdf, Rect, Ref, Str, TextStr};
use hayro_write::{extract, ChunkSettings, ExtractionQuery};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

const MAX_FIELD_DEPTH: usize = 16;
const MAX_FIELD_VALUE_CHARS: usize = 2_048;
const MAX_NAME_CHARS: usize = 256;
const DEFAULT_DA: &[u8] = b"/Helv 10 Tf 0 g";
const EO_LABELED_TAG: &str = "eo_labeled";

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormRole {
    Applicant,
    Department,
}

#[derive(Clone)]
enum SavedValue {
    None,
    Text(String),
    Name(Vec<u8>),
}

struct InheritedField {
    field_type: FieldType,
    leaf_name: String,
    full_name: String,
    alt_name: String,
    mapping_name: String,
    flags: u32,
    value: SavedValue,
    default_value: SavedValue,
    da: Option<Vec<u8>>,
    quadding: Option<i32>,
    max_len: Option<i32>,
    options: Vec<String>,
}

/// Geometry of an output page needed to place or rotate widget appearances.
#[derive(Clone, Copy)]
pub struct PageGeom {
    pub width: f32,
    pub height: f32,
    pub rotate: u16,
    pub page_ref: Ref,
}

impl PageGeom {
    fn shown_size(self) -> (f32, f32) {
        if self.rotate % 180 == 90 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

/// Result of preserving and updating interactive form fields across all output pages.
pub struct FormBuildOutput {
    pub field_refs: Vec<Ref>,
    pub helv_font_ref: Option<Ref>,
    pub need_appearances: bool,
    /// Widgets of a kind that is not written (a signature field), for a warning.
    pub skipped_fields: usize,
}

type WidgetCacheKey = (u32, u32, OverlayKind);

fn widget_cache_key(w: f32, h: f32, kind: OverlayKind) -> WidgetCacheKey {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let q = |v: f32| (v * 100.0).round() as u32;
    (q(w), q(h), kind)
}

/// Preserves existing `/AcroForm` widgets on each output page, applies `form` updates, adds missing
/// studio fields on page 1, and appends widget references to `page_annots`.
#[allow(clippy::too_many_arguments)]
pub fn process_forms(
    out: &mut Pdf,
    next: &mut Ref,
    parsed: &BTreeMap<usize, ReadPdf>,
    pages: &[PagePlan],
    geoms: &[PageGeom],
    page_annots: &mut [Vec<Ref>],
    form: &WireForm,
    make_overlay: &mut OverlayMaker<'_>,
    total_annots: &mut usize,
    deadline: &Deadline,
) -> Result<FormBuildOutput, String> {
    let mut field_refs = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();
    let mut widget_cache: HashMap<WidgetCacheKey, Ref> = HashMap::new();
    let (mut matched_applicant, mut matched_dept) = (false, false);
    let (mut wrote_custom_ap, mut needs_viewer_ap) = (false, false);
    let mut helv_ref: Option<Ref> = None;
    let mut skipped_fields = 0usize;

    for (out_idx, (plan, geom)) in pages.iter().zip(geoms).enumerate() {
        deadline.check()?;
        let Some(pdf) = parsed.get(&plan.source) else {
            continue;
        };
        let Some(src_page) = pdf.pages().get(plan.index) else {
            continue;
        };
        let Some(annots) = src_page.raw().get::<Array<'_>>(b"Annots") else {
            continue;
        };
        let crop = src_page.intersected_crop_box();
        #[allow(clippy::cast_possible_truncation)]
        let (dx, dy) = (-(crop.x0 as f32), -(crop.y0 as f32));

        for annot in annots.iter::<Dict<'_>>() {
            if annot.get::<Name<'_>>(b"Subtype").as_ref().map(Name::as_str) != Some("Widget") {
                continue;
            }
            let Some(rect) = read_shifted_rect(&annot, dx, dy) else {
                continue;
            };
            let Some(mut field) = read_inherited_field(&annot)? else {
                skipped_fields += 1;
                continue;
            };
            if field_refs.len() >= MAX_FORM_FIELDS {
                return Err(format!(
                    "PDF 양식 필드 수가 한도({MAX_FORM_FIELDS})를 초과합니다."
                ));
            }
            let role = match_field_role(&field);
            let mut ap_stream: Option<Ref> = None;

            if field.field_type == FieldType::Text {
                if role == Some(FormRole::Applicant) {
                    matched_applicant = true;
                    if let Some(val) = &form.applicant {
                        field.value = SavedValue::Text(val.clone());
                        let kind = if field.mapping_name == EO_LABELED_TAG {
                            OverlayKind::ApplicantLabeled
                        } else {
                            OverlayKind::ApplicantField
                        };
                        ap_stream = Some(build_widget_ap(
                            out,
                            next,
                            rect,
                            geom.rotate,
                            val,
                            kind,
                            &mut widget_cache,
                            make_overlay,
                        )?);
                        wrote_custom_ap = true;
                    }
                } else if role == Some(FormRole::Department) {
                    matched_dept = true;
                    if let Some(val) = &form.department {
                        field.value = SavedValue::Text(val.clone());
                        let kind = if field.mapping_name == EO_LABELED_TAG {
                            OverlayKind::DepartmentLabeled
                        } else {
                            OverlayKind::DepartmentField
                        };
                        ap_stream = Some(build_widget_ap(
                            out,
                            next,
                            rect,
                            geom.rotate,
                            val,
                            kind,
                            &mut widget_cache,
                            make_overlay,
                        )?);
                        wrote_custom_ap = true;
                    }
                }
            }

            if ap_stream.is_none() {
                ap_stream = fallback_ascii_ap(out, next, &mut helv_ref, &field, rect);
                if ap_stream.is_none()
                    && matches!(&field.value, SavedValue::Text(s) if !s.is_empty())
                {
                    needs_viewer_ap = true;
                }
            }

            let unique_name = allocate_field_name(&field, &mut used_names);
            let annot_flags = annot
                .get::<u32>(b"F")
                .map(AnnotationFlags::from_bits_truncate)
                .unwrap_or(AnnotationFlags::PRINT);
            let widget_ref = write_merged_widget(
                out,
                next,
                &field,
                &unique_name,
                rect,
                annot_flags,
                geom.page_ref,
                ap_stream,
            );
            *total_annots += 1;
            if *total_annots > MAX_TOTAL_ANNOTS {
                return Err(format!(
                    "문서 전체의 주석 수가 한도({MAX_TOTAL_ANNOTS})를 초과합니다."
                ));
            }
            page_annots[out_idx].push(widget_ref);
            field_refs.push(widget_ref);
        }
    }

    if let Some(&first_geom) = geoms.first() {
        let add_app =
            !matched_applicant && form.applicant.as_deref().is_some_and(|v| !v.is_empty());
        let add_dept = !matched_dept && form.department.as_deref().is_some_and(|v| !v.is_empty());
        let mut slot = 0usize;
        for (should_add, name, alt, value_opt, kind) in [
            (
                add_dept,
                "department",
                "부서",
                form.department.as_deref(),
                OverlayKind::DepartmentLabeled,
            ),
            (
                add_app,
                "applicant",
                "신청인",
                form.applicant.as_deref(),
                OverlayKind::ApplicantLabeled,
            ),
        ] {
            if !should_add {
                continue;
            }
            let value = value_opt.unwrap_or_default();
            if field_refs.len() >= MAX_FORM_FIELDS {
                return Err(format!(
                    "PDF 양식 필드 수가 한도({MAX_FORM_FIELDS})를 초과합니다."
                ));
            }
            let rect = new_widget_rect(first_geom, slot);
            slot += 1;
            let ap_ref = build_widget_ap(
                out,
                next,
                rect,
                first_geom.rotate,
                value,
                kind,
                &mut widget_cache,
                make_overlay,
            )?;
            wrote_custom_ap = true;
            let spec = InheritedField {
                field_type: FieldType::Text,
                leaf_name: name.to_string(),
                full_name: name.to_string(),
                alt_name: alt.to_string(),
                mapping_name: EO_LABELED_TAG.to_string(),
                flags: 0,
                value: SavedValue::Text(value.to_string()),
                default_value: SavedValue::None,
                da: None,
                quadding: None,
                max_len: None,
                options: Vec::new(),
            };
            let unique_name = allocate_field_name(&spec, &mut used_names);
            let widget_ref = write_merged_widget(
                out,
                next,
                &spec,
                &unique_name,
                rect,
                AnnotationFlags::PRINT,
                first_geom.page_ref,
                Some(ap_ref),
            );
            *total_annots += 1;
            if *total_annots > MAX_TOTAL_ANNOTS {
                return Err(format!(
                    "문서 전체의 주석 수가 한도({MAX_TOTAL_ANNOTS})를 초과합니다."
                ));
            }
            page_annots[0].push(widget_ref);
            field_refs.push(widget_ref);
        }
    }

    if !field_refs.is_empty() && helv_ref.is_none() {
        let f = next.bump();
        out.type1_font(f).base_font(WName(b"Helvetica"));
        helv_ref = Some(f);
    }

    Ok(FormBuildOutput {
        field_refs,
        helv_font_ref: helv_ref,
        need_appearances: needs_viewer_ap && !wrote_custom_ap,
        skipped_fields,
    })
}

/// Writes `/Catalog /AcroForm` when `forms.field_refs` is non-empty.
pub fn write_catalog_acroform(
    catalog: &mut hayro_write::pdf_writer::writers::Catalog<'_>,
    forms: &FormBuildOutput,
) {
    if forms.field_refs.is_empty() {
        return;
    }
    let mut acro = catalog.form();
    acro.fields(forms.field_refs.iter().copied())
        .default_appearance(Str(DEFAULT_DA));
    if forms.need_appearances {
        acro.pair(WName(b"NeedAppearances"), true);
    }
    if let Some(helv) = forms.helv_font_ref {
        acro.default_resources().fonts().pair(WName(b"Helv"), helv);
    }
    acro.finish();
}

fn read_inherited_field(widget: &Dict<'_>) -> Result<Option<InheritedField>, String> {
    let mut chain: Vec<Dict<'_>> = Vec::with_capacity(4);
    let mut seen = HashSet::new();
    let mut cur = Some(widget.clone());
    while let Some(d) = cur {
        if chain.len() >= MAX_FIELD_DEPTH {
            return Err(format!(
                "PDF 양식 필드 계층이 허용 깊이({MAX_FIELD_DEPTH}단계)를 초과합니다."
            ));
        }
        if d.obj_id().is_some_and(|id| !seen.insert(id)) {
            return Err("PDF 양식 필드 계층에 순환 참조가 있습니다.".to_string());
        }
        if let Some(parent_ref) = d.get_ref(b"Parent") {
            if seen.contains(&ObjectIdentifier::from(parent_ref)) {
                return Err("PDF 양식 필드 계층에 순환 참조가 있습니다.".to_string());
            }
        }
        let next_parent = d.get::<Dict<'_>>(b"Parent");
        chain.push(d);
        cur = next_parent;
    }
    let pick_name_key = |key: &[u8]| -> Option<String> {
        chain
            .iter()
            .find_map(|d| d.get::<Name<'_>>(key).map(|n| n.as_str().to_string()))
    };
    let field_type = match pick_name_key(b"FT").as_deref() {
        Some("Tx") => FieldType::Text,
        Some("Btn") => FieldType::Button,
        Some("Ch") => FieldType::Choice,
        // Cryptographic `/Sig` byte-range signatures cannot survive object renumbering.
        _ => return Ok(None),
    };
    let mut parts: Vec<String> = chain
        .iter()
        .rev()
        .filter_map(|d| {
            d.get::<hayro_syntax::object::String<'_>>(b"T")
                .map(|s| decode_pdf_text(s.as_bytes(), false, MAX_NAME_CHARS))
                .filter(|s| !s.is_empty())
        })
        .collect();
    let leaf_name = parts.last().cloned().unwrap_or_default();
    if parts.is_empty() {
        parts.push("field".to_string());
    }
    let full_name = parts.join("_");
    let alt_name = chain
        .iter()
        .find_map(|d| {
            d.get::<hayro_syntax::object::String<'_>>(b"TU")
                .map(|s| decode_pdf_text(s.as_bytes(), false, MAX_NAME_CHARS))
        })
        .unwrap_or_default();
    let mapping_name = chain
        .iter()
        .find_map(|d| {
            d.get::<hayro_syntax::object::String<'_>>(b"TM")
                .map(|s| decode_pdf_text(s.as_bytes(), false, MAX_NAME_CHARS))
        })
        .unwrap_or_default();
    let flags = chain.iter().find_map(|d| d.get::<u32>(b"Ff")).unwrap_or(0);
    let value = chain
        .iter()
        .find_map(|d| read_saved_value(d, b"V"))
        .unwrap_or(SavedValue::None);
    let default_value = chain
        .iter()
        .find_map(|d| read_saved_value(d, b"DV"))
        .unwrap_or(SavedValue::None);
    let da = chain.iter().find_map(|d| {
        d.get::<hayro_syntax::object::String<'_>>(b"DA")
            .map(|s| s.as_bytes().to_vec())
    });
    let quadding = chain.iter().find_map(|d| d.get::<i32>(b"Q"));
    let max_len = chain.iter().find_map(|d| d.get::<i32>(b"MaxLen"));
    let options = chain
        .iter()
        .find_map(|d| d.get::<Array<'_>>(b"Opt"))
        .map(|arr| {
            arr.iter::<hayro_syntax::object::String<'_>>()
                .take(256)
                .map(|s| decode_pdf_text(s.as_bytes(), false, MAX_NAME_CHARS))
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(InheritedField {
        field_type,
        leaf_name,
        full_name,
        alt_name,
        mapping_name,
        flags,
        value,
        default_value,
        da,
        quadding,
        max_len,
        options,
    }))
}

fn read_saved_value(dict: &Dict<'_>, key: &[u8]) -> Option<SavedValue> {
    if let Some(s) = dict.get::<hayro_syntax::object::String<'_>>(key) {
        return Some(SavedValue::Text(decode_pdf_text(
            s.as_bytes(),
            true,
            MAX_FIELD_VALUE_CHARS,
        )));
    }
    dict.get::<Name<'_>>(key)
        .map(|n| SavedValue::Name(n.as_ref().to_vec()))
}

fn match_field_role(field: &InheritedField) -> Option<FormRole> {
    for raw in [&field.leaf_name, &field.full_name, &field.alt_name] {
        let lower = raw.trim().to_ascii_lowercase();
        if lower.is_empty() {
            continue;
        }
        let tail = lower
            .rsplit_once(['.', '_'])
            .map_or(lower.as_str(), |(_, t)| t);
        if matches!(
            tail,
            "applicant" | "신청인" | "성명" | "이름" | "신청자" | "작성자"
        ) {
            return Some(FormRole::Applicant);
        }
        if matches!(tail, "dept" | "department" | "부서" | "부서명" | "소속") {
            return Some(FormRole::Department);
        }
    }
    None
}

fn allocate_field_name(field: &InheritedField, used: &mut HashSet<String>) -> String {
    let base: String = field
        .full_name
        .chars()
        .map(|c| if c == '.' || c.is_control() { '_' } else { c })
        .collect();
    let base = if base.is_empty() {
        "field".to_string()
    } else {
        base
    };
    if used.insert(base.clone()) {
        return base;
    }
    for n in 2..=10_000 {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    base
}

fn new_widget_rect(geom: PageGeom, slot: usize) -> [f32; 4] {
    let (shown_w, shown_h) = geom.shown_size();
    let box_w = (shown_w - 72.0).clamp(64.0, 180.0);
    let box_h = 20.0_f32.min((shown_h * 0.12).max(12.0));
    let x0 = 36.0_f32.min((shown_w - box_w) * 0.5).max(4.0);
    let base_y = 24.0_f32.min((shown_h - box_h * 2.5).max(4.0));
    #[allow(clippy::cast_precision_loss)]
    let y0 = base_y + (slot as f32) * (box_h + 4.0);
    let (x1, y1) = (x0 + box_w, y0 + box_h);
    let m = upright_matrix(geom.rotate, geom.width, geom.height);
    let map = |x: f32, y: f32| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
    let (ax, ay) = map(x0, y0);
    let (bx, by) = map(x1, y1);
    [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
}

#[allow(clippy::too_many_arguments)]
fn build_widget_ap(
    out: &mut Pdf,
    next: &mut Ref,
    rect: [f32; 4],
    rotate: u16,
    value: &str,
    kind: OverlayKind,
    cache: &mut HashMap<WidgetCacheKey, Ref>,
    make_overlay: &mut OverlayMaker<'_>,
) -> Result<Ref, String> {
    let (rw, rh) = ((rect[2] - rect[0]).max(1.0), (rect[3] - rect[1]).max(1.0));
    if value.is_empty() {
        let empty_ref = next.bump();
        out.form_xobject(empty_ref, b"")
            .bbox(Rect::new(0.0, 0.0, rw, rh));
        return Ok(empty_ref);
    }
    let (shown_w, shown_h) = if rotate % 180 == 90 {
        (rh, rw)
    } else {
        (rw, rh)
    };
    let key = widget_cache_key(shown_w, shown_h, kind);
    let inner_ref = if let Some(&cached) = cache.get(&key) {
        cached
    } else {
        if cache.len() >= MAX_WIDGET_OVERLAYS {
            return Err(format!(
                "양식 필드 모양 수가 한도({MAX_WIDGET_OVERLAYS})를 초과합니다."
            ));
        }
        let bytes = Arc::new(make_overlay(shown_w, shown_h, kind)?);
        let pdf = open_pdf(&bytes)?;
        let copied = extract(
            &pdf,
            Box::new(|| next.bump()),
            ChunkSettings::default(),
            &[ExtractionQuery::new_xobject(0)],
        )
        .map_err(|_| "양식 필드 모양을 복사하지 못했습니다.".to_string())?;
        out.extend(&copied.chunk);
        let form_ref = copied
            .root_refs
            .into_iter()
            .next()
            .and_then(Result::ok)
            .ok_or("양식 필드 모양을 복사하지 못했습니다.")?;
        cache.insert(key, form_ref);
        form_ref
    };
    if rotate == 0 || rotate == 360 {
        return Ok(inner_ref);
    }
    let wrap_ref = next.bump();
    let mut content = Content::new();
    content
        .save_state()
        .transform(upright_matrix(rotate, rw, rh))
        .x_object(WName(b"Fm0"))
        .restore_state();
    let stream = content.finish();
    let mut form = out.form_xobject(wrap_ref, &stream);
    form.bbox(Rect::new(0.0, 0.0, rw, rh));
    form.resources().x_objects().pair(WName(b"Fm0"), inner_ref);
    form.finish();
    Ok(wrap_ref)
}

fn fallback_ascii_ap(
    out: &mut Pdf,
    next: &mut Ref,
    helv_ref: &mut Option<Ref>,
    field: &InheritedField,
    rect: [f32; 4],
) -> Option<Ref> {
    let SavedValue::Text(text) = &field.value else {
        return None;
    };
    if text.is_empty() || !text.is_ascii() || field.field_type != FieldType::Text {
        return None;
    }
    let font_ref = *helv_ref.get_or_insert_with(|| {
        let f = next.bump();
        out.type1_font(f).base_font(WName(b"Helvetica"));
        f
    });
    let (rw, rh) = ((rect[2] - rect[0]).max(1.0), (rect[3] - rect[1]).max(1.0));
    let font_size = (rh * 0.58).clamp(7.0, 12.0);
    let baseline = (rh - font_size) * 0.5 + font_size * 0.15;
    let mut content = Content::new();
    content
        .begin_text()
        .set_font(WName(b"Helv"), font_size)
        .next_line(3.0, baseline.max(2.0))
        .show(Str(text.as_bytes()))
        .end_text();
    let ap_ref = next.bump();
    let stream = content.finish();
    let mut form = out.form_xobject(ap_ref, &stream);
    form.bbox(Rect::new(0.0, 0.0, rw, rh));
    form.resources().fonts().pair(WName(b"Helv"), font_ref);
    form.finish();
    Some(ap_ref)
}

#[allow(clippy::too_many_arguments)]
fn write_merged_widget(
    out: &mut Pdf,
    next: &mut Ref,
    field: &InheritedField,
    unique_name: &str,
    rect: [f32; 4],
    annot_flags: AnnotationFlags,
    page_ref: Ref,
    ap_stream: Option<Ref>,
) -> Ref {
    let widget_ref = next.bump();
    let mut f = out.form_field(widget_ref);
    f.field_type(field.field_type)
        .partial_name(TextStr(unique_name))
        .field_flags(FieldFlags::from_bits_truncate(field.flags));
    if !field.alt_name.is_empty() {
        f.alternate_name(TextStr(&field.alt_name));
    }
    if !field.mapping_name.is_empty() {
        f.mapping_name(TextStr(&field.mapping_name));
    }
    let da = field.da.as_deref().unwrap_or(DEFAULT_DA);
    if matches!(field.field_type, FieldType::Text | FieldType::Choice) {
        f.vartext_default_appearance(Str(da));
        if let Some(q) = field.quadding {
            f.pair(WName(b"Q"), q.clamp(0, 2));
        }
    }
    if let Some(max_len) = field.max_len.filter(|&m| m > 0) {
        if field.field_type == FieldType::Text {
            f.text_max_len(max_len);
        }
    }
    if field.field_type == FieldType::Choice && !field.options.is_empty() {
        f.choice_options()
            .options(field.options.iter().map(|s| TextStr(s)));
    }
    write_field_value(&mut f, b"V", &field.value);
    write_field_value(&mut f, b"DV", &field.default_value);

    let mut annot = f.into_annotation();
    annot
        .subtype(AnnotationType::Widget)
        .rect(Rect::new(rect[0], rect[1], rect[2], rect[3]))
        .flags(annot_flags)
        .page(page_ref);
    if let SavedValue::Name(state) = &field.value {
        if field.field_type == FieldType::Button {
            annot.appearance_state(WName(state));
        }
    }
    if let Some(ap_ref) = ap_stream {
        annot.appearance().normal().stream(ap_ref);
    }
    annot.finish();
    widget_ref
}

fn write_field_value(
    field: &mut hayro_write::pdf_writer::writers::Field<'_>,
    key: &[u8],
    value: &SavedValue,
) {
    match value {
        SavedValue::None => {}
        SavedValue::Text(s) => {
            field.pair(WName(key), TextStr(s));
        }
        SavedValue::Name(n) => {
            field.pair(WName(key), WName(n));
        }
    }
}

//! Security inspection and unsigned-document sanitization for ISO 32000-1 incremental PDF saves
//! (PD-349).
//!
//! Detects `/Catalog /Names /JavaScript`, `/Catalog /AcroForm /XFA`, `/Catalog /OpenAction`,
//! `/Catalog /AA`, `/Catalog /Collection`, `/Page /AA`, page `/Annots` and `/AcroForm /Fields`
//! `/AA` or unsafe `/A` (via `pdf_edit_incremental_actions`), and `/Catalog /Outlines` `/AA` or
//! unsafe `/A`. Unsigned tagged PDFs strip these entries while signed PDFs keep original bytes
//! untouched and emit a warning.

use crate::pdf_edit_budget::{MAX_FORM_FIELDS, MAX_PAGE_ANNOTS, MAX_VALUE_DEPTH};
use crate::pdf_edit_carry::Warnings;
use crate::pdf_edit_incremental_actions::{
    aa_dict_has_js, inspect_action_entry, inspect_outlines, sanitize_outlines, ActionInspect,
};
use crate::pdf_edit_incremental_forms::{
    has_non_null, load_obj_val, read_raw_array, resolve_raw_dict, MAX_FIELD_TREE_DEPTH,
};
use crate::pdf_edit_incremental_val::IncVal;
use crate::pdf_edit_preserve::{too_many_page_annots, PageDestMap};
use crate::pdf_edit_value::{Budget, Refused};
use hayro_syntax::object::{Array, Dict, MaybeRef, Object, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::{BTreeMap, HashMap, HashSet};

const MSG_STRIPPED_SCRIPT: &str = "문서 스크립트는 보안을 위해 저장하지 않았습니다.";
const MSG_STRIPPED_AUTO_ACTION: &str = "열 때 자동 실행 동작은 보안을 위해 저장하지 않았습니다.";
const MSG_STRIPPED_ITEM_ACTION: &str =
    "주석·북마크의 안전하지 않은 동작은 보안을 위해 저장하지 않았습니다.";
const MSG_STRIPPED_COLLECTION: &str = "PDF 포트폴리오 구성은 저장하지 않았습니다.";

pub(crate) struct DangerousEntries {
    pub has_js: bool,
    pub has_xfa: bool,
    pub has_open_action: bool,
    pub has_catalog_aa: bool,
    pub has_collection: bool,
    pub page_aa: Vec<usize>,
    pub has_unsafe_annot_action: bool,
    pub has_unsafe_outline_action: bool,
    pub has_action_js: bool,
}

impl DangerousEntries {
    pub fn any(&self) -> bool {
        self.has_js
            || self.has_xfa
            || self.has_open_action
            || self.has_catalog_aa
            || self.has_collection
            || !self.page_aa.is_empty()
            || self.has_unsafe_annot_action
            || self.has_unsafe_outline_action
    }
}

pub(crate) fn inspect_dangerous(
    pdf: &ReadPdf,
    catalog: &Dict<'_>,
    dest_map: &PageDestMap,
) -> Result<DangerousEntries, String> {
    let has_js = catalog
        .get::<Dict<'_>>(b"Names")
        .is_some_and(|names| has_non_null(&names, b"JavaScript"));
    let has_xfa = catalog
        .get::<Dict<'_>>(b"AcroForm")
        .is_some_and(|acro| has_non_null(&acro, b"XFA"));
    let has_open_action = has_non_null(catalog, b"OpenAction");
    let has_catalog_aa = has_non_null(catalog, b"AA");
    let has_collection = has_non_null(catalog, b"Collection");

    let mut page_aa = Vec::new();
    let mut has_unsafe_annot_action = false;
    let mut has_action_js = false;

    for (idx, page) in pdf.pages().iter().enumerate() {
        if has_non_null(page.raw(), b"AA") {
            page_aa.push(idx);
        }
        if let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") {
            for (annot_idx, raw_annot) in annots.raw_iter().enumerate() {
                if annot_idx >= MAX_PAGE_ANNOTS {
                    return Err(too_many_page_annots());
                }
                let (_, Some(annot)) = resolve_raw_dict(pdf, raw_annot) else {
                    continue;
                };
                if has_non_null(&annot, b"AA") {
                    has_unsafe_annot_action = true;
                    if aa_dict_has_js(&annot) {
                        has_action_js = true;
                    }
                }
                if let ActionInspect::Unsafe { has_js } =
                    inspect_action_entry(pdf, dest_map, &annot)?
                {
                    has_unsafe_annot_action = true;
                    has_action_js |= has_js;
                }
            }
        }
    }

    walk_acroform_fields(pdf, catalog, &mut |_, field| {
        if has_non_null(field, b"AA") {
            has_unsafe_annot_action = true;
            if aa_dict_has_js(field) {
                has_action_js = true;
            }
        }
        if let ActionInspect::Unsafe { has_js } = inspect_action_entry(pdf, dest_map, field)? {
            has_unsafe_annot_action = true;
            has_action_js |= has_js;
        }
        Ok(())
    })?;

    let mut has_unsafe_outline_action = false;
    inspect_outlines(
        pdf,
        catalog,
        dest_map,
        &mut has_unsafe_outline_action,
        &mut has_action_js,
    )?;

    Ok(DangerousEntries {
        has_js,
        has_xfa,
        has_open_action,
        has_catalog_aa,
        has_collection,
        page_aa,
        has_unsafe_annot_action,
        has_unsafe_outline_action,
        has_action_js,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_unsigned_catalog_sanitization(
    pdf: &ReadPdf,
    catalog_dict: &Dict<'_>,
    catalog_val: &mut IncVal,
    catalog_dirty: &mut bool,
    dangerous: &DangerousEntries,
    dest_map: &PageDestMap,
    page_ids: &[ObjectIdentifier],
    page_dirty: &mut [bool],
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    budget: &mut Budget,
    warnings: &mut Warnings,
) -> Result<(), String> {
    if dangerous.has_js {
        if let Some(names_dict) = catalog_dict.get::<Dict<'_>>(b"Names") {
            if let Some(names_id) = catalog_dict.get_ref(b"Names").map(ObjectIdentifier::from) {
                let mut names_val =
                    load_obj_val(pdf, names_id, &names_dict, modified_objs, budget)?.clone();
                names_val.remove(b"JavaScript");
                if names_val.as_dict().is_some_and(BTreeMap::is_empty) {
                    modified_objs.remove(&names_id);
                    catalog_val.remove(b"Names");
                    *catalog_dirty = true;
                } else {
                    modified_objs.insert(names_id, names_val);
                }
            } else if let Some(IncVal::Dict(mut updated)) = catalog_val.get(b"Names").cloned() {
                updated.remove(b"JavaScript".as_slice());
                if updated.is_empty() {
                    catalog_val.remove(b"Names");
                } else {
                    catalog_val.set(b"Names", IncVal::Dict(updated));
                }
                *catalog_dirty = true;
            }
        }
    }

    if dangerous.has_xfa {
        if let Some(acro_dict) = catalog_dict.get::<Dict<'_>>(b"AcroForm") {
            if let Some(acro_id) = catalog_dict
                .get_ref(b"AcroForm")
                .map(ObjectIdentifier::from)
            {
                let acro_val = load_obj_val(pdf, acro_id, &acro_dict, modified_objs, budget)?;
                acro_val.remove(b"XFA");
            } else if let Some(acro_val) = catalog_val.get_mut(b"AcroForm") {
                acro_val.remove(b"XFA");
                *catalog_dirty = true;
            }
        }
    }

    if dangerous.has_js || dangerous.has_xfa || dangerous.has_action_js {
        warnings.push(MSG_STRIPPED_SCRIPT.to_string());
    }

    if dangerous.has_open_action {
        catalog_val.remove(b"OpenAction");
        *catalog_dirty = true;
    }
    if dangerous.has_catalog_aa {
        catalog_val.remove(b"AA");
        *catalog_dirty = true;
    }
    for &idx in &dangerous.page_aa {
        if let Some(page) = pdf.pages().get(idx) {
            let page_id = page_ids[idx];
            let page_val = load_obj_val(pdf, page_id, page.raw(), modified_objs, budget)?;
            page_val.remove(b"AA");
            page_dirty[idx] = true;
        }
    }
    if dangerous.has_open_action || dangerous.has_catalog_aa || !dangerous.page_aa.is_empty() {
        warnings.push(MSG_STRIPPED_AUTO_ACTION.to_string());
    }

    if dangerous.has_unsafe_outline_action {
        sanitize_outlines(
            pdf,
            catalog_dict,
            catalog_val,
            catalog_dirty,
            dest_map,
            modified_objs,
            budget,
        )?;
    }
    if dangerous.has_unsafe_annot_action || dangerous.has_unsafe_outline_action {
        warnings.push(MSG_STRIPPED_ITEM_ACTION.to_string());
    }

    if dangerous.has_collection {
        catalog_val.remove(b"Collection");
        *catalog_dirty = true;
        warnings.push(MSG_STRIPPED_COLLECTION.to_string());
    }
    Ok(())
}

/// Sanitizes `/AA` and unsafe `/A` on existing page `/Annots` (indirect and inline) and
/// `/AcroForm /Fields` + `/Kids` (indirect and inline) after `apply_incremental_forms` so widget
/// updates and removals compose.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sanitize_page_annots_and_fields(
    pdf: &ReadPdf,
    catalog_dict: &Dict<'_>,
    catalog_val: &mut IncVal,
    catalog_dirty: &mut bool,
    dest_map: &PageDestMap,
    inline_annot_patches: &mut [HashMap<usize, IncVal>],
    page_dirty: &mut [bool],
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    budget: &mut Budget,
) -> Result<(), String> {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") else {
            continue;
        };
        for (annot_idx, raw_annot) in annots.raw_iter().enumerate() {
            if annot_idx >= MAX_PAGE_ANNOTS {
                return Err(too_many_page_annots());
            }
            let (annot_id_opt, Some(annot)) = resolve_raw_dict(pdf, raw_annot) else {
                continue;
            };
            let remove_aa = has_non_null(&annot, b"AA");
            let remove_a = inspect_action_entry(pdf, dest_map, &annot)?.is_unsafe();
            if !remove_aa && !remove_a {
                continue;
            }
            if let Some(annot_id) = annot_id_opt {
                let annot_val = load_obj_val(pdf, annot_id, &annot, modified_objs, budget)?;
                if remove_aa {
                    annot_val.remove(b"AA");
                }
                if remove_a {
                    annot_val.remove(b"A");
                }
            } else {
                if let Some(existing) = inline_annot_patches[page_idx].get_mut(&annot_idx) {
                    if remove_aa {
                        existing.remove(b"AA");
                    }
                    if remove_a {
                        existing.remove(b"A");
                    }
                } else {
                    let mut val = IncVal::read_dict(&annot, budget)
                        .map_err(|r| r.sentence("PDF 주석 사전"))?;
                    if remove_aa {
                        val.remove(b"AA");
                    }
                    if remove_a {
                        val.remove(b"A");
                    }
                    inline_annot_patches[page_idx].insert(annot_idx, val);
                }
                page_dirty[page_idx] = true;
            }
        }
    }

    sanitize_acroform_fields(
        pdf,
        catalog_dict,
        catalog_val,
        catalog_dirty,
        dest_map,
        modified_objs,
        budget,
    )
}

fn sanitize_acroform_fields(
    pdf: &ReadPdf,
    catalog_dict: &Dict<'_>,
    catalog_val: &mut IncVal,
    catalog_dirty: &mut bool,
    dest_map: &PageDestMap,
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    budget: &mut Budget,
) -> Result<(), String> {
    if let Some(acro) = catalog_dict.get::<Dict<'_>>(b"AcroForm") {
        if let Some(fields_arr) = acro.get::<Array<'_>>(b"Fields") {
            if inline_array_needs_sanitize(pdf, dest_map, &fields_arr)? {
                if let Some(arr_ref) = acro.get_ref(b"Fields") {
                    let arr_id = ObjectIdentifier::from(arr_ref);
                    let arr_vals = load_array_val(pdf, arr_id, &fields_arr, modified_objs, budget)?;
                    sanitize_inline_array_vals(pdf, dest_map, &fields_arr, arr_vals)?;
                } else if let Some(acro_ref) = catalog_dict.get_ref(b"AcroForm") {
                    let acro_id = ObjectIdentifier::from(acro_ref);
                    let acro_val = load_obj_val(pdf, acro_id, &acro, modified_objs, budget)?;
                    if let Some(IncVal::Array(arr_vals)) = acro_val.get_mut(b"Fields") {
                        sanitize_inline_array_vals(pdf, dest_map, &fields_arr, arr_vals)?;
                    }
                } else if let Some(IncVal::Array(arr_vals)) = catalog_val
                    .get_mut(b"AcroForm")
                    .and_then(|a| a.get_mut(b"Fields"))
                {
                    sanitize_inline_array_vals(pdf, dest_map, &fields_arr, arr_vals)?;
                    *catalog_dirty = true;
                }
            }
        }
    }

    let mut seen_arrays = HashSet::new();
    walk_acroform_fields(pdf, catalog_dict, &mut |field_id, field| {
        if let Some(id) = field_id {
            if inline_field_needs_sanitize(pdf, dest_map, field, 0)? {
                let val = load_obj_val(pdf, id, field, modified_objs, budget)?;
                sanitize_inline_field_val(pdf, dest_map, val, field, 0)?;
            }
        }
        if let Some(kids_ref) = field.get_ref(b"Kids") {
            let kids_id = ObjectIdentifier::from(kids_ref);
            if seen_arrays.insert(kids_id) {
                if let Some(kids_arr) = field.get::<Array<'_>>(b"Kids") {
                    if inline_array_needs_sanitize(pdf, dest_map, &kids_arr)? {
                        let arr_vals =
                            load_array_val(pdf, kids_id, &kids_arr, modified_objs, budget)?;
                        sanitize_inline_array_vals(pdf, dest_map, &kids_arr, arr_vals)?;
                    }
                }
            }
        }
        Ok(())
    })
}

fn load_array_val<'m>(
    pdf: &ReadPdf,
    id: ObjectIdentifier,
    arr: &Array<'_>,
    modified_objs: &'m mut BTreeMap<ObjectIdentifier, IncVal>,
    budget: &mut Budget,
) -> Result<&'m mut Vec<IncVal>, String> {
    if let std::collections::btree_map::Entry::Vacant(e) = modified_objs.entry(id) {
        let base = pdf.xref().get::<Array<'_>>(id);
        let target = base.as_ref().unwrap_or(arr);
        let items = read_raw_array(target, budget)?;
        e.insert(IncVal::Array(items));
    }
    match modified_objs.get_mut(&id) {
        Some(IncVal::Array(items)) => Ok(items),
        _ => Err("PDF 배열 객체 형식이 올바르지 않습니다.".to_string()),
    }
}

fn inline_array_needs_sanitize(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    arr: &Array<'_>,
) -> Result<bool, String> {
    for raw in arr.raw_iter() {
        if let MaybeRef::NotRef(Object::Dict(dict)) = raw {
            if inline_field_needs_sanitize(pdf, dest_map, &dict, 0)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn sanitize_inline_array_vals(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    arr_dict: &Array<'_>,
    arr_vals: &mut [IncVal],
) -> Result<(), String> {
    for (raw, val) in arr_dict.raw_iter().zip(arr_vals.iter_mut()) {
        if let MaybeRef::NotRef(Object::Dict(dict)) = raw {
            sanitize_inline_field_val(pdf, dest_map, val, &dict, 0)?;
        }
    }
    Ok(())
}

fn inline_field_needs_sanitize(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    dict: &Dict<'_>,
    inline_depth: usize,
) -> Result<bool, String> {
    if inline_depth >= MAX_VALUE_DEPTH {
        return Err(Refused::Deep.sentence("PDF 양식 사전"));
    }
    if has_non_null(dict, b"AA") || inspect_action_entry(pdf, dest_map, dict)?.is_unsafe() {
        return Ok(true);
    }
    if dict.get_ref(b"Kids").is_none() {
        if let Some(kids) = dict.get::<Array<'_>>(b"Kids") {
            for raw in kids.raw_iter() {
                if let MaybeRef::NotRef(Object::Dict(kid)) = raw {
                    if inline_field_needs_sanitize(pdf, dest_map, &kid, inline_depth + 1)? {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

fn sanitize_inline_field_val(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    val: &mut IncVal,
    dict: &Dict<'_>,
    inline_depth: usize,
) -> Result<(), String> {
    if inline_depth >= MAX_VALUE_DEPTH {
        return Err(Refused::Deep.sentence("PDF 양식 사전"));
    }
    if has_non_null(dict, b"AA") {
        val.remove(b"AA");
    }
    if inspect_action_entry(pdf, dest_map, dict)?.is_unsafe() {
        val.remove(b"A");
    }
    if dict.get_ref(b"Kids").is_none() {
        if let (Some(kids_dict), Some(IncVal::Array(kids_vals))) =
            (dict.get::<Array<'_>>(b"Kids"), val.get_mut(b"Kids"))
        {
            for (raw, kid_val) in kids_dict.raw_iter().zip(kids_vals.iter_mut()) {
                if let MaybeRef::NotRef(Object::Dict(kid_dict)) = raw {
                    sanitize_inline_field_val(pdf, dest_map, kid_val, &kid_dict, inline_depth + 1)?;
                }
            }
        }
    }
    Ok(())
}

fn walk_acroform_fields<F>(pdf: &ReadPdf, catalog: &Dict<'_>, visit: &mut F) -> Result<(), String>
where
    F: FnMut(Option<ObjectIdentifier>, &Dict<'_>) -> Result<(), String>,
{
    let Some(acro) = catalog.get::<Dict<'_>>(b"AcroForm") else {
        return Ok(());
    };
    let Some(fields) = acro.get::<Array<'_>>(b"Fields") else {
        return Ok(());
    };
    let mut seen = HashSet::new();
    let mut visited = 0usize;
    for raw in fields.raw_iter() {
        let (field_id, Some(field)) = resolve_raw_dict(pdf, raw) else {
            continue;
        };
        walk_field_node(pdf, field_id, &field, 0, &mut seen, &mut visited, visit)?;
    }
    Ok(())
}

fn walk_field_node<F>(
    pdf: &ReadPdf,
    field_id: Option<ObjectIdentifier>,
    field: &Dict<'_>,
    depth: usize,
    seen: &mut HashSet<ObjectIdentifier>,
    visited: &mut usize,
    visit: &mut F,
) -> Result<(), String>
where
    F: FnMut(Option<ObjectIdentifier>, &Dict<'_>) -> Result<(), String>,
{
    if depth >= MAX_FIELD_TREE_DEPTH {
        return Err("PDF 양식 필드 트리가 허용 깊이를 초과합니다.".to_string());
    }
    *visited += 1;
    if *visited > MAX_FORM_FIELDS * 4 {
        return Err("PDF 양식 필드 트리 노드 수가 허용 한도를 초과합니다.".to_string());
    }
    if let Some(id) = field_id {
        if !seen.insert(id) {
            return Ok(());
        }
    }
    visit(field_id, field)?;
    if let Some(kids) = field.get::<Array<'_>>(b"Kids") {
        for raw_kid in kids.raw_iter() {
            let (kid_id, Some(kid)) = resolve_raw_dict(pdf, raw_kid) else {
                continue;
            };
            walk_field_node(pdf, kid_id, &kid, depth + 1, seen, visited, visit)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::pdf_edit_budget::MAX_VALUE_DEPTH;
    use crate::pdf_edit_build::build;
    use crate::pdf_edit_fixtures::{deadline, hand_written_with_xref};
    use crate::pdf_edit_incremental::verify_incremental_output;
    use crate::pdf_edit_incremental_forms::inspect_tags_and_sigs;
    use crate::pdf_edit_inspect::open_pdf;
    use crate::pdf_edit_overlay::Overlay;
    use crate::pdf_edit_wire::{OverlayKind, PagePlan, WireForm};
    use hayro_syntax::object::{Array, Dict, Name, Object, ObjectIdentifier};
    use hayro_write::pdf_writer::{Content, Pdf, Rect, Ref};
    use std::sync::Arc;

    fn test_overlay(w: f32, h: f32, _kind: OverlayKind) -> Result<Vec<u8>, String> {
        let mut pdf = Pdf::new();
        let mut next = Ref::new(1);
        let (catalog, tree, page_ref, content_ref) =
            (next.bump(), next.bump(), next.bump(), next.bump());
        pdf.catalog(catalog).pages(tree);
        pdf.pages(tree).count(1).kids([page_ref]);
        let mut content = Content::new();
        content
            .rect(1.0, 1.0, (w - 2.0).max(1.0), (h - 2.0).max(1.0))
            .stroke();
        pdf.stream(content_ref, &content.finish());
        pdf.page(page_ref)
            .media_box(Rect::new(0.0, 0.0, w.max(1.0), h.max(1.0)))
            .parent(tree)
            .contents(content_ref);
        Ok(pdf.finish())
    }

    #[test]
    fn inline_acroform_fields_and_inline_kids_actions_are_sanitized() {
        let run_with_form = |bytes: Vec<u8>, form: WireForm| {
            build(
                &[Arc::new(bytes)],
                &[PagePlan {
                    source: 0,
                    index: 0,
                    turn: 0,
                }],
                &Overlay {
                    form,
                    ..Default::default()
                },
                &mut test_overlay,
                &deadline(),
            )
            .unwrap()
        };

        // (a1) Inline `/AcroForm /Fields` with inline `/AA` trap (not in `/Page /Annots`) + new `applicant`.
        let inline_acro = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R \
                     /AcroForm << /Fields [ << /FT /Tx /T (trap) \
                     /AA << /K << /S /JavaScript /JS (app.alert(1)) >> >> >> ] >> >>"
                        .to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (5, "<< /Type /StructTreeRoot >>".to_string()),
            ],
            "",
        );
        let built_a1 = run_with_form(
            inline_acro,
            WireForm {
                applicant: Some("홍길동".to_string()),
                department: None,
            },
        );
        assert!(built_a1
            .warnings
            .iter()
            .any(|w| w == "문서 스크립트는 보안을 위해 저장하지 않았습니다."));
        assert!(built_a1
            .warnings
            .iter()
            .any(|w| w.contains("안전하지 않은 동작")));
        let r_a1 = open_pdf(&Arc::new(built_a1.bytes)).unwrap();
        let cat_a1 = r_a1.xref().get::<Dict<'_>>(r_a1.xref().root_id()).unwrap();
        assert!(cat_a1.get::<Dict<'_>>(b"StructTreeRoot").is_some());
        let fields_a1 = cat_a1
            .get::<Dict<'_>>(b"AcroForm")
            .unwrap()
            .get::<Array<'_>>(b"Fields")
            .unwrap();
        let f_items_a1: Vec<Dict<'_>> = fields_a1.iter::<Dict<'_>>().collect();
        assert_eq!(f_items_a1.len(), 2);
        assert!(
            f_items_a1[0].get_raw::<Object<'_>>(b"AA").is_none(),
            "inline /Fields trap /AA must be removed"
        );
        assert_eq!(
            f_items_a1[1].get::<Name<'_>>(b"FT").as_deref(),
            Some(b"Tx".as_slice())
        );
        assert_eq!(
            r_a1.pages()
                .first()
                .unwrap()
                .raw()
                .get::<Array<'_>>(b"Annots")
                .unwrap()
                .raw_iter()
                .count(),
            1
        );

        // (a2) Indirect `/AcroForm` + indirect `/Fields` array containing inline `/AA` + new `applicant`.
        let ind_fields_arr = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm 6 0 R >>"
                        .to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (5, "<< /Type /StructTreeRoot >>".to_string()),
                (6, "<< /Fields 7 0 R >>".to_string()),
                (
                    7,
                    "[ << /FT /Tx /T (trap) /AA << /K << /S /JavaScript /JS (1) >> >> >> ]"
                        .to_string(),
                ),
            ],
            "",
        );
        let built_a2 = run_with_form(
            ind_fields_arr,
            WireForm {
                applicant: Some("김철수".to_string()),
                department: None,
            },
        );
        let r_a2 = open_pdf(&Arc::new(built_a2.bytes)).unwrap();
        let fields_a2: Vec<Dict<'_>> = r_a2
            .xref()
            .get::<Dict<'_>>(r_a2.xref().root_id())
            .unwrap()
            .get::<Dict<'_>>(b"AcroForm")
            .unwrap()
            .get::<Array<'_>>(b"Fields")
            .unwrap()
            .iter::<Dict<'_>>()
            .collect();
        assert_eq!(fields_a2.len(), 2);
        assert!(fields_a2[0].get_raw::<Object<'_>>(b"AA").is_none());

        // (b) Indirect parent field with direct inline `/Kids` AND indirect `/Kids` array with unsafe `/A`.
        let inline_kids = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm << /Fields [6 0 R 7 0 R] >> >>"
                        .to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (5, "<< /Type /StructTreeRoot >>".to_string()),
                (
                    6,
                    "<< /FT /Btn /T (grp1) /Kids [ << /Subtype /Widget /A << /S /Launch /F (evil) >> >> ] >>"
                        .to_string(),
                ),
                (7, "<< /FT /Btn /T (grp2) /Kids 8 0 R >>".to_string()),
                (
                    8,
                    "[ << /Subtype /Widget /A << /S /Launch /F (evil2) >> >> ]".to_string(),
                ),
            ],
            "",
        );
        let built_b = run_with_form(inline_kids, WireForm::default());
        assert!(built_b
            .warnings
            .iter()
            .any(|w| w.contains("안전하지 않은 동작")));
        let r_b = open_pdf(&Arc::new(built_b.bytes)).unwrap();
        assert!(r_b
            .xref()
            .get::<Dict<'_>>(r_b.xref().root_id())
            .unwrap()
            .get::<Dict<'_>>(b"StructTreeRoot")
            .is_some());
        let p6 = r_b
            .xref()
            .get::<Dict<'_>>(ObjectIdentifier::new(6, 0))
            .unwrap();
        let k6 = p6
            .get::<Array<'_>>(b"Kids")
            .unwrap()
            .iter::<Dict<'_>>()
            .next()
            .unwrap();
        assert!(k6.get_raw::<Object<'_>>(b"A").is_none());
        let p7 = r_b
            .xref()
            .get::<Dict<'_>>(ObjectIdentifier::new(7, 0))
            .unwrap();
        let k7 = p7
            .get::<Array<'_>>(b"Kids")
            .unwrap()
            .iter::<Dict<'_>>()
            .next()
            .unwrap();
        assert!(k7.get_raw::<Object<'_>>(b"A").is_none());
    }

    #[test]
    fn hostile_next_cycle_depth_outline_cycle_and_partial_sig_loss_fail_closed() {
        let run_err = |bytes: Vec<u8>| -> String {
            let pages = vec![PagePlan {
                source: 0,
                index: 0,
                turn: 0,
            }];
            build(
                &[Arc::new(bytes)],
                &pages,
                &Overlay::default(),
                &mut |_, _, _| Ok(Vec::new()),
                &deadline(),
            )
            .unwrap_err()
        };

        // 1. Cyclic `/Next` action chain.
        let cyc_next = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Annots [6 0 R] >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (5, "<< /Type /StructTreeRoot >>".to_string()),
                (
                    6,
                    "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A 7 0 R >>".to_string(),
                ),
                (
                    7,
                    "<< /S /URI /URI (https://example.org) /Next 7 0 R >>".to_string(),
                ),
            ],
            "",
        );
        assert!(run_err(cyc_next).contains("순환 참조"));

        // 2. `/Next` chain exceeding `MAX_VALUE_DEPTH` (`32`).
        let mut deep_objs = vec![
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Annots [6 0 R] >>"
                    .to_string(),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
            (
                6,
                "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A 7 0 R >>".to_string(),
            ),
        ];
        for depth in 0..=MAX_VALUE_DEPTH {
            let id = 7 + u32::try_from(depth).unwrap();
            let next = if depth == MAX_VALUE_DEPTH {
                String::new()
            } else {
                format!(" /Next {} 0 R", id + 1)
            };
            deep_objs.push((
                id,
                format!("<< /S /URI /URI (https://example.org){next} >>"),
            ));
        }
        assert!(run_err(hand_written_with_xref(&deep_objs, "")).contains("허용 깊이"));

        // 3. Cyclic `/Outlines`.
        let cyc_out = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /Outlines 6 0 R >>"
                        .to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (5, "<< /Type /StructTreeRoot >>".to_string()),
                (
                    6,
                    "<< /Type /Outlines /First 7 0 R /Last 7 0 R /Count 1 >>".to_string(),
                ),
                (7, "<< /Title (L) /Parent 6 0 R /Next 7 0 R >>".to_string()),
            ],
            "",
        );
        assert!(run_err(cyc_out).contains("순환 참조"));

        // 4. `verify_incremental_output` rejects losing 1 of 2 signatures.
        let two_sig = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R] /SigFlags 3 >> >>"
                        .to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Annots [5 0 R 6 0 R] >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (
                    5,
                    "<< /Type /Annot /Subtype /Widget /FT /Sig /T (S1) /Rect [0 0 20 20] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>"
                        .to_string(),
                ),
                (
                    6,
                    "<< /Type /Annot /Subtype /Widget /FT /Sig /T (S2) /Rect [0 30 20 50] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>"
                        .to_string(),
                ),
            ],
            "",
        );
        let one_sig = hand_written_with_xref(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] /SigFlags 3 >> >>"
                        .to_string(),
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
                (
                    3,
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Annots [5 0 R] >>"
                        .to_string(),
                ),
                (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
                (
                    5,
                    "<< /Type /Annot /Subtype /Widget /FT /Sig /T (S1) /Rect [0 0 20 20] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>"
                        .to_string(),
                ),
            ],
            "",
        );
        let expected_info = inspect_tags_and_sigs(&open_pdf(&Arc::new(two_sig)).unwrap()).unwrap();
        assert!(verify_incremental_output(&one_sig, 1, &expected_info).is_err());
    }
}

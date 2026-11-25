//! ISO 32000-1 single-source incremental PDF save preserving `/StructTreeRoot` (tag structure)
//! and `/FT /Sig` digital signature fields (PD-349).
//!
//! Three-step reuse check: (1) no workspace crate implements general incremental updates on top of
//! existing PDF bytes (the PDF page copier always extracts/renumbers all objects into a brand-new
//! PDF, breaking `/ByteRange` digests and dropping `/StructTreeRoot`); (2) existing
//! `pdf_edit_inspect`, `pdf_edit_scan`, `pdf_edit_forms`, `pdf_edit_text_pos`, `pdf_edit_preserve`
//! and `pdf_edit_overlay` provide the security gates, appearance generation and coordinate helpers;
//! (3) this module plus `pdf_edit_incremental_{val,xref,writer,forms,locks,actions,sanitize}`
//! implement ISO 32000-1 §7.5.6 / §7.5.7 / §7.5.8.4 directly using low-level [`Chunk`] and
//! page-extraction helpers for new objects while leaving `0..orig.len()` byte-identical.

#[cfg(test)]
#[path = "pdf_edit_incremental_edge_tests.rs"]
mod edge_tests;
#[cfg(test)]
#[path = "pdf_edit_incremental_tests.rs"]
mod tests;

use crate::pdf_edit_budget::{
    Deadline, MAX_CONTENT_STREAMS, MAX_OBJECTS, MAX_OUTPUT_BYTES, MAX_OVERLAY_SIZES,
    MAX_PAGE_ANNOTS, MAX_PAGE_TREE_DEPTH, MAX_TOTAL_ANNOTS,
};
use crate::pdf_edit_build::{Built, MSG_TOO_BIG};
use crate::pdf_edit_carry::Warnings;
use crate::pdf_edit_forms::PageGeom;
use crate::pdf_edit_incremental_forms::{
    apply_incremental_forms, inspect_tags_and_sigs, load_obj_val, patch_annot_parent_ref,
    read_raw_array, update_catalog_acroform, TagAndSigInfo,
};
use crate::pdf_edit_incremental_sanitize::{
    apply_unsigned_catalog_sanitization, inspect_dangerous, sanitize_page_annots_and_fields,
};
use crate::pdf_edit_incremental_val::{scan_pdf_writer_chunk, IncVal};
use crate::pdf_edit_incremental_writer::IncWriter;
use crate::pdf_edit_incremental_xref::{inspect_xref, XRefInspectResult};
use crate::pdf_edit_inspect::{check_page_limits, open_pdf, open_scanned_within, Limits};
use crate::pdf_edit_overlay::{upright_matrix, Overlay};
use crate::pdf_edit_preserve::{markup_key, read_shifted_rect, MarkupKey, PageDestMap};
use crate::pdf_edit_text_pos::add_page_phrase_markups_in_space;
use crate::pdf_edit_value::Budget;
use crate::pdf_edit_wire::{OverlayKind, OverlayMaker, PagePlan};
use hayro::hayro_interpret::InterpreterCache;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object, ObjectIdentifier, Stream};
use hayro_syntax::page::{Page, Rotation};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::{Chunk, Ref};
use hayro_write::{extract, ChunkSettings, ExtractionQuery};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

const MAX_OVERLAY_KEY_SCALE: f32 = 100.0;
const MSG_SIGNED_LOCKED_EDIT: &str = "전자서명 문서가 수정 잠금(/DocMDP P=1 또는 서명 잠금)으로 보호되어 있어, 편집 내용을 추가하면 뷰어에서 기존 서명 무효가 표시될 수 있습니다.";
const MSG_SIGNED_VISUAL_EDIT: &str = "전자서명이 포함된 PDF에 쪽 회전·주석·오버레이를 증분 저장해, 기존 전자서명의 유효성 표시가 달라질 수 있습니다.";
const MSG_SIGNED_ADDED_FORM_EDIT: &str = "전자서명이 포함된 PDF에 새 양식 필드를 추가해 /AcroForm 구조가 변경되었으므로, 뷰어에서 기존 전자서명의 유효성 표시가 달라질 수 있습니다.";
const MSG_SIGNED_FORM_EDIT: &str =
    "전자서명이 포함된 PDF의 미서명 양식 필드를 증분 갱신했으며 기존 서명 필드는 유지했습니다.";
const MSG_SIGNED_DANGEROUS_KEPT: &str = "원본 전자서명 보존을 위해 문서 내 스크립트·자동 실행·포트폴리오 항목은 제거하지 않고 유지했습니다.";
const MSG_OUTPUT_VERIFY_FAILED: &str = "증분 저장 결과 PDF 검증에 실패해 저장을 중단했습니다.";

/// Result of attempting ISO 32000-1 incremental save before falling back to full PDF rewrite.
pub(crate) enum IncrementalOutcome {
    NotApplicable,
    Built(Built),
    Fallback(String),
}

/// Attempts incremental save when saving a single source PDF with all pages in original order
/// containing `/StructTreeRoot` or `/Sig` fields; otherwise returns `NotApplicable` or `Fallback`.
pub(crate) fn try_incremental_build(
    sources: &[Arc<Vec<u8>>],
    pages: &[PagePlan],
    overlay: &Overlay,
    make_overlay: &mut OverlayMaker<'_>,
    deadline: &Deadline,
) -> Result<IncrementalOutcome, String> {
    if sources.len() != 1
        || pages.is_empty()
        || !pages
            .iter()
            .enumerate()
            .all(|(i, p)| p.source == 0 && p.index == i)
    {
        return Ok(IncrementalOutcome::NotApplicable);
    }
    let orig = &sources[0];
    let pdf = open_scanned_within(orig, deadline, &Limits::DEFAULT)?;
    let orig_pages = pdf.pages();
    let page_count = orig_pages.len();
    if pages.len() != page_count {
        return Ok(IncrementalOutcome::NotApplicable);
    }

    let doc_info = inspect_tags_and_sigs(&pdf)?;
    if !doc_info.has_struct_tree && doc_info.sigs.sig_field_count == 0 && !doc_info.sigs.has_perms {
        return Ok(IncrementalOutcome::NotApplicable);
    }
    check_page_limits(&pdf, deadline, &Limits::DEFAULT)?;

    let meta = match inspect_xref(orig, &pdf)? {
        XRefInspectResult::Ready(meta) => meta,
        XRefInspectResult::Fallback(reason) => {
            return Ok(IncrementalOutcome::Fallback(reason.to_string()));
        }
    };

    let mut page_ids = Vec::with_capacity(page_count);
    for src_page in orig_pages.iter() {
        let Some(id) = src_page.raw().obj_id() else {
            return Ok(IncrementalOutcome::Fallback(
                "원본 쪽 객체 식별자가 없어 새 PDF로 다시 저장했습니다.".to_string(),
            ));
        };
        if id.obj_number == 0 || id.obj_number as usize >= MAX_OBJECTS {
            return Ok(IncrementalOutcome::Fallback(
                "원본 쪽 객체 번호가 허용 범위를 벗어나 새 PDF로 다시 저장했습니다.".to_string(),
            ));
        }
        page_ids.push(id);
    }

    let mut budget = Budget::default();
    let mut writer = IncWriter::new(orig, meta)?;
    let mut warnings = Warnings::default();
    let mut modified_objs: BTreeMap<ObjectIdentifier, IncVal> = BTreeMap::new();
    let mut page_dirty = vec![false; page_count];
    let mut catalog_dirty = false;

    let catalog_id = pdf.xref().root_id();
    let catalog_dict = pdf
        .xref()
        .get::<Dict<'_>>(catalog_id)
        .ok_or("원본 PDF 카탈로그를 읽을 수 없습니다.")?;
    let mut catalog_val = IncVal::read_dict(&catalog_dict, &mut budget)
        .map_err(|r| r.sentence("PDF 문서 카탈로그"))?;

    let dest_map = PageDestMap::build_single(&pdf, deadline)?;
    let dangerous = inspect_dangerous(&pdf, &catalog_dict, &dest_map)?;
    let is_signed = doc_info.sigs.is_signed();

    if !is_signed {
        apply_unsigned_catalog_sanitization(
            &pdf,
            &catalog_dict,
            &mut catalog_val,
            &mut catalog_dirty,
            &dangerous,
            &dest_map,
            &page_ids,
            &mut page_dirty,
            &mut modified_objs,
            &mut budget,
            &mut warnings,
        )?;
    } else if dangerous.any() {
        warnings.push(MSG_SIGNED_DANGEROUS_KEPT.to_string());
    }

    let mut total_annots = 0usize;
    let mut added_markups = 0usize;
    let mut pending_annots: Vec<Vec<ObjectIdentifier>> = vec![Vec::new(); page_count];
    let mut inline_annot_patches: Vec<HashMap<usize, IncVal>> = vec![HashMap::new(); page_count];
    let mut page_geoms = Vec::with_capacity(page_count);
    let mut overlay_forms: HashMap<(u32, u32, bool), ObjectIdentifier> = HashMap::new();
    let interp_cache = overlay.markup.as_ref().map(|_| InterpreterCache::new());
    let mut wrote_visual_edit = false;

    for (idx, (plan, &page_id)) in pages.iter().zip(&page_ids).enumerate() {
        deadline.check()?;
        let src_page = orig_pages.get(idx).ok_or("없는 쪽을 가리킵니다.")?;
        let (width, height) = src_page.base_dimensions();
        if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
            return Err("PDF 쪽 크기가 유효하지 않습니다.".to_string());
        }
        let new_rot = (rotation_degrees(src_page.rotation()) + plan.turn) % 360;
        let crop = src_page.intersected_crop_box();
        let page_ref_i32 = i32::try_from(page_id.obj_number)
            .map_err(|_| "쪽 객체 번호가 범위를 벗어났습니다.".to_string())?;
        page_geoms.push((
            PageGeom {
                width,
                height,
                rotate: new_rot,
                page_ref: Ref::new(page_ref_i32),
            },
            crop,
        ));

        if plan.turn % 360 != 0 {
            let page_val = load_obj_val(
                &pdf,
                page_id,
                src_page.raw(),
                &mut modified_objs,
                &mut budget,
            )?;
            page_val.set(b"Rotate", IncVal::Int(i64::from(new_rot)));
            page_dirty[idx] = true;
            wrote_visual_edit = true;
        }

        let shown = if new_rot % 180 == 90 {
            (height, width)
        } else {
            (width, height)
        };
        let is_first = idx == 0 && overlay.stamp.is_some();
        if overlay.svg(shown.0, shown.1, is_first).is_some() {
            let key = overlay_cache_key(shown, is_first);
            let ov_id = if let Some(&cached) = overlay_forms.get(&key) {
                cached
            } else {
                if overlay_forms.len() >= MAX_OVERLAY_SIZES {
                    return Err(format!(
                        "쪽 크기가 {MAX_OVERLAY_SIZES}가지를 넘어 스탬프와 워터마크를 넣을 수 없습니다."
                    ));
                }
                let kind = if is_first {
                    OverlayKind::FirstPage
                } else {
                    OverlayKind::Watermark
                };
                let extracted_id =
                    append_overlay_xobject(shown, kind, make_overlay, &mut writer, deadline)?;
                overlay_forms.insert(key, extracted_id);
                extracted_id
            };
            apply_page_overlay(
                &pdf,
                src_page,
                page_id,
                new_rot,
                width,
                height,
                crop,
                ov_id,
                &mut modified_objs,
                &mut writer,
                &mut budget,
            )?;
            page_dirty[idx] = true;
            wrote_visual_edit = true;
        }

        let mut seen_markups = collect_existing_annots(src_page, &mut total_annots)?;
        if let (Some(markup), Some(cache)) = (&overlay.markup, &interp_cache) {
            let mut chunk = Chunk::new();
            let mut next = writer.peek_next_ref()?;
            let added = add_page_phrase_markups_in_space(
                &mut chunk,
                &mut next,
                cache,
                pdf.xref(),
                src_page,
                (width, height),
                Ref::new(page_ref_i32),
                markup,
                &mut added_markups,
                &mut total_annots,
                &mut seen_markups,
                false,
            )?;
            if !added.is_empty() {
                let offsets = scan_pdf_writer_chunk(&chunk)?;
                writer.append_chunk(&chunk)?;
                writer.advance_next_to(next)?;
                for r in added {
                    let obj_id = ObjectIdentifier::new(
                        u32::try_from(r.get()).map_err(|_| "주석 번호 오류")?,
                        0,
                    );
                    if page_id.gen_number != 0 {
                        patch_annot_parent_ref(
                            &mut writer,
                            &chunk,
                            &offsets,
                            obj_id,
                            page_id,
                            &mut budget,
                        )?;
                    }
                    pending_annots[idx].push(obj_id);
                }
                page_dirty[idx] = true;
                wrote_visual_edit = true;
            }
        }
    }

    let form_res = apply_incremental_forms(
        &pdf,
        &page_ids,
        &page_geoms,
        &overlay.form,
        &doc_info.sigs,
        make_overlay,
        &mut total_annots,
        &mut pending_annots,
        &mut inline_annot_patches,
        &mut page_dirty,
        &mut modified_objs,
        &mut writer,
        &mut budget,
        deadline,
    )?;

    if !is_signed && dangerous.has_unsafe_annot_action {
        sanitize_page_annots_and_fields(
            &pdf,
            &catalog_dict,
            &mut catalog_val,
            &mut catalog_dirty,
            &dest_map,
            &mut inline_annot_patches,
            &mut page_dirty,
            &mut modified_objs,
            &mut budget,
        )?;
    }

    for (idx, ((&page_id, dirty), added)) in page_ids
        .iter()
        .zip(&mut page_dirty)
        .zip(&pending_annots)
        .enumerate()
    {
        let patches = &inline_annot_patches[idx];
        if !*dirty && added.is_empty() && patches.is_empty() {
            continue;
        }
        let src_page = orig_pages.get(idx).ok_or("없는 쪽을 가리킵니다.")?;
        if !added.is_empty() || !patches.is_empty() {
            let mut annots_vec = read_page_annots_vec(&pdf, src_page.raw(), &mut budget)?;
            for (&annot_idx, patched) in patches {
                if let Some(slot) = annots_vec.get_mut(annot_idx) {
                    *slot = patched.clone();
                }
            }
            for &new_id in added {
                annots_vec.push(IncVal::Ref(new_id));
            }
            if annots_vec.len() > MAX_PAGE_ANNOTS {
                return Err(crate::pdf_edit_preserve::too_many_page_annots());
            }
            let page_val = load_obj_val(
                &pdf,
                page_id,
                src_page.raw(),
                &mut modified_objs,
                &mut budget,
            )?;
            page_val.set(b"Annots", IncVal::Array(annots_vec));
            *dirty = true;
        }
    }

    if !form_res.added_fields.is_empty() || (!is_signed && form_res.clear_need_appearances) {
        update_catalog_acroform(
            &pdf,
            &catalog_dict,
            &mut catalog_val,
            &mut catalog_dirty,
            &form_res.added_fields,
            !is_signed && form_res.clear_need_appearances,
            &mut modified_objs,
            &mut writer,
            &mut budget,
        )?;
    }

    let wrote_form_edit = form_res.wrote_any;
    let added_new_fields = !form_res.added_fields.is_empty();
    if is_signed && (wrote_visual_edit || wrote_form_edit) {
        if doc_info.sigs.doc_mdp_p == Some(1)
            || doc_info.sigs.locked_all
            || form_res.locked_field_modified
            || (doc_info.sigs.doc_mdp_p == Some(2) && added_new_fields)
        {
            warnings.push(MSG_SIGNED_LOCKED_EDIT.to_string());
        } else if wrote_visual_edit {
            warnings.push(MSG_SIGNED_VISUAL_EDIT.to_string());
        } else if added_new_fields {
            warnings.push(MSG_SIGNED_ADDED_FORM_EDIT.to_string());
        } else if wrote_form_edit {
            warnings.push(MSG_SIGNED_FORM_EDIT.to_string());
        }
    }

    for (obj_id, val) in &modified_objs {
        if *obj_id == catalog_id {
            continue;
        }
        writer.write_val(*obj_id, val)?;
    }
    if catalog_dirty {
        writer.write_val(catalog_id, &catalog_val)?;
    }

    let orig_len = writer.orig_len();
    let out_bytes = writer.finish_update()?;
    if out_bytes.len() > MAX_OUTPUT_BYTES {
        return Err(MSG_TOO_BIG.to_string());
    }
    if out_bytes.get(..orig_len) != Some(orig.as_slice()) {
        return Err("증분 저장 결과가 원본 바이트 접두부를 유지하지 못했습니다.".to_string());
    }

    verify_incremental_output(&out_bytes, page_count, &doc_info)?;

    Ok(IncrementalOutcome::Built(Built {
        bytes: out_bytes,
        warnings: warnings.into_vec(),
    }))
}

pub(crate) fn verify_incremental_output(
    out_bytes: &[u8],
    expected_pages: usize,
    doc_info: &TagAndSigInfo,
) -> Result<(), String> {
    let arc = Arc::new(out_bytes.to_vec());
    let reopened = open_pdf(&arc).map_err(|_| MSG_OUTPUT_VERIFY_FAILED.to_string())?;
    if reopened.pages().len() != expected_pages {
        return Err(MSG_OUTPUT_VERIFY_FAILED.to_string());
    }
    match inspect_xref(out_bytes, &reopened)? {
        XRefInspectResult::Ready(_) => {}
        XRefInspectResult::Fallback(_) => return Err(MSG_OUTPUT_VERIFY_FAILED.to_string()),
    }
    let after = inspect_tags_and_sigs(&reopened)?;
    if doc_info.has_struct_tree && !after.has_struct_tree {
        return Err(MSG_OUTPUT_VERIFY_FAILED.to_string());
    }
    if after.sigs.sig_field_count < doc_info.sigs.sig_field_count
        || after.sigs.signed_count < doc_info.sigs.signed_count
        || (doc_info.sigs.has_perms && !after.sigs.has_perms)
    {
        return Err(MSG_OUTPUT_VERIFY_FAILED.to_string());
    }
    Ok(())
}

fn append_overlay_xobject(
    shown: (f32, f32),
    kind: OverlayKind,
    make_overlay: &mut OverlayMaker<'_>,
    writer: &mut IncWriter,
    deadline: &Deadline,
) -> Result<ObjectIdentifier, String> {
    deadline.check()?;
    let bytes = Arc::new(make_overlay(shown.0, shown.1, kind)?);
    let pdf = open_pdf(&bytes)?;
    let mut next = writer.peek_next_ref()?;
    let result = extract(
        &pdf,
        Box::new(|| next.bump()),
        ChunkSettings::default(),
        &[ExtractionQuery::new_xobject(0)],
    )
    .map_err(|_| "오버레이 PDF를 읽지 못했습니다.".to_string())?;
    writer.append_chunk(&result.chunk)?;
    writer.advance_next_to(next)?;
    let root_ref = result
        .root_refs
        .into_iter()
        .next()
        .and_then(Result::ok)
        .ok_or("오버레이 PDF 페이지를 복사하지 못했습니다.")?;
    let id = u32::try_from(root_ref.get()).map_err(|_| "오버레이 객체 번호 오류")?;
    Ok(ObjectIdentifier::new(id, 0))
}

#[allow(clippy::too_many_arguments)]
fn apply_page_overlay(
    pdf: &ReadPdf,
    src_page: &Page<'_>,
    page_id: ObjectIdentifier,
    new_rot: u16,
    width: f32,
    height: f32,
    crop: hayro_syntax::object::Rect,
    ov_id: ObjectIdentifier,
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    writer: &mut IncWriter,
    budget: &mut Budget,
) -> Result<(), String> {
    let mut res_val = resolve_effective_resources(pdf, src_page.raw(), budget)?;
    let mut xobj_map = match res_val.get(b"XObject").cloned() {
        Some(IncVal::Ref(xo_id)) => match pdf.xref().get::<Dict<'_>>(xo_id) {
            Some(xo_dict) => match IncVal::read_dict(&xo_dict, budget) {
                Ok(IncVal::Dict(map)) => map,
                Ok(_) => BTreeMap::new(),
                Err(r) => return Err(r.sentence("PDF 리소스 사전")),
            },
            None => BTreeMap::new(),
        },
        Some(IncVal::Dict(map)) => map,
        _ => BTreeMap::new(),
    };
    let ov_name = allocate_xobject_name(&xobj_map);
    xobj_map.insert(ov_name.as_bytes().to_vec(), IncVal::Ref(ov_id));
    res_val.set(b"XObject", IncVal::Dict(xobj_map));

    let new_res_id = writer.alloc_id()?;
    writer.write_val(new_res_id, &res_val)?;

    let mut m = upright_matrix(new_rot, width, height);
    #[allow(clippy::cast_possible_truncation)]
    {
        m[4] += crop.x0 as f32;
        m[5] += crop.y0 as f32;
    }
    let orig_contents = read_page_contents_refs(pdf, src_page.raw())?;
    let empty_stream_dict = BTreeMap::new();
    let new_contents = if orig_contents.is_empty() {
        let mut op = String::new();
        let _ = write!(
            &mut op,
            "/Artifact BMC\nq {} {} {} {} {} {} cm /{ov_name} Do Q\nEMC\n",
            fmt_coord(m[0]),
            fmt_coord(m[1]),
            fmt_coord(m[2]),
            fmt_coord(m[3]),
            fmt_coord(m[4]),
            fmt_coord(m[5]),
        );
        let post_id = writer.alloc_id()?;
        writer.write_stream(post_id, &empty_stream_dict, op.as_bytes())?;
        vec![IncVal::Ref(post_id)]
    } else {
        let pre_id = writer.alloc_id()?;
        writer.write_stream(pre_id, &empty_stream_dict, b"q\n")?;
        let mut op = String::new();
        let _ = write!(
            &mut op,
            "\nQ\n/Artifact BMC\nq {} {} {} {} {} {} cm /{ov_name} Do Q\nEMC\n",
            fmt_coord(m[0]),
            fmt_coord(m[1]),
            fmt_coord(m[2]),
            fmt_coord(m[3]),
            fmt_coord(m[4]),
            fmt_coord(m[5]),
        );
        let post_id = writer.alloc_id()?;
        writer.write_stream(post_id, &empty_stream_dict, op.as_bytes())?;
        let mut arr = Vec::with_capacity(orig_contents.len() + 2);
        arr.push(IncVal::Ref(pre_id));
        for cid in orig_contents {
            arr.push(IncVal::Ref(cid));
        }
        arr.push(IncVal::Ref(post_id));
        arr
    };

    let page_val = load_obj_val(pdf, page_id, src_page.raw(), modified_objs, budget)?;
    page_val.set(b"Resources", IncVal::Ref(new_res_id));
    page_val.set(b"Contents", IncVal::Array(new_contents));
    Ok(())
}

fn resolve_effective_resources(
    pdf: &ReadPdf,
    page_dict: &Dict<'_>,
    budget: &mut Budget,
) -> Result<IncVal, String> {
    if let Some(res) = page_dict.get::<Dict<'_>>(b"Resources") {
        return IncVal::read_dict(&res, budget).map_err(|r| r.sentence("PDF 쪽 리소스"));
    }
    let mut cur_id = page_dict.get_ref(b"Parent").map(ObjectIdentifier::from);
    let mut seen = HashSet::new();
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        let Some(id) = cur_id else { break };
        if !seen.insert(id) {
            break;
        }
        let Some(d) = pdf.xref().get::<Dict<'_>>(id) else {
            break;
        };
        if let Some(res) = d.get::<Dict<'_>>(b"Resources") {
            return IncVal::read_dict(&res, budget).map_err(|r| r.sentence("PDF 쪽 리소스"));
        }
        cur_id = d.get_ref(b"Parent").map(ObjectIdentifier::from);
    }
    Ok(IncVal::Dict(BTreeMap::new()))
}

fn allocate_xobject_name(existing: &BTreeMap<Vec<u8>, IncVal>) -> String {
    for idx in 0..10_000 {
        let candidate = format!("EoOv{idx}");
        if !existing.contains_key(candidate.as_bytes()) {
            return candidate;
        }
    }
    "EoOvFallback".to_string()
}

fn read_page_contents_refs(
    pdf: &ReadPdf,
    page_dict: &Dict<'_>,
) -> Result<Vec<ObjectIdentifier>, String> {
    let Some(raw) = page_dict.get_raw::<Object<'_>>(b"Contents") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    match raw {
        MaybeRef::NotRef(Object::Null(_)) => {}
        MaybeRef::Ref(r) => {
            let id = ObjectIdentifier::from(r);
            if pdf.xref().get::<Stream<'_>>(id).is_some() {
                out.push(id);
            } else if let Some(arr) = pdf.xref().get::<Array<'_>>(id) {
                collect_stream_array(&arr, &mut out)?;
            }
        }
        MaybeRef::NotRef(Object::Array(arr)) => collect_stream_array(&arr, &mut out)?,
        MaybeRef::NotRef(_) => {}
    }
    Ok(out)
}

fn collect_stream_array(arr: &Array<'_>, out: &mut Vec<ObjectIdentifier>) -> Result<(), String> {
    for item in arr.raw_iter() {
        if let MaybeRef::Ref(r) = item {
            out.push(ObjectIdentifier::from(r));
            if out.len() > MAX_CONTENT_STREAMS {
                return Err(format!(
                    "쪽 하나의 내용 스트림 수가 한도({MAX_CONTENT_STREAMS})를 초과합니다."
                ));
            }
        }
    }
    Ok(())
}

fn collect_existing_annots(
    src_page: &Page<'_>,
    total_annots: &mut usize,
) -> Result<HashSet<MarkupKey>, String> {
    let mut seen = HashSet::new();
    let Some(annots) = src_page.raw().get::<Array<'_>>(b"Annots") else {
        return Ok(seen);
    };
    for (idx, annot) in annots.iter::<Dict<'_>>().enumerate() {
        if idx >= MAX_PAGE_ANNOTS {
            return Err(crate::pdf_edit_preserve::too_many_page_annots());
        }
        *total_annots += 1;
        if *total_annots > MAX_TOTAL_ANNOTS {
            return Err(crate::pdf_edit_preserve::too_many_annots());
        }
        let Some(subtype) = annot.get::<Name<'_>>(b"Subtype") else {
            continue;
        };
        let tag = match subtype.as_str() {
            "Highlight" => 1u8,
            "Underline" => 2u8,
            "StrikeOut" => 3u8,
            "Squiggly" => 4u8,
            _ => continue,
        };
        if let Some(rect) = read_shifted_rect(&annot, 0.0, 0.0) {
            seen.insert(markup_key(tag, rect));
        }
    }
    Ok(seen)
}

fn read_page_annots_vec(
    pdf: &ReadPdf,
    page_dict: &Dict<'_>,
    budget: &mut Budget,
) -> Result<Vec<IncVal>, String> {
    match page_dict.get_raw::<Object<'_>>(b"Annots") {
        None | Some(MaybeRef::NotRef(Object::Null(_))) => Ok(Vec::new()),
        Some(MaybeRef::Ref(r)) => {
            let id = ObjectIdentifier::from(r);
            match pdf.xref().get::<Array<'_>>(id) {
                Some(arr) => read_raw_array(&arr, budget),
                None => Ok(Vec::new()),
            }
        }
        Some(MaybeRef::NotRef(Object::Array(arr))) => read_raw_array(&arr, budget),
        Some(_) => Ok(Vec::new()),
    }
}

fn rotation_degrees(rotation: Rotation) -> u16 {
    match rotation {
        Rotation::None => 0,
        Rotation::Horizontal => 90,
        Rotation::Flipped => 180,
        Rotation::FlippedHorizontal => 270,
    }
}

fn overlay_cache_key(shown: (f32, f32), first: bool) -> (u32, u32, bool) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let q = |v: f32| (v * MAX_OVERLAY_KEY_SCALE).round() as u32;
    (q(shown.0), q(shown.1), first)
}

fn fmt_coord(v: f32) -> String {
    if !v.is_finite() || v == 0.0 {
        return "0".to_string();
    }
    let mut s = format!("{v:.6}");
    if let Some(dot) = s.find('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.len() == dot + 1 {
            s.pop();
        }
    }
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

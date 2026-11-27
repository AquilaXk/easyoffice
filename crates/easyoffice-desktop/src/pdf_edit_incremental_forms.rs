//! Signature/AcroForm inspection and incremental form updates for ISO 32000-1 incremental PDF
//! saves (PD-349).
//!
//! Collects existing `/FT /Sig` fields, `/Perms` certification locks (`/DocMDP /P 1`), `/Lock`
//! restrictions and all existing field names without ever modifying or overwriting `/FT /Sig`
//! fields, while updating matching unsigned `/FT /Tx` fields (`/V` and Noto Sans KR `/AP /N`) or
//! appending new studio `/Tx` widgets on page 1.

use crate::pdf_edit_budget::{Deadline, MAX_FORM_FIELDS, MAX_TOTAL_ANNOTS, MAX_WIDGET_OVERLAYS};
use crate::pdf_edit_forms::{
    allocate_field_name, build_widget_ap, match_field_role, new_widget_rect, read_inherited_field,
    widget_cache_key, write_merged_widget, FormRole, InheritedField, PageGeom, SavedValue,
    WidgetCacheKey, DEFAULT_DA, EO_LABELED_TAG,
};
use crate::pdf_edit_incremental_locks::{
    collect_field_name_parts, collect_lock_rule_from_lock, extract_sig_restrictions,
    is_field_locked, FieldLockRule,
};
use crate::pdf_edit_incremental_val::{encode_pdf_text_bytes, scan_pdf_writer_chunk, IncVal};
use crate::pdf_edit_incremental_writer::IncWriter;
use crate::pdf_edit_preserve::{decode_pdf_text, read_shifted_rect};
use crate::pdf_edit_value::Budget;
use crate::pdf_edit_wire::{OverlayKind, OverlayMaker, WireForm};
use hayro_syntax::object::{Array, Dict, FromBytes, MaybeRef, Name, Object, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::types::{AnnotationFlags, FieldType};
use hayro_write::pdf_writer::{Chunk, Ref};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) const MAX_FIELD_TREE_DEPTH: usize = 32;

pub(crate) struct TagAndSigInfo {
    pub has_struct_tree: bool,
    pub sigs: SigState,
}

pub(crate) struct SigState {
    pub sig_field_count: usize,
    pub signed_count: usize,
    pub has_perms: bool,
    pub doc_mdp_p: Option<i64>,
    pub locked_all: bool,
    pub lock_rules: Vec<FieldLockRule>,
    pub used_names: HashSet<String>,
    pub sig_field_ids: HashSet<ObjectIdentifier>,
}

impl SigState {
    pub fn is_signed(&self) -> bool {
        self.signed_count > 0 || self.has_perms
    }
}

pub(crate) struct FormUpdateResult {
    pub wrote_any: bool,
    pub added_fields: Vec<ObjectIdentifier>,
    pub clear_need_appearances: bool,
    pub locked_field_modified: bool,
}

pub(crate) fn inspect_tags_and_sigs(pdf: &ReadPdf) -> Result<TagAndSigInfo, String> {
    let xref = pdf.xref();
    let catalog = xref
        .get::<Dict<'_>>(xref.root_id())
        .ok_or("원본 PDF 카탈로그를 읽을 수 없습니다.")?;
    let has_struct_tree = catalog.get::<Dict<'_>>(b"StructTreeRoot").is_some();

    let mut sigs = SigState {
        sig_field_count: 0,
        signed_count: 0,
        has_perms: false,
        doc_mdp_p: None,
        locked_all: false,
        lock_rules: Vec::new(),
        used_names: HashSet::new(),
        sig_field_ids: HashSet::new(),
    };

    if let Some(perms) = catalog.get::<Dict<'_>>(b"Perms") {
        if has_non_null(&perms, b"DocMDP")
            || has_non_null(&perms, b"UR3")
            || has_non_null(&perms, b"UR")
        {
            sigs.has_perms = true;
        }
        if let Some(doc_mdp) = perms.get::<Dict<'_>>(b"DocMDP") {
            extract_sig_restrictions(pdf, &doc_mdp, &mut sigs)?;
        }
    }

    let mut seen_fields = HashSet::new();
    let mut visited_field_count = 0usize;
    if let Some(acro) = catalog.get::<Dict<'_>>(b"AcroForm") {
        if let Some(fields) = acro.get::<Array<'_>>(b"Fields") {
            for raw_field in fields.raw_iter() {
                let (field_id, Some(field_dict)) = resolve_raw_dict(pdf, raw_field) else {
                    continue;
                };
                walk_field_tree(
                    pdf,
                    field_id,
                    &field_dict,
                    &[],
                    0,
                    &mut seen_fields,
                    &mut visited_field_count,
                    &mut sigs,
                )?;
            }
        }
    }

    for page in pdf.pages().iter() {
        let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") else {
            continue;
        };
        for raw_annot in annots.raw_iter() {
            let (annot_id, Some(annot)) = resolve_raw_dict(pdf, raw_annot) else {
                continue;
            };
            if annot.get::<Name<'_>>(b"Subtype").as_deref() != Some(b"Widget") {
                continue;
            }
            if let Some(id) = annot_id {
                if seen_fields.contains(&id) {
                    continue;
                }
            }
            inspect_single_widget_sig(pdf, annot_id, &annot, &mut sigs)?;
        }
    }

    Ok(TagAndSigInfo {
        has_struct_tree,
        sigs,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_incremental_forms(
    pdf: &ReadPdf,
    page_ids: &[ObjectIdentifier],
    page_geoms: &[(PageGeom, hayro_syntax::object::Rect)],
    form: &WireForm,
    sigs: &SigState,
    make_overlay: &mut OverlayMaker<'_>,
    total_annots: &mut usize,
    pending_annots: &mut [Vec<ObjectIdentifier>],
    inline_annot_patches: &mut [HashMap<usize, IncVal>],
    page_dirty: &mut [bool],
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    writer: &mut IncWriter,
    budget: &mut Budget,
    deadline: &Deadline,
) -> Result<FormUpdateResult, String> {
    let mut used_names = sigs.used_names.clone();
    let mut widget_cache: HashMap<WidgetCacheKey, Ref> = HashMap::new();
    let mut matched_applicant = false;
    let mut matched_dept = false;
    let mut wrote_custom_ap = false;
    // An untouched text-like widget that has a value but no `/AP /N` is still drawn by the viewer
    // because of `/NeedAppearances`; dropping the flag would blank it.
    let mut viewer_draws_untouched = false;
    let mut locked_field_modified = false;
    let mut added_fields = Vec::new();
    let mut field_count = sigs.sig_field_count;

    if form.applicant.is_none() && form.department.is_none() {
        return Ok(FormUpdateResult {
            wrote_any: false,
            added_fields,
            clear_need_appearances: false,
            locked_field_modified: false,
        });
    }

    let orig_pages = pdf.pages();
    for (idx, ((&page_id, &(geom, _)), patches)) in page_ids
        .iter()
        .zip(page_geoms)
        .zip(inline_annot_patches.iter_mut())
        .enumerate()
    {
        deadline.check()?;
        let Some(src_page) = orig_pages.get(idx) else {
            continue;
        };
        let Some(annots) = src_page.raw().get::<Array<'_>>(b"Annots") else {
            continue;
        };
        for (annot_idx, raw_annot) in annots.raw_iter().enumerate() {
            let (annot_obj_id, Some(annot)) = resolve_raw_dict(pdf, raw_annot) else {
                continue;
            };
            if annot.get::<Name<'_>>(b"Subtype").as_deref() != Some(b"Widget") {
                continue;
            }
            let Some(rect) = read_shifted_rect(&annot, 0.0, 0.0) else {
                continue;
            };
            // `read_inherited_field` returns `Ok(None)` for `/FT /Sig`, never overwriting
            // signature fields even if named `applicant` or `department`.
            let Some(field) = read_inherited_field(&annot)? else {
                continue;
            };
            field_count += 1;
            if field_count > MAX_FORM_FIELDS {
                return Err(format!(
                    "PDF 양식 필드 수가 한도({MAX_FORM_FIELDS})를 초과합니다."
                ));
            }
            if !field.leaf_name.is_empty() {
                used_names.insert(field.leaf_name.clone());
            }
            if !field.full_name.is_empty() {
                used_names.insert(field.full_name.clone());
            }

            let depends_on_viewer = depends_on_need_appearances(&field, &annot);
            if field.field_type != FieldType::Text {
                viewer_draws_untouched |= depends_on_viewer;
                continue;
            }
            if annot_obj_id.is_some_and(|id| sigs.sig_field_ids.contains(&id)) {
                continue;
            }
            let update_spec = match match_field_role(&field) {
                Some(FormRole::Applicant) => {
                    matched_applicant = true;
                    form.applicant.as_deref().map(|val| {
                        let kind = if field.mapping_name == EO_LABELED_TAG {
                            OverlayKind::ApplicantLabeled
                        } else {
                            OverlayKind::ApplicantField
                        };
                        (val, kind)
                    })
                }
                Some(FormRole::Department) => {
                    matched_dept = true;
                    form.department.as_deref().map(|val| {
                        let kind = if field.mapping_name == EO_LABELED_TAG {
                            OverlayKind::DepartmentLabeled
                        } else {
                            OverlayKind::DepartmentField
                        };
                        (val, kind)
                    })
                }
                None => None,
            };

            let Some((val, kind)) = update_spec else {
                viewer_draws_untouched |= depends_on_viewer;
                continue;
            };
            if sigs.locked_all {
                locked_field_modified = true;
            } else if !sigs.lock_rules.is_empty() {
                let parts = collect_field_name_parts(pdf, annot_obj_id, &annot)?;
                if is_field_locked(sigs, &parts, &field) {
                    locked_field_modified = true;
                }
            }

            let ap_ref = build_cached_widget_ap(
                writer,
                rect,
                geom.rotate,
                val,
                kind,
                &mut widget_cache,
                make_overlay,
            )?;
            let ap_id = ObjectIdentifier::new(
                u32::try_from(ap_ref.get()).map_err(|_| "양식 외형 객체 번호 오류")?,
                0,
            );
            wrote_custom_ap = true;

            let encoded_v = IncVal::Str(encode_pdf_text_bytes(val));
            if let Some(annot_id) = annot_obj_id {
                let mut remove_child_v = false;
                if let Some((owner_id, owner_dict)) =
                    find_value_owner(pdf, &annot, annot_id, &sigs.sig_field_ids)?
                {
                    let owner_val =
                        load_obj_val(pdf, owner_id, &owner_dict, modified_objs, budget)?;
                    owner_val.set(b"V", encoded_v);
                    remove_child_v = owner_id != annot_id;
                }
                let annot_val = load_obj_val(pdf, annot_id, &annot, modified_objs, budget)?;
                if remove_child_v {
                    annot_val.remove(b"V");
                }
                let mut ap_dict = BTreeMap::new();
                ap_dict.insert(b"N".to_vec(), IncVal::Ref(ap_id));
                annot_val.set(b"AP", IncVal::Dict(ap_dict));
            } else {
                let mut inline_val =
                    IncVal::read_dict(&annot, budget).map_err(|r| r.sentence("PDF 양식 위젯"))?;
                if annot.get_ref(b"Parent").is_some() {
                    if let Some((owner_id, owner_dict)) =
                        find_parent_owner(pdf, &annot, &sigs.sig_field_ids)?
                    {
                        let owner_val =
                            load_obj_val(pdf, owner_id, &owner_dict, modified_objs, budget)?;
                        owner_val.set(b"V", encoded_v);
                        inline_val.remove(b"V");
                    }
                } else {
                    inline_val.set(b"V", encoded_v);
                }
                let mut ap_dict = BTreeMap::new();
                ap_dict.insert(b"N".to_vec(), IncVal::Ref(ap_id));
                inline_val.set(b"AP", IncVal::Dict(ap_dict));
                patches.insert(annot_idx, inline_val);
                page_dirty[idx] = true;
            }
            let _ = page_id;
        }
    }

    if let Some(&(first_geom, first_crop)) = page_geoms.first() {
        let p0_id = page_ids[0];
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
            field_count += 1;
            if field_count > MAX_FORM_FIELDS {
                return Err(format!(
                    "PDF 양식 필드 수가 한도({MAX_FORM_FIELDS})를 초과합니다."
                ));
            }
            let value = value_opt.unwrap_or_default();
            let mut rect = new_widget_rect(first_geom, slot);
            #[allow(clippy::cast_possible_truncation)]
            {
                rect[0] += first_crop.x0 as f32;
                rect[1] += first_crop.y0 as f32;
                rect[2] += first_crop.x0 as f32;
                rect[3] += first_crop.y0 as f32;
            }
            slot += 1;

            let mut chunk = Chunk::new();
            let mut next = writer.peek_next_ref()?;
            let ap_ref = build_widget_ap(
                &mut chunk,
                &mut next,
                rect,
                first_geom.rotate,
                value,
                kind,
                &mut widget_cache,
                make_overlay,
            )?;
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
                &mut chunk,
                &mut next,
                &spec,
                &unique_name,
                rect,
                AnnotationFlags::PRINT,
                first_geom.page_ref,
                Some(ap_ref),
            );
            let offsets = scan_pdf_writer_chunk(&chunk)?;
            writer.append_chunk(&chunk)?;
            writer.advance_next_to(next)?;
            let widget_id = ObjectIdentifier::new(
                u32::try_from(widget_ref.get()).map_err(|_| "위젯 객체 번호 오류")?,
                0,
            );
            if p0_id.gen_number != 0 {
                patch_annot_parent_ref(writer, &chunk, &offsets, widget_id, p0_id, budget)?;
            }
            *total_annots += 1;
            if *total_annots > MAX_TOTAL_ANNOTS {
                return Err(crate::pdf_edit_preserve::too_many_annots());
            }
            pending_annots[0].push(widget_id);
            added_fields.push(widget_id);
            page_dirty[0] = true;
            wrote_custom_ap = true;
        }
    }

    Ok(FormUpdateResult {
        wrote_any: wrote_custom_ap,
        clear_need_appearances: wrote_custom_ap && !viewer_draws_untouched,
        added_fields,
        locked_field_modified,
    })
}

/// True for a text or choice widget that carries a value but has no normal appearance stream.
fn depends_on_need_appearances(field: &InheritedField, widget: &Dict<'_>) -> bool {
    if !matches!(field.field_type, FieldType::Text | FieldType::Choice) {
        return false;
    }
    let has_value = match &field.value {
        SavedValue::None => false,
        SavedValue::Text(text) => !text.is_empty(),
        SavedValue::Name(name) => !name.is_empty(),
    };
    has_value
        && !widget
            .get::<Dict<'_>>(b"AP")
            .is_some_and(|ap| has_non_null(&ap, b"N"))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_catalog_acroform(
    pdf: &ReadPdf,
    catalog_dict: &Dict<'_>,
    catalog_val: &mut IncVal,
    catalog_dirty: &mut bool,
    added_fields: &[ObjectIdentifier],
    clear_need_appearances: bool,
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    writer: &mut IncWriter,
    budget: &mut Budget,
) -> Result<(), String> {
    if let Some(acro_dict) = catalog_dict.get::<Dict<'_>>(b"AcroForm") {
        if let Some(acro_id) = catalog_dict
            .get_ref(b"AcroForm")
            .map(ObjectIdentifier::from)
        {
            let mut acro_val =
                load_obj_val(pdf, acro_id, &acro_dict, modified_objs, budget)?.clone();
            let changed = mutate_acroform_val(
                pdf,
                &acro_dict,
                &mut acro_val,
                added_fields,
                clear_need_appearances,
                modified_objs,
                writer,
                budget,
            )?;
            if changed {
                modified_objs.insert(acro_id, acro_val);
            }
            return Ok(());
        }
        if let Some(mut acro_val) = catalog_val.get(b"AcroForm").cloned() {
            let changed = mutate_acroform_val(
                pdf,
                &acro_dict,
                &mut acro_val,
                added_fields,
                clear_need_appearances,
                modified_objs,
                writer,
                budget,
            )?;
            if changed {
                catalog_val.set(b"AcroForm", acro_val);
                *catalog_dirty = true;
            }
            return Ok(());
        }
    }

    if added_fields.is_empty() {
        return Ok(());
    }
    let helv_id = alloc_helv_font(writer)?;
    let mut font_map = BTreeMap::new();
    font_map.insert(b"Helv".to_vec(), IncVal::Ref(helv_id));
    let mut dr_map = BTreeMap::new();
    dr_map.insert(b"Font".to_vec(), IncVal::Dict(font_map));
    let mut acro_map = BTreeMap::new();
    acro_map.insert(
        b"Fields".to_vec(),
        IncVal::Array(added_fields.iter().copied().map(IncVal::Ref).collect()),
    );
    acro_map.insert(b"DA".to_vec(), IncVal::Str(DEFAULT_DA.to_vec()));
    acro_map.insert(b"DR".to_vec(), IncVal::Dict(dr_map));
    catalog_val.set(b"AcroForm", IncVal::Dict(acro_map));
    *catalog_dirty = true;
    Ok(())
}

pub(crate) fn load_obj_val<'m>(
    pdf: &ReadPdf,
    id: ObjectIdentifier,
    dict: &Dict<'_>,
    modified_objs: &'m mut BTreeMap<ObjectIdentifier, IncVal>,
    budget: &mut Budget,
) -> Result<&'m mut IncVal, String> {
    if let std::collections::btree_map::Entry::Vacant(e) = modified_objs.entry(id) {
        let base_dict = pdf.xref().get::<Dict<'_>>(id);
        let target = base_dict.as_ref().unwrap_or(dict);
        let val = IncVal::read_dict(target, budget).map_err(|r| r.sentence("PDF 객체 사전"))?;
        e.insert(val);
    }
    Ok(modified_objs.get_mut(&id).expect("inserted"))
}

pub(crate) fn read_raw_array(arr: &Array<'_>, budget: &mut Budget) -> Result<Vec<IncVal>, String> {
    let mut out = Vec::new();
    for item in arr.raw_iter() {
        out.push(IncVal::read_raw(&item, budget).map_err(|r| r.sentence("PDF 배열"))?);
    }
    Ok(out)
}

pub(crate) fn patch_annot_parent_ref(
    writer: &mut IncWriter,
    chunk: &Chunk,
    offsets: &[(u32, u16, usize)],
    annot_id: ObjectIdentifier,
    page_id: ObjectIdentifier,
    budget: &mut Budget,
) -> Result<(), String> {
    let &(_, _, start) = offsets
        .iter()
        .find(|&&(n, _, _)| n == annot_id.obj_number)
        .ok_or_else(|| "생성된 위젯 주석의 /P 참조 대상을 찾지 못했습니다.".to_string())?;
    let slice = &chunk.as_bytes()[start..];
    let obj_pos = slice
        .windows(3)
        .position(|w| w == b"obj")
        .ok_or_else(|| "생성된 위젯 주석의 객체 헤더를 찾지 못했습니다.".to_string())?;
    let after_obj = &slice[obj_pos + 3..];
    let trimmed = after_obj
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map_or(after_obj, |p| &after_obj[p..]);
    let dict = Dict::from_bytes(trimmed)
        .ok_or_else(|| "생성된 위젯 주석의 사전을 읽지 못했습니다.".to_string())?;
    let mut val = IncVal::read_dict(&dict, budget).map_err(|r| r.sentence("PDF 주석 사전"))?;
    val.set(b"P", IncVal::Ref(page_id));
    writer.write_val(annot_id, &val)
}

pub(crate) fn has_non_null(dict: &Dict<'_>, key: &[u8]) -> bool {
    dict.get_raw::<Object<'_>>(key)
        .is_some_and(|raw| !matches!(raw, MaybeRef::NotRef(Object::Null(_))))
}

pub(crate) fn resolve_raw_dict<'a>(
    pdf: &'a ReadPdf,
    raw: MaybeRef<Object<'a>>,
) -> (Option<ObjectIdentifier>, Option<Dict<'a>>) {
    match raw {
        MaybeRef::Ref(r) => {
            let id = ObjectIdentifier::from(r);
            (Some(id), pdf.xref().get::<Dict<'_>>(id))
        }
        MaybeRef::NotRef(Object::Dict(d)) => (None, Some(d)),
        MaybeRef::NotRef(_) => (None, None),
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_field_tree(
    pdf: &ReadPdf,
    field_id: Option<ObjectIdentifier>,
    field: &Dict<'_>,
    prefix: &[String],
    depth: usize,
    seen: &mut HashSet<ObjectIdentifier>,
    visited_count: &mut usize,
    sigs: &mut SigState,
) -> Result<(), String> {
    if depth >= MAX_FIELD_TREE_DEPTH {
        return Err("PDF 양식 필드 트리가 허용 깊이를 초과합니다.".to_string());
    }
    *visited_count += 1;
    if *visited_count > MAX_FORM_FIELDS * 4 {
        return Err("PDF 양식 필드 트리 노드 수가 허용 한도를 초과합니다.".to_string());
    }
    if let Some(id) = field_id {
        if !seen.insert(id) {
            return Ok(());
        }
    }

    let mut parts = prefix.to_vec();
    if let Some(t) = field.get::<hayro_syntax::object::String<'_>>(b"T") {
        let decoded = decode_pdf_text(t.as_bytes(), false, 256);
        if !decoded.is_empty() {
            sigs.used_names.insert(decoded.clone());
            parts.push(decoded);
        }
    }
    if !parts.is_empty() {
        sigs.used_names.insert(parts.join("_"));
    }

    if field_effective_ft(pdf, field).as_deref() == Some("Sig") {
        record_sig_dict(pdf, field_id, field, sigs)?;
    }

    if let Some(kids) = field.get::<Array<'_>>(b"Kids") {
        for raw_kid in kids.raw_iter() {
            let (kid_id, Some(kid_dict)) = resolve_raw_dict(pdf, raw_kid) else {
                continue;
            };
            walk_field_tree(
                pdf,
                kid_id,
                &kid_dict,
                &parts,
                depth + 1,
                seen,
                visited_count,
                sigs,
            )?;
        }
    }
    Ok(())
}

fn inspect_single_widget_sig(
    pdf: &ReadPdf,
    annot_id: Option<ObjectIdentifier>,
    annot: &Dict<'_>,
    sigs: &mut SigState,
) -> Result<(), String> {
    let mut ancestor_parts = Vec::new();
    let mut cur_id = annot.get_ref(b"Parent").map(ObjectIdentifier::from);
    let mut seen = HashSet::new();
    for _ in 0..MAX_FIELD_TREE_DEPTH {
        let Some(id) = cur_id else { break };
        if !seen.insert(id) {
            break;
        }
        let Some(parent) = pdf.xref().get::<Dict<'_>>(id) else {
            break;
        };
        if let Some(t) = parent.get::<hayro_syntax::object::String<'_>>(b"T") {
            let decoded = decode_pdf_text(t.as_bytes(), false, 256);
            if !decoded.is_empty() {
                sigs.used_names.insert(decoded.clone());
                ancestor_parts.push(decoded);
            }
        }
        cur_id = parent.get_ref(b"Parent").map(ObjectIdentifier::from);
    }
    ancestor_parts.reverse();
    if let Some(t) = annot.get::<hayro_syntax::object::String<'_>>(b"T") {
        let decoded = decode_pdf_text(t.as_bytes(), false, 256);
        if !decoded.is_empty() {
            sigs.used_names.insert(decoded.clone());
            ancestor_parts.push(decoded);
        }
    }
    if !ancestor_parts.is_empty() {
        sigs.used_names.insert(ancestor_parts.join("_"));
    }
    if field_effective_ft(pdf, annot).as_deref() == Some("Sig") {
        record_sig_dict(pdf, annot_id, annot, sigs)?;
    }
    Ok(())
}

fn field_effective_ft(pdf: &ReadPdf, dict: &Dict<'_>) -> Option<String> {
    if let Some(ft) = dict.get::<Name<'_>>(b"FT") {
        return Some(ft.as_str().to_string());
    }
    let mut cur_id = dict.get_ref(b"Parent").map(ObjectIdentifier::from);
    let mut seen = HashSet::new();
    for _ in 0..MAX_FIELD_TREE_DEPTH {
        let id = cur_id?;
        if !seen.insert(id) {
            return None;
        }
        let d = pdf.xref().get::<Dict<'_>>(id)?;
        if let Some(ft) = d.get::<Name<'_>>(b"FT") {
            return Some(ft.as_str().to_string());
        }
        cur_id = d.get_ref(b"Parent").map(ObjectIdentifier::from);
    }
    None
}

fn record_sig_dict(
    pdf: &ReadPdf,
    field_id: Option<ObjectIdentifier>,
    dict: &Dict<'_>,
    sigs: &mut SigState,
) -> Result<(), String> {
    sigs.sig_field_count += 1;
    if let Some(id) = field_id {
        sigs.sig_field_ids.insert(id);
    }
    if let Some(lock) = dict.get::<Dict<'_>>(b"Lock") {
        collect_lock_rule_from_lock(pdf, &lock, sigs)?;
    }
    if let Some(sig_v) = dict.get::<Dict<'_>>(b"V") {
        sigs.signed_count += 1;
        extract_sig_restrictions(pdf, &sig_v, sigs)?;
    } else if has_non_null(dict, b"V") {
        sigs.signed_count += 1;
    }
    Ok(())
}

fn build_cached_widget_ap(
    writer: &mut IncWriter,
    rect: [f32; 4],
    rotate: u16,
    val: &str,
    kind: OverlayKind,
    widget_cache: &mut HashMap<WidgetCacheKey, Ref>,
    make_overlay: &mut OverlayMaker<'_>,
) -> Result<Ref, String> {
    let (rw, rh) = ((rect[2] - rect[0]).max(1.0), (rect[3] - rect[1]).max(1.0));
    let (shown_w, shown_h) = if rotate % 180 == 90 {
        (rh, rw)
    } else {
        (rw, rh)
    };
    let key = widget_cache_key(shown_w, shown_h, kind);
    if let Some(&cached) = widget_cache.get(&key) {
        if rotate == 0 || rotate == 360 {
            return Ok(cached);
        }
    }
    if !widget_cache.contains_key(&key) && widget_cache.len() >= MAX_WIDGET_OVERLAYS {
        return Err(format!(
            "양식 필드 모양 수가 한도({MAX_WIDGET_OVERLAYS})를 초과합니다."
        ));
    }
    let mut chunk = Chunk::new();
    let mut next = writer.peek_next_ref()?;
    let ap_ref = build_widget_ap(
        &mut chunk,
        &mut next,
        rect,
        rotate,
        val,
        kind,
        widget_cache,
        make_overlay,
    )?;
    writer.append_chunk(&chunk)?;
    writer.advance_next_to(next)?;
    Ok(ap_ref)
}

fn find_value_owner<'a>(
    pdf: &'a ReadPdf,
    annot: &Dict<'a>,
    annot_id: ObjectIdentifier,
    sig_ids: &HashSet<ObjectIdentifier>,
) -> Result<Option<(ObjectIdentifier, Dict<'a>)>, String> {
    if annot
        .get::<hayro_syntax::object::String<'_>>(b"T")
        .is_some()
        || annot.get::<Name<'_>>(b"FT").is_some()
    {
        return Ok(Some((annot_id, annot.clone())));
    }
    Ok(find_parent_owner(pdf, annot, sig_ids)?.or_else(|| Some((annot_id, annot.clone()))))
}

fn find_parent_owner<'a>(
    pdf: &'a ReadPdf,
    annot: &Dict<'a>,
    sig_ids: &HashSet<ObjectIdentifier>,
) -> Result<Option<(ObjectIdentifier, Dict<'a>)>, String> {
    let mut cur_id = annot.get_ref(b"Parent").map(ObjectIdentifier::from);
    let mut seen = HashSet::new();
    for _ in 0..MAX_FIELD_TREE_DEPTH {
        let Some(id) = cur_id else { break };
        if !seen.insert(id) || sig_ids.contains(&id) {
            return Ok(None);
        }
        let Some(d) = pdf.xref().get::<Dict<'_>>(id) else {
            break;
        };
        if d.get::<Name<'_>>(b"FT").as_deref() == Some(b"Sig") {
            return Ok(None);
        }
        if d.get::<hayro_syntax::object::String<'_>>(b"T").is_some()
            || d.get::<Name<'_>>(b"FT").is_some()
        {
            return Ok(Some((id, d)));
        }
        cur_id = d.get_ref(b"Parent").map(ObjectIdentifier::from);
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn mutate_acroform_val(
    pdf: &ReadPdf,
    acro_dict: &Dict<'_>,
    acro_val: &mut IncVal,
    added_fields: &[ObjectIdentifier],
    clear_need_appearances: bool,
    modified_objs: &BTreeMap<ObjectIdentifier, IncVal>,
    writer: &mut IncWriter,
    budget: &mut Budget,
) -> Result<bool, String> {
    let mut changed = false;
    if !added_fields.is_empty() {
        let mut fields = match acro_val.get(b"Fields") {
            Some(IncVal::Array(items)) => items.clone(),
            Some(IncVal::Ref(arr_id)) => {
                if let Some(IncVal::Array(items)) = modified_objs.get(arr_id) {
                    items.clone()
                } else if let Some(arr) = pdf.xref().get::<Array<'_>>(*arr_id) {
                    read_raw_array(&arr, budget)?
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        for &id in added_fields {
            fields.push(IncVal::Ref(id));
        }
        acro_val.set(b"Fields", IncVal::Array(fields));
        if acro_val.get(b"DA").is_none() {
            acro_val.set(b"DA", IncVal::Str(DEFAULT_DA.to_vec()));
        }
        if !acro_has_helv(acro_dict) {
            let helv_id = alloc_helv_font(writer)?;
            let mut dr = if let Some(dr_dict) = acro_dict.get::<Dict<'_>>(b"DR") {
                IncVal::read_dict(&dr_dict, budget)
                    .map_err(|r| r.sentence("PDF 양식 자원"))?
                    .as_dict()
                    .cloned()
                    .unwrap_or_default()
            } else {
                BTreeMap::new()
            };
            let mut fonts = if let Some(font_dict) = acro_dict
                .get::<Dict<'_>>(b"DR")
                .and_then(|d| d.get::<Dict<'_>>(b"Font"))
            {
                IncVal::read_dict(&font_dict, budget)
                    .map_err(|r| r.sentence("PDF 양식 글꼴"))?
                    .as_dict()
                    .cloned()
                    .unwrap_or_default()
            } else {
                BTreeMap::new()
            };
            fonts
                .entry(b"Helv".to_vec())
                .or_insert(IncVal::Ref(helv_id));
            dr.insert(b"Font".to_vec(), IncVal::Dict(fonts));
            acro_val.set(b"DR", IncVal::Dict(dr));
        }
        changed = true;
    }
    if clear_need_appearances && acro_val.get(b"NeedAppearances").is_some() {
        acro_val.remove(b"NeedAppearances");
        changed = true;
    }
    Ok(changed)
}

fn acro_has_helv(acro_dict: &Dict<'_>) -> bool {
    acro_dict
        .get::<Dict<'_>>(b"DR")
        .and_then(|dr| dr.get::<Dict<'_>>(b"Font"))
        .is_some_and(|fonts| has_non_null(&fonts, b"Helv"))
}

fn alloc_helv_font(writer: &mut IncWriter) -> Result<ObjectIdentifier, String> {
    let mut chunk = Chunk::new();
    let mut next = writer.peek_next_ref()?;
    let font_ref = next.bump();
    chunk
        .type1_font(font_ref)
        .base_font(hayro_write::pdf_writer::Name(b"Helvetica"));
    writer.append_chunk(&chunk)?;
    writer.advance_next_to(next)?;
    Ok(ObjectIdentifier::new(
        u32::try_from(font_ref.get()).map_err(|_| "글꼴 객체 번호 오류")?,
        0,
    ))
}

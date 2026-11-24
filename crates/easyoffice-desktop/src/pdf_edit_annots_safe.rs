//! Security validation and sanitization for carried PDF annotations (PD-337).
//!
//! Ensures `/AP` appearance streams (`pdf_edit_annots_ap.rs`), `/OC` layer references, and
//! `FileAttachment` `/FS` / `/EF` embedded file specifications never smuggle page trees,
//! catalogs, actions, or external paths.

use crate::pdf_edit_annots_ap::is_safe_ap_stream;
use crate::pdf_edit_value::{spend_bytes, spend_node, Budget, Value};
use hayro_syntax::object::{Dict, MaybeRef, Name, ObjRef, Object, Stream};
use hayro_syntax::Pdf as ReadPdf;

/// Longest `/FS` string (`/F`, `/UF`, `/Desc`) carried on a `FileAttachment` annotation.
const MAX_FILESPEC_STR_BYTES: usize = 4_096;
/// Longest `/AFRelationship` and `/VE` operator name bytes.
pub(crate) const MAX_SAFE_NAME_BYTES: usize = 128;
/// Deepest nesting and node count inspected inside `/EF` stream and `/OCG` dictionaries.
pub(crate) const MAX_SUBDICT_DEPTH: usize = 4;
const MAX_SUBDICT_NODES: usize = 64;

pub(crate) fn has_structural_type(dict: &Dict<'_>) -> bool {
    dict.get::<Name<'_>>(b"Type").is_some_and(|ty| {
        matches!(
            ty.as_str(),
            "Page"
                | "Pages"
                | "Catalog"
                | "StructTreeRoot"
                | "Annot"
                | "Action"
                | "Filespec"
                | "EmbeddedFile"
                | "ObjStm"
                | "XRef"
                | "Sig"
        )
    })
}

pub(crate) fn is_structural_dict(dict: &Dict<'_>) -> bool {
    has_structural_type(dict)
        || dict.contains_key(b"Parent")
        || dict.contains_key(b"Kids")
        || dict.contains_key(b"AA")
        || dict.contains_key(b"A")
        || dict.contains_key(b"JS")
        || dict.contains_key(b"JavaScript")
        || dict.contains_key(b"OpenAction")
        || dict.contains_key(b"StructTreeRoot")
        || dict.contains_key(b"Annots")
        || dict.contains_key(b"Pages")
        || dict.contains_key(b"Root")
        || dict.contains_key(b"AcroForm")
}

pub(crate) fn is_forbidden_action_or_file_key(kb: &[u8]) -> bool {
    matches!(
        kb,
        b"F" | b"FS"
            | b"EF"
            | b"URI"
            | b"Launch"
            | b"GoToR"
            | b"GoToE"
            | b"SubmitForm"
            | b"ImportData"
            | b"Names"
            | b"EmbeddedFiles"
            | b"Next"
            | b"FFilter"
            | b"FDecodeParms"
            | b"OPI"
            | b"Ref"
    )
}

pub(crate) fn resolve_indirect_ref<'a>(
    pdf: &'a ReadPdf,
    raw: &MaybeRef<Object<'a>>,
) -> Option<Object<'a>> {
    match raw {
        MaybeRef::NotRef(obj) => Some(obj.clone()),
        MaybeRef::Ref(r) => pdf.xref().get::<Object<'a>>((*r).into()),
    }
}

fn sanitize_ap_states(
    pdf: &ReadPdf,
    subdict: &Dict<'_>,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    if is_structural_dict(subdict) {
        return Ok(None);
    }
    let mut states = Vec::new();
    for (state, val) in subdict.entries() {
        spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
        let MaybeRef::Ref(stream_ref) = val else {
            continue;
        };
        let Some(stream) = pdf.xref().get::<Stream<'_>>(stream_ref.into()) else {
            continue;
        };
        if is_safe_ap_stream(pdf, &stream, stream_ref) {
            let sb = state.as_ref();
            spend_bytes(sb.len(), own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
            states.push((sb.to_vec(), Value::Ref(stream_ref)));
        }
    }
    Ok((!states.is_empty()).then_some(Value::Dict(states)))
}

pub(crate) fn sanitize_ap(
    pdf: &ReadPdf,
    raw: &MaybeRef<Object<'_>>,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    let Some(Object::Dict(ap_dict)) = resolve_indirect_ref(pdf, raw) else {
        return Ok(None);
    };
    if is_structural_dict(&ap_dict) {
        return Ok(None);
    }
    let mut ap_entries = Vec::new();
    for (key, item) in ap_dict.entries() {
        let kb = key.as_ref();
        if !matches!(kb, b"N" | b"R" | b"D") {
            continue;
        }
        spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
        let sanitized = match item {
            MaybeRef::Ref(r) => match pdf.xref().get::<Object<'_>>(r.into()) {
                Some(Object::Stream(s)) if is_safe_ap_stream(pdf, &s, r) => Some(Value::Ref(r)),
                Some(Object::Dict(sub)) => sanitize_ap_states(pdf, &sub, own, own_bytes, budget)?,
                _ => None,
            },
            MaybeRef::NotRef(Object::Dict(ref sub)) => {
                sanitize_ap_states(pdf, sub, own, own_bytes, budget)?
            }
            _ => None,
        };
        if let Some(val) = sanitized {
            spend_bytes(kb.len(), own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
            ap_entries.push((kb.to_vec(), val));
        }
    }
    Ok((!ap_entries.is_empty()).then_some(Value::Dict(ap_entries)))
}

pub(crate) fn has_non_oc_keys(dict: &Dict<'_>) -> bool {
    is_structural_dict(dict)
        || dict.keys().any(|k| {
            let kb = k.as_ref();
            is_forbidden_action_or_file_key(kb)
                || matches!(
                    kb,
                    b"S" | b"Resources" | b"Contents" | b"MediaBox" | b"CropBox"
                )
        })
}

pub(crate) fn is_valid_ocg_dict(pdf: &ReadPdf, dict: &Dict<'_>, self_ref: Option<ObjRef>) -> bool {
    if has_non_oc_keys(dict)
        || dict
            .keys()
            .any(|k| !matches!(k.as_ref(), b"Type" | b"Name" | b"Intent" | b"Usage"))
    {
        return false;
    }
    let has_string_name = dict
        .get_raw::<Object<'_>>(b"Name")
        .is_some_and(|raw| matches!(resolve_indirect_ref(pdf, &raw), Some(Object::String(_))));
    if let Some(raw_ty) = dict.get_raw::<Object<'_>>(b"Type") {
        match resolve_indirect_ref(pdf, &raw_ty) {
            Some(Object::Null(_)) if has_string_name => {}
            Some(Object::Name(n)) if n.as_ref() == b"OCG" => {}
            _ => return false,
        }
    } else if !has_string_name {
        return false;
    }
    let mut walk = OcWalk {
        pdf,
        self_ref,
        nodes: 0,
        active: Vec::new(),
    };
    walk.object_safe(&Object::Dict(dict.clone()), 0)
}

struct OcWalk<'a> {
    pdf: &'a ReadPdf,
    self_ref: Option<ObjRef>,
    nodes: usize,
    active: Vec<ObjRef>,
}

impl OcWalk<'_> {
    fn object_safe(&mut self, object: &Object<'_>, depth: usize) -> bool {
        self.nodes += 1;
        if self.nodes > MAX_SUBDICT_NODES || depth > MAX_SUBDICT_DEPTH {
            return false;
        }
        match object {
            Object::Stream(_) => false,
            Object::Dict(dict) => {
                !has_non_oc_keys(dict)
                    && dict
                        .entries()
                        .all(|(_, item)| self.raw_safe(&item, depth + 1))
            }
            Object::Array(arr) => arr.raw_iter().all(|item| self.raw_safe(&item, depth + 1)),
            _ => true,
        }
    }

    fn raw_safe(&mut self, raw: &MaybeRef<Object<'_>>, depth: usize) -> bool {
        match raw {
            MaybeRef::Ref(r) if self.self_ref == Some(*r) => true,
            MaybeRef::Ref(r) => {
                if self.active.contains(r) {
                    return false;
                }
                let Some(obj) = self.pdf.xref().get::<Object<'_>>((*r).into()) else {
                    return false;
                };
                self.active.push(*r);
                let ok = self.object_safe(&obj, depth);
                self.active.pop();
                ok
            }
            MaybeRef::NotRef(obj) => self.object_safe(obj, depth),
        }
    }
}

fn sanitize_ve_array(
    pdf: &ReadPdf,
    raw: &MaybeRef<Object<'_>>,
    depth: usize,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
    check_ocg_ref: &dyn Fn(ObjRef) -> bool,
) -> Result<Option<Value>, String> {
    if depth > MAX_SUBDICT_DEPTH {
        return Ok(None);
    }
    let Some(Object::Array(arr)) = resolve_indirect_ref(pdf, raw) else {
        return Ok(None);
    };
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    let mut items = Vec::new();
    for item in arr.raw_iter() {
        spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
        match item {
            MaybeRef::Ref(r) if check_ocg_ref(r) => items.push(Value::Ref(r)),
            MaybeRef::NotRef(Object::Name(n)) if n.as_ref().len() <= MAX_SAFE_NAME_BYTES => {
                let nb = n.as_ref();
                spend_bytes(nb.len(), own_bytes, budget)
                    .map_err(|refused| refused.sentence("주석"))?;
                items.push(Value::Name(nb.to_vec()));
            }
            MaybeRef::NotRef(Object::Array(_)) => {
                let Some(sub) = sanitize_ve_array(
                    pdf,
                    &item,
                    depth + 1,
                    own,
                    own_bytes,
                    budget,
                    check_ocg_ref,
                )?
                else {
                    return Ok(None);
                };
                items.push(sub);
            }
            _ => return Ok(None),
        }
    }
    Ok((!items.is_empty()).then_some(Value::Array(items)))
}

fn sanitize_ocmd_dict(
    pdf: &ReadPdf,
    dict: &Dict<'_>,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    if has_non_oc_keys(dict)
        || dict
            .keys()
            .any(|k| !matches!(k.as_ref(), b"Type" | b"OCGs" | b"P" | b"VE"))
    {
        return Ok(None);
    }
    if let Some(raw_ty) = dict.get_raw::<Object<'_>>(b"Type") {
        match resolve_indirect_ref(pdf, &raw_ty) {
            Some(Object::Null(_)) => {}
            Some(Object::Name(n)) if n.as_ref() == b"OCMD" => {}
            _ => return Ok(None),
        }
    }
    let check_ocg_ref = |r: ObjRef| -> bool {
        matches!(
            pdf.xref().get::<Object<'_>>(r.into()),
            Some(Object::Dict(d)) if is_valid_ocg_dict(pdf, &d, Some(r))
        )
    };
    spend_bytes(8, own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
    let mut entries = vec![(b"Type".to_vec(), Value::Name(b"OCMD".to_vec()))];
    let mut has_target = false;
    if let Some(raw_p) = dict.get_raw::<Object<'_>>(b"P") {
        match resolve_indirect_ref(pdf, &raw_p) {
            Some(Object::Null(_)) => {}
            Some(Object::Name(p)) if p.as_ref().len() <= MAX_SAFE_NAME_BYTES => {
                spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
                let pb = p.as_ref();
                spend_bytes(1 + pb.len(), own_bytes, budget)
                    .map_err(|refused| refused.sentence("주석"))?;
                entries.push((b"P".to_vec(), Value::Name(pb.to_vec())));
            }
            _ => return Ok(None),
        }
    }
    if let Some(raw_ocgs) = dict.get_raw::<Object<'_>>(b"OCGs") {
        match raw_ocgs {
            MaybeRef::Ref(r) if check_ocg_ref(r) => {
                spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
                spend_bytes(4, own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
                entries.push((b"OCGs".to_vec(), Value::Ref(r)));
                has_target = true;
            }
            MaybeRef::NotRef(Object::Array(arr)) => {
                let mut refs = Vec::new();
                for item in arr.raw_iter() {
                    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
                    let MaybeRef::Ref(r) = item else {
                        return Ok(None);
                    };
                    if !check_ocg_ref(r) {
                        return Ok(None);
                    }
                    refs.push(Value::Ref(r));
                }
                if !refs.is_empty() {
                    spend_bytes(4, own_bytes, budget)
                        .map_err(|refused| refused.sentence("주석"))?;
                    entries.push((b"OCGs".to_vec(), Value::Array(refs)));
                    has_target = true;
                }
            }
            MaybeRef::NotRef(Object::Null(_)) => {}
            _ => return Ok(None),
        }
    }
    if let Some(raw_ve) = dict.get_raw::<Object<'_>>(b"VE") {
        let Some(ve) = sanitize_ve_array(pdf, &raw_ve, 0, own, own_bytes, budget, &check_ocg_ref)?
        else {
            return Ok(None);
        };
        spend_bytes(2, own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
        entries.push((b"VE".to_vec(), ve));
        has_target = true;
    }
    Ok(has_target.then_some(Value::Dict(entries)))
}

pub(crate) fn sanitize_oc(
    pdf: &ReadPdf,
    raw: &MaybeRef<Object<'_>>,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    match raw {
        MaybeRef::Ref(r) => match pdf.xref().get::<Object<'_>>((*r).into()) {
            Some(Object::Dict(d)) if is_valid_ocg_dict(pdf, &d, Some(*r)) => {
                Ok(Some(Value::Ref(*r)))
            }
            Some(Object::Dict(d)) => sanitize_ocmd_dict(pdf, &d, own, own_bytes, budget),
            _ => Ok(None),
        },
        MaybeRef::NotRef(Object::Dict(d)) => sanitize_ocmd_dict(pdf, d, own, own_bytes, budget),
        _ => Ok(None),
    }
}

pub(crate) fn sanitize_filespec(
    pdf: &ReadPdf,
    annot: &Dict<'_>,
    own: &mut usize,
    own_bytes: &mut usize,
    budget: &mut Budget,
) -> Result<Option<Value>, String> {
    let Some(fs_dict) = annot.get::<Dict<'_>>(b"FS") else {
        return Ok(None);
    };
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    if let Some(raw_ty) = fs_dict.get_raw::<Object<'_>>(b"Type") {
        match resolve_indirect_ref(pdf, &raw_ty) {
            Some(Object::Null(_)) => {}
            Some(Object::Name(n)) if n.as_ref() == b"Filespec" => {}
            _ => return Ok(None),
        }
    }
    if let Some(raw_fs) = fs_dict.get_raw::<Object<'_>>(b"FS") {
        if !matches!(resolve_indirect_ref(pdf, &raw_fs), Some(Object::Null(_))) {
            return Ok(None);
        }
    }
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    spend_bytes(12, own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
    let mut fs_entries = vec![(b"Type".to_vec(), Value::Name(b"Filespec".to_vec()))];
    for key in [b"F".as_slice(), b"UF".as_slice(), b"Desc".as_slice()] {
        let Some(raw) = fs_dict.get_raw::<Object<'_>>(key) else {
            continue;
        };
        let bytes = match resolve_indirect_ref(pdf, &raw) {
            Some(Object::Null(_)) => continue,
            Some(Object::String(s)) => {
                let b = s.as_bytes();
                if b.len() > MAX_FILESPEC_STR_BYTES {
                    return Err(
                        "첨부 파일 주석의 파일 이름 길이가 허용 한도를 초과합니다.".to_string()
                    );
                }
                spend_bytes(key.len() + b.len(), own_bytes, budget)
                    .map_err(|refused| refused.sentence("주석"))?;
                b.to_vec()
            }
            _ => return Ok(None),
        };
        spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
        fs_entries.push((key.to_vec(), Value::Str(bytes)));
    }
    if let Some(raw_rel) = fs_dict.get_raw::<Object<'_>>(b"AFRelationship") {
        match resolve_indirect_ref(pdf, &raw_rel) {
            Some(Object::Null(_)) => {}
            Some(Object::Name(rel)) if rel.as_ref().len() <= MAX_SAFE_NAME_BYTES => {
                spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
                let rb = rel.as_ref();
                spend_bytes(14 + rb.len(), own_bytes, budget)
                    .map_err(|refused| refused.sentence("주석"))?;
                fs_entries.push((b"AFRelationship".to_vec(), Value::Name(rb.to_vec())));
            }
            _ => return Ok(None),
        }
    }
    let Some(ef_dict) = fs_dict.get::<Dict<'_>>(b"EF") else {
        return Ok(None);
    };
    spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
    let mut ef_entries = Vec::new();
    for key in [b"F".as_slice(), b"UF".as_slice()] {
        let Some(raw_target) = ef_dict.get_raw::<Object<'_>>(key) else {
            continue;
        };
        let stream_ref = match raw_target {
            MaybeRef::NotRef(Object::Null(_)) => continue,
            MaybeRef::Ref(r) => match pdf.xref().get::<Object<'_>>(r.into()) {
                Some(Object::Null(_)) => continue,
                Some(Object::Stream(stream)) => {
                    if !embedded_stream_dict_is_safe(pdf, &stream)? {
                        return Ok(None);
                    }
                    r
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        spend_node(own, budget).map_err(|refused| refused.sentence("주석"))?;
        spend_bytes(key.len(), own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
        ef_entries.push((key.to_vec(), Value::Ref(stream_ref)));
    }
    if ef_entries.is_empty() {
        return Ok(None);
    }
    spend_bytes(2, own_bytes, budget).map_err(|refused| refused.sentence("주석"))?;
    fs_entries.push((b"EF".to_vec(), Value::Dict(ef_entries)));
    Ok(Some(Value::Dict(fs_entries)))
}

fn is_allowed_stream_key(key: &[u8]) -> bool {
    matches!(
        key,
        b"Type" | b"Subtype" | b"Filter" | b"DecodeParms" | b"DP" | b"Length" | b"DL" | b"Params"
    )
}

fn is_forbidden_param_key(key: &[u8]) -> bool {
    is_forbidden_action_or_file_key(key)
        || matches!(
            key,
            b"Type"
                | b"Parent"
                | b"Pages"
                | b"Kids"
                | b"Root"
                | b"Catalog"
                | b"AA"
                | b"A"
                | b"JS"
                | b"JavaScript"
                | b"P"
                | b"StructTreeRoot"
                | b"AcroForm"
                | b"Annots"
                | b"S"
        )
}

fn embedded_stream_dict_is_safe(pdf: &ReadPdf, stream: &Stream<'_>) -> Result<bool, String> {
    let mut nodes = 0usize;
    for (key, item) in stream.dict().entries() {
        let kb = key.as_ref();
        if !is_allowed_stream_key(kb) {
            return Ok(false);
        }
        if kb == b"Type" {
            let valid = match resolve_indirect_ref(pdf, &item) {
                Some(Object::Null(_)) => true,
                Some(Object::Name(n)) => n.as_ref() == b"EmbeddedFile",
                _ => false,
            };
            if !valid {
                return Ok(false);
            }
            nodes += 1;
            if nodes > MAX_SUBDICT_NODES {
                return Err("첨부 파일 주석의 스트림 사전 크기가 한도를 초과합니다.".to_string());
            }
            continue;
        }
        if !stream_value_is_safe(pdf, &item, 0, &mut nodes)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn stream_value_is_safe(
    pdf: &ReadPdf,
    raw: &MaybeRef<Object<'_>>,
    depth: usize,
    nodes: &mut usize,
) -> Result<bool, String> {
    *nodes += 1;
    if *nodes > MAX_SUBDICT_NODES {
        return Err("첨부 파일 주석의 스트림 사전 크기가 한도를 초과합니다.".to_string());
    }
    if depth > MAX_SUBDICT_DEPTH {
        return Err("첨부 파일 주석의 스트림 사전 중첩이 허용 깊이를 초과합니다.".to_string());
    }
    let obj = match raw {
        MaybeRef::Ref(r) => {
            if depth > 0 {
                return Ok(false);
            }
            let Some(resolved) = pdf.xref().get::<Object<'_>>((*r).into()) else {
                return Ok(false);
            };
            resolved
        }
        MaybeRef::NotRef(obj) => obj.clone(),
    };
    match obj {
        Object::Stream(_) => Ok(false),
        Object::Array(arr) => {
            for item in arr.raw_iter() {
                if !stream_value_is_safe(pdf, &item, depth + 1, nodes)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Object::Dict(d) => {
            for (k, item) in d.entries() {
                if is_forbidden_param_key(k.as_ref()) {
                    return Ok(false);
                }
                if !stream_value_is_safe(pdf, &item, depth + 1, nodes)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Object::Null(_)
        | Object::Boolean(_)
        | Object::Number(_)
        | Object::String(_)
        | Object::Name(_) => Ok(true),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::pdf_edit_budget::{MAX_ATTACHMENTS, MAX_VALUE_INLINED_BYTES};
    use crate::pdf_edit_carry_tests::rebuilt;
    use crate::pdf_edit_engine::InProcessEngine;
    use crate::pdf_edit_fixtures::{hand_written_with_xref, read};
    use crate::pdf_edit_session::EditSession;
    use hayro_syntax::object::{Array, Dict};
    use hayro_syntax::Pdf as ReadPdf;
    use std::sync::Arc;
    use usvg::fontdb::Database;

    pub(crate) fn page_doc(
        catalog: &str,
        boxes: &str,
        annots: &str,
        extra: Vec<(u32, String)>,
    ) -> Vec<u8> {
        let boxes = if boxes.is_empty() {
            "/MediaBox [0 0 200 300]"
        } else {
            boxes
        };
        let mut objs = vec![
            (1, format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>")),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                format!(
                    "<< /Type /Page /Parent 2 0 R {boxes} \
                     /Contents 4 0 R /Annots [{annots}] >>"
                ),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        ];
        objs.extend(extra);
        hand_written_with_xref(&objs, "")
    }

    pub(crate) fn form_stream(extra_entries: &str) -> String {
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] {extra_entries} /Length 0 >>\nstream\n\nendstream")
    }

    pub(crate) fn empty_stream(entries: &str) -> String {
        format!("<< {entries} /Length 0 >>\nstream\n\nendstream")
    }

    pub(crate) fn first_annot(pdf: &ReadPdf) -> Dict<'_> {
        pdf.pages()[0]
            .raw()
            .get::<Array<'_>>(b"Annots")
            .unwrap()
            .iter::<Dict<'_>>()
            .next()
            .unwrap()
    }

    #[test]
    fn oc_omitted_type_nested_usage_smuggling_and_ve_fallback_are_blocked() {
        let extra = vec![
            (5, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC 10 0 R >>".to_string()),
            (6, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC 11 0 R >>".to_string()),
            (7, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC << /Type /OCMD /OCGs [12 0 R] /VE [/And 3 0 R] >> >>".to_string()),
            (8, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC 13 0 R >>".to_string()),
            (9, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC << /Type /OCMD /OCGs [12 0 R] /VE [/Not 12 0 R] /P /AnyOn >> >>".to_string()),
            (10, "<< /Name (Layer) /S /Launch /F (/bin/sh) >>".to_string()),
            (11, "<< /Type /OCG /Name (Layer) /Usage << /S /URI /URI (https://evil.example) >> >>".to_string()),
            (12, "<< /Type /OCG /Name (Base) >>".to_string()),
            (13, "<< /Type /OCG /Name (UserLayer) /Usage << /User << /Type /Ind /Name (QA) >> /Print << /PrintState /ON >> >> >>".to_string()),
        ];
        let built = rebuilt(page_doc(
            "/OCProperties << /OCGs [12 0 R 13 0 R] /D << /OFF [12 0 R] >> >>",
            "",
            "5 0 R 6 0 R 7 0 R 8 0 R 9 0 R",
            extra,
        ));
        assert!(built.warnings.is_empty(), "{:?}", built.warnings);
        let out = read(&built.bytes);
        let annots: Vec<Dict<'_>> = out.pages()[0]
            .raw()
            .get::<Array<'_>>(b"Annots")
            .unwrap()
            .iter::<Dict<'_>>()
            .collect();
        assert_eq!(annots.len(), 5);
        assert!(!annots[0].contains_key(b"OC"));
        assert!(!annots[1].contains_key(b"OC"));
        assert!(
            !annots[2].contains_key(b"OC"),
            "invalid /VE must drop whole OCMD"
        );
        assert!(annots[3].get_ref(b"OC").is_some());
        assert!(annots[4]
            .get::<Dict<'_>>(b"OC")
            .unwrap()
            .contains_key(b"VE"));
    }

    #[test]
    fn rect_overflow_ffilter_dag_and_combined_attachment_limits_are_enforced() {
        let neg_big = format!("-1{}.0", "0".repeat(38));
        let pos_big = format!("1{}.0", "0".repeat(38));
        let near_max = format!("34{}.0", "0".repeat(37));
        let overflow_built = rebuilt(page_doc(
            "",
            &format!("/MediaBox [{neg_big} 0 0 300]"),
            "5 0 R",
            vec![(
                5,
                format!("<< /Type /Annot /Subtype /Square /Rect [{pos_big} 10 {near_max} 20] >>"),
            )],
        ));
        assert_eq!(
            overflow_built.warnings,
            ["지원하지 않는 주석 1개는 저장하지 못했습니다 (Square 1)."]
        );
        assert!(read(&overflow_built.bytes).pages()[0]
            .raw()
            .get::<Array<'_>>(b"Annots")
            .is_none());

        let ffilter_built = rebuilt(page_doc(
            "",
            "",
            "5 0 R 6 0 R",
            vec![
                (5, "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /EF << /F 7 0 R >> >> >>".to_string()),
                (6, "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /EF << /F 8 0 R >> >> >>".to_string()),
                (7, "<< /Type /EmbeddedFile /FFilter /FlateDecode /Length 2 >>\nstream\nhi\nendstream".to_string()),
                (8, "<< /Type /EmbeddedFile /FDecodeParms << /K -1 >> /Length 2 >>\nstream\nhi\nendstream".to_string()),
            ],
        ));
        assert_eq!(
            ffilter_built.warnings,
            ["지원하지 않는 주석 2개는 저장하지 못했습니다 (FileAttachment 2)."]
        );

        let engine = InProcessEngine::new(Arc::new(Database::new()));
        let combined_doc = |cat_count: usize, page_count: usize| {
            let mut objs = vec![
                (
                    5,
                    "<< /Type /EmbeddedFile /Length 1 >>\nstream\nx\nendstream".to_string(),
                ),
                (
                    6,
                    "<< /Type /Filespec /F (a.bin) /EF << /F 5 0 R >> >>".to_string(),
                ),
            ];
            let names: String = (0..cat_count)
                .map(|i| format!("(c{i:04}.bin) 6 0 R "))
                .collect();
            let mut annots = Vec::with_capacity(page_count);
            for i in 0..page_count {
                let id = 7 + i as u32;
                annots.push(format!("{id} 0 R"));
                objs.push((
                    id,
                    "<< /Type /Annot /Subtype /FileAttachment /Rect [0 0 10 10] /FS 6 0 R >>"
                        .to_string(),
                ));
            }
            page_doc(
                &format!("/Names << /EmbeddedFiles << /Names [{names}] >> >>"),
                "",
                &annots.join(" "),
                objs,
            )
        };
        assert!(
            EditSession::open(combined_doc(600, MAX_ATTACHMENTS - 600), &engine)
                .unwrap()
                .render(None, &engine)
                .is_ok()
        );
        let err = EditSession::open(combined_doc(600, MAX_ATTACHMENTS - 600 + 1), &engine)
            .unwrap()
            .render(None, &engine)
            .unwrap_err();
        assert!(err.contains("첨부 파일"), "{err}");

        let big = format!("({})", "x".repeat(MAX_VALUE_INLINED_BYTES));
        let dag_file = page_doc(
            "",
            "",
            "5 0 R",
            vec![
                (5, "<< /Type /Annot /Subtype /Square /Rect [10 10 30 30] /Contents 6 0 R /T 6 0 R >>".to_string()),
                (6, big),
            ],
        );
        let err = EditSession::open(dag_file, &engine)
            .unwrap()
            .render(None, &engine)
            .unwrap_err();
        assert!(err.contains("크기"), "{err}");
    }

    #[test]
    fn ap_state_byte_budget_oc_nested_usage_and_duplicate_keys_are_guarded() {
        let engine = InProcessEngine::new(Arc::new(Database::new()));
        let half_state_a = "s".repeat(MAX_VALUE_INLINED_BYTES / 2 + 128);
        let half_state_b = "t".repeat(MAX_VALUE_INLINED_BYTES / 2 + 128);
        let big_states = page_doc(
            "",
            "",
            "5 0 R",
            vec![
                (5, format!("<< /Type /Annot /Subtype /Square /Rect [10 10 30 30] /AP << /N << /{half_state_a} 6 0 R /{half_state_b} 6 0 R >> >> >>")),
                (6, form_stream("")),
            ],
        );
        let err = EditSession::open(big_states, &engine)
            .unwrap()
            .render(None, &engine)
            .unwrap_err();
        assert!(err.contains("크기"), "{err}");

        let extra = vec![
            (5, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC 10 0 R >>".to_string()),
            (6, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC << /Type /OCMD /OCGs [11 0 R] /VE [[[[[[11 0 R]]]]]] >> >>".to_string()),
            (7, "<< /Type /Annot /Subtype /Square /Rect [10 10 40 40] /OC << /Type /OCMD /OCGs [11 0 R] /VE 12 0 R >> >>".to_string()),
            (8, "<< /Type /Annot /Subtype /Square /Rect [30 40 70 80] /Rect [900 900 950 950] /AP << /N 13 0 R >> /AP << /N 3 0 R >> >>".to_string()),
            (9, "<< /Type /Annot /Subtype /Square /Rect [30 40 70 80] /AP << /N 3 0 R >> /AP << /N 13 0 R >> >>".to_string()),
            (10, "<< /Name (Layer) /Usage << /CreatorInfo << /Subtype 3 0 R >> >> >>".to_string()),
            (11, "<< /Type /OCG /Name (Base) >>".to_string()),
            (12, "[/Not 12 0 R]".to_string()),
            (13, form_stream("")),
        ];
        let built = rebuilt(page_doc(
            "/OCProperties << /OCGs [11 0 R] /D << /ON [11 0 R] >> >>",
            "/MediaBox [0 0 200 300] /CropBox [20 30 200 300]",
            "5 0 R 6 0 R 7 0 R 8 0 R 9 0 R",
            extra,
        ));
        assert!(built.warnings.is_empty(), "{:?}", built.warnings);
        let out = read(&built.bytes);
        let annots: Vec<Dict<'_>> = out.pages()[0]
            .raw()
            .get::<Array<'_>>(b"Annots")
            .unwrap()
            .iter::<Dict<'_>>()
            .collect();
        assert!(!annots[0].contains_key(b"OC"));
        assert!(!annots[1].contains_key(b"OC"));
        assert!(!annots[2].contains_key(b"OC"));
        let rect_count = annots[3]
            .entries()
            .filter(|(k, _)| k.as_ref() == b"Rect")
            .count();
        assert_eq!(rect_count, 1);
        assert!(!annots[3].contains_key(b"AP"));
        assert!(annots[4].contains_key(b"AP"));
        assert!(out.objects().into_iter().count() < 20);
    }
}

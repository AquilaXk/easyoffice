//! Partial signature field-lock (`/Lock`, `/FieldMDP`) and `/DocMDP` extraction and matching
//! for ISO 32000-1 incremental PDF saves (PD-349).

use crate::pdf_edit_budget::MAX_FORM_FIELDS;
use crate::pdf_edit_forms::InheritedField;
use crate::pdf_edit_incremental_forms::{SigState, MAX_FIELD_TREE_DEPTH};
use crate::pdf_edit_preserve::decode_pdf_text;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldLockAction {
    Include,
    Exclude,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldLockRule {
    pub action: FieldLockAction,
    pub fields: HashSet<String>,
}

impl FieldLockRule {
    pub fn locks_candidates(&self, candidates: &[String]) -> bool {
        let listed = candidates.iter().any(|name| self.fields.contains(name));
        match self.action {
            FieldLockAction::Include => listed,
            FieldLockAction::Exclude => !listed,
        }
    }
}

pub(crate) fn collect_lock_rule_from_lock(
    pdf: &ReadPdf,
    lock: &Dict<'_>,
    sigs: &mut SigState,
) -> Result<(), String> {
    if let Some(p) = lock.get::<i64>(b"P") {
        if p == 1 {
            sigs.locked_all = true;
        }
        if (1..=3).contains(&p) {
            sigs.doc_mdp_p = Some(sigs.doc_mdp_p.map_or(p, |cur| cur.min(p)));
        }
    }
    if let Some(action) = lock.get::<Name<'_>>(b"Action") {
        match action.as_str() {
            "All" => sigs.locked_all = true,
            "Include" => {
                let fields = extract_field_name_list(pdf, lock)?;
                sigs.lock_rules.push(FieldLockRule {
                    action: FieldLockAction::Include,
                    fields,
                });
            }
            "Exclude" => {
                let fields = extract_field_name_list(pdf, lock)?;
                sigs.lock_rules.push(FieldLockRule {
                    action: FieldLockAction::Exclude,
                    fields,
                });
            }
            _ => sigs.locked_all = true,
        }
    }
    Ok(())
}

pub(crate) fn extract_sig_restrictions(
    pdf: &ReadPdf,
    sig_dict: &Dict<'_>,
    sigs: &mut SigState,
) -> Result<(), String> {
    let Some(refs) = sig_dict.get::<Array<'_>>(b"Reference") else {
        return Ok(());
    };
    for (idx, item) in refs.iter::<Dict<'_>>().enumerate() {
        if idx >= MAX_FORM_FIELDS * 4 {
            return Err("PDF 서명 참조 목록이 허용 한도를 초과합니다.".to_string());
        }
        match item.get::<Name<'_>>(b"TransformMethod").as_deref() {
            Some(b"DocMDP") => {
                if let Some(p) = item
                    .get::<Dict<'_>>(b"TransformParams")
                    .and_then(|params| params.get::<i64>(b"P"))
                {
                    sigs.doc_mdp_p = Some(sigs.doc_mdp_p.map_or(p, |cur| cur.min(p)));
                }
            }
            Some(b"FieldMDP") => {
                let Some(params) = item.get::<Dict<'_>>(b"TransformParams") else {
                    continue;
                };
                if let Some(action) = params.get::<Name<'_>>(b"Action") {
                    match action.as_str() {
                        "All" => sigs.locked_all = true,
                        "Include" => {
                            let fields = extract_field_name_list(pdf, &params)?;
                            sigs.lock_rules.push(FieldLockRule {
                                action: FieldLockAction::Include,
                                fields,
                            });
                        }
                        "Exclude" => {
                            let fields = extract_field_name_list(pdf, &params)?;
                            sigs.lock_rules.push(FieldLockRule {
                                action: FieldLockAction::Exclude,
                                fields,
                            });
                        }
                        _ => sigs.locked_all = true,
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn extract_field_name_list(pdf: &ReadPdf, container: &Dict<'_>) -> Result<HashSet<String>, String> {
    let mut fields = HashSet::new();
    let Some(arr) = container.get::<Array<'_>>(b"Fields") else {
        return Ok(fields);
    };
    for (idx, raw) in arr.raw_iter().enumerate() {
        if idx >= MAX_FORM_FIELDS * 4 {
            return Err("PDF 서명 잠금 필드 목록이 허용 한도를 초과합니다.".to_string());
        }
        if let Some(name) = resolve_field_name_entry(pdf, raw) {
            if !name.is_empty() {
                fields.insert(name);
            }
        }
    }
    Ok(fields)
}

fn resolve_field_name_entry(pdf: &ReadPdf, raw: MaybeRef<Object<'_>>) -> Option<String> {
    match raw {
        MaybeRef::NotRef(Object::String(s)) => Some(decode_pdf_text(s.as_bytes(), false, 256)),
        MaybeRef::NotRef(Object::Name(n)) => Some(decode_pdf_text(n.as_ref(), false, 256)),
        MaybeRef::Ref(r) => match pdf.xref().get::<Object<'_>>(ObjectIdentifier::from(r))? {
            Object::String(s) => Some(decode_pdf_text(s.as_bytes(), false, 256)),
            Object::Name(n) => Some(decode_pdf_text(n.as_ref(), false, 256)),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn collect_field_name_parts(
    pdf: &ReadPdf,
    annot_id: Option<ObjectIdentifier>,
    annot: &Dict<'_>,
) -> Result<Vec<String>, String> {
    let mut parts = Vec::with_capacity(4);
    let mut seen = HashSet::new();
    let mut cur = Some((annot_id, annot.clone()));
    let mut depth = 0usize;
    while let Some((cur_id, dict)) = cur {
        if depth >= MAX_FIELD_TREE_DEPTH {
            return Err("PDF 양식 필드 트리가 허용 깊이를 초과합니다.".to_string());
        }
        if cur_id.is_some_and(|id| !seen.insert(id)) {
            return Err("PDF 양식 필드 계층에 순환 참조가 있습니다.".to_string());
        }
        if let Some(t) = dict.get::<hayro_syntax::object::String<'_>>(b"T") {
            let decoded = decode_pdf_text(t.as_bytes(), false, 256);
            if !decoded.is_empty() {
                parts.push(decoded);
            }
        }
        let next_id = dict.get_ref(b"Parent").map(ObjectIdentifier::from);
        if next_id.is_some_and(|id| seen.contains(&id)) {
            return Err("PDF 양식 필드 계층에 순환 참조가 있습니다.".to_string());
        }
        cur = next_id.and_then(|id| pdf.xref().get::<Dict<'_>>(id).map(|d| (Some(id), d)));
        depth += 1;
    }
    parts.reverse();
    Ok(parts)
}

pub(crate) fn is_field_locked(sigs: &SigState, parts: &[String], field: &InheritedField) -> bool {
    if sigs.locked_all {
        return true;
    }
    if sigs.lock_rules.is_empty() {
        return false;
    }
    let mut candidates = Vec::with_capacity(parts.len() * 2 + 2);
    for i in 1..=parts.len() {
        candidates.push(parts[..i].join("."));
        if i > 1 {
            candidates.push(parts[..i].join("_"));
        }
    }
    if !field.leaf_name.is_empty() && !candidates.contains(&field.leaf_name) {
        candidates.push(field.leaf_name.clone());
    }
    if !parts.is_empty() && !field.full_name.is_empty() && !candidates.contains(&field.full_name) {
        candidates.push(field.full_name.clone());
    }
    sigs.lock_rules
        .iter()
        .any(|rule| rule.locks_candidates(&candidates))
}

//! Action (`/A`, `/AA`, recursive `/Next`) validation and `/Catalog /Outlines` inspection and
//! sanitization for ISO 32000-1 incremental PDF saves (PD-349).

use crate::open_link::open_target;
use crate::pdf_edit_budget::{
    MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, MAX_VALUE_DEPTH, MAX_VALUE_NODES,
};
use crate::pdf_edit_incremental_forms::{has_non_null, load_obj_val};
use crate::pdf_edit_incremental_val::IncVal;
use crate::pdf_edit_preserve::PageDestMap;
use crate::pdf_edit_value::{Budget, Refused};
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy)]
pub(crate) enum ActionInspect {
    Safe,
    Unsafe { has_js: bool },
}

impl ActionInspect {
    pub fn is_unsafe(self) -> bool {
        matches!(self, Self::Unsafe { .. })
    }

    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Safe, Self::Safe) => Self::Safe,
            (Self::Unsafe { has_js: a }, Self::Unsafe { has_js: b }) => {
                Self::Unsafe { has_js: a || b }
            }
            (Self::Unsafe { has_js }, Self::Safe) | (Self::Safe, Self::Unsafe { has_js }) => {
                Self::Unsafe { has_js }
            }
        }
    }
}

pub(crate) fn inspect_action_entry(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    dict: &Dict<'_>,
) -> Result<ActionInspect, String> {
    let Some(raw) = dict.get_raw::<Object<'_>>(b"A") else {
        return Ok(ActionInspect::Safe);
    };
    if matches!(raw, MaybeRef::NotRef(Object::Null(_))) {
        return Ok(ActionInspect::Safe);
    }
    let mut active = HashSet::new();
    let mut nodes = 0usize;
    inspect_action_raw(pdf, dest_map, &raw, 1, &mut active, &mut nodes)
}

fn inspect_action_raw(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    raw: &MaybeRef<Object<'_>>,
    depth: usize,
    active: &mut HashSet<ObjectIdentifier>,
    nodes: &mut usize,
) -> Result<ActionInspect, String> {
    if depth > MAX_VALUE_DEPTH {
        return Err(Refused::Deep.sentence("PDF 동작"));
    }
    *nodes += 1;
    if *nodes > MAX_VALUE_NODES {
        return Err(Refused::Nodes.sentence("PDF 동작"));
    }
    match raw {
        MaybeRef::Ref(r) => {
            let id = ObjectIdentifier::from(*r);
            if !active.insert(id) {
                return Err("PDF 동작(/Next)에 순환 참조가 있습니다.".to_string());
            }
            let res = match pdf.xref().get::<Dict<'_>>(id) {
                Some(d) => inspect_action_dict(pdf, dest_map, &d, depth, active, nodes),
                None => Ok(ActionInspect::Unsafe { has_js: false }),
            };
            active.remove(&id);
            res
        }
        MaybeRef::NotRef(Object::Dict(d)) => {
            inspect_action_dict(pdf, dest_map, d, depth, active, nodes)
        }
        MaybeRef::NotRef(_) => Ok(ActionInspect::Unsafe { has_js: false }),
    }
}

fn inspect_action_dict(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    action: &Dict<'_>,
    depth: usize,
    active: &mut HashSet<ObjectIdentifier>,
    nodes: &mut usize,
) -> Result<ActionInspect, String> {
    let has_hidden_js = has_non_null(action, b"JS") || has_non_null(action, b"JavaScript");
    let has_forbidden_extra = has_non_null(action, b"AA")
        || has_non_null(action, b"F")
        || has_non_null(action, b"FS")
        || has_non_null(action, b"EF");
    let self_res = match action.get::<Name<'_>>(b"S").as_deref() {
        Some(b"GoTo")
            if !has_hidden_js
                && !has_forbidden_extra
                && dest_map.resolve_dest_entry(0, pdf, action, b"D").is_some() =>
        {
            ActionInspect::Safe
        }
        Some(b"URI") if !has_hidden_js && !has_forbidden_extra && is_safe_uri_action(action) => {
            ActionInspect::Safe
        }
        Some(b"JavaScript") => ActionInspect::Unsafe { has_js: true },
        _ => ActionInspect::Unsafe {
            has_js: has_hidden_js,
        },
    };
    let next_res = inspect_next_actions(pdf, dest_map, action, depth, active, nodes)?;
    Ok(self_res.combine(next_res))
}

fn inspect_next_actions(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    action: &Dict<'_>,
    depth: usize,
    active: &mut HashSet<ObjectIdentifier>,
    nodes: &mut usize,
) -> Result<ActionInspect, String> {
    let Some(raw_next) = action.get_raw::<Object<'_>>(b"Next") else {
        return Ok(ActionInspect::Safe);
    };
    match raw_next {
        MaybeRef::NotRef(Object::Null(_)) => Ok(ActionInspect::Safe),
        MaybeRef::NotRef(Object::Array(arr)) => {
            inspect_next_array(pdf, dest_map, &arr, depth + 1, active, nodes)
        }
        MaybeRef::Ref(r)
            if pdf
                .xref()
                .get::<Array<'_>>(ObjectIdentifier::from(r))
                .is_some() =>
        {
            let id = ObjectIdentifier::from(r);
            if !active.insert(id) {
                return Err("PDF 동작(/Next)에 순환 참조가 있습니다.".to_string());
            }
            let res = match pdf.xref().get::<Array<'_>>(id) {
                Some(arr) => inspect_next_array(pdf, dest_map, &arr, depth + 1, active, nodes),
                None => Ok(ActionInspect::Unsafe { has_js: false }),
            };
            active.remove(&id);
            res
        }
        other => inspect_action_raw(pdf, dest_map, &other, depth + 1, active, nodes),
    }
}

fn inspect_next_array(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    arr: &Array<'_>,
    depth: usize,
    active: &mut HashSet<ObjectIdentifier>,
    nodes: &mut usize,
) -> Result<ActionInspect, String> {
    if depth > MAX_VALUE_DEPTH {
        return Err(Refused::Deep.sentence("PDF 동작"));
    }
    let mut combined = ActionInspect::Safe;
    for item in arr.raw_iter() {
        let item_res = inspect_action_raw(pdf, dest_map, &item, depth, active, nodes)?;
        combined = combined.combine(item_res);
    }
    Ok(combined)
}

fn is_safe_uri_action(action: &Dict<'_>) -> bool {
    action
        .get::<hayro_syntax::object::String<'_>>(b"URI")
        .and_then(|raw| decode_pdf_uri_strict(raw.as_bytes()))
        .is_some_and(|text| open_target(&text).is_some())
}

fn decode_pdf_uri_strict(bytes: &[u8]) -> Option<String> {
    if bytes.len() > 4_098 {
        return None;
    }
    let decoded = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        decode_utf16_strict(rest, true)?
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        decode_utf16_strict(rest, false)?
    } else {
        let mut s = String::with_capacity(bytes.len());
        for &b in bytes {
            if b < 0x20 || b == 0x7f {
                return None;
            }
            let ch = pdf_doc_char_strict(b);
            if ch.is_control() {
                return None;
            }
            s.push(ch);
        }
        s
    };
    if decoded.chars().count() > 2_048 || decoded.chars().any(char::is_control) {
        return None;
    }
    Some(decoded)
}

fn decode_utf16_strict(bytes: &[u8], big_endian: bool) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes.as_chunks::<2>().0.iter().copied().map(|pair| {
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}

fn pdf_doc_char_strict(byte: u8) -> char {
    const HIGH_CONTROL: [char; 33] = [
        '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{0192}',
        '\u{2044}', '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}', '\u{201E}', '\u{201C}',
        '\u{201D}', '\u{2018}', '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}', '\u{FB02}',
        '\u{0141}', '\u{0152}', '\u{0160}', '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}',
        '\u{0153}', '\u{0161}', '\u{017E}', '\u{FFFD}', '\u{20AC}',
    ];
    match byte {
        0x80..=0xA0 => HIGH_CONTROL[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

pub(crate) fn aa_dict_has_js(dict: &Dict<'_>) -> bool {
    let Some(aa) = dict.get::<Dict<'_>>(b"AA") else {
        return false;
    };
    let found = aa.keys().any(|key| {
        aa.get::<Dict<'_>>(key.as_ref()).is_some_and(|act| {
            act.get::<Name<'_>>(b"S").as_deref() == Some(b"JavaScript") || has_non_null(&act, b"JS")
        })
    });
    found
}

pub(crate) fn inspect_outlines(
    pdf: &ReadPdf,
    catalog: &Dict<'_>,
    dest_map: &PageDestMap,
    has_unsafe: &mut bool,
    has_action_js: &mut bool,
) -> Result<(), String> {
    walk_outlines(catalog, &mut |_, item| {
        if has_non_null(item, b"AA") {
            *has_unsafe = true;
            if aa_dict_has_js(item) {
                *has_action_js = true;
            }
        }
        if let ActionInspect::Unsafe { has_js } = inspect_action_entry(pdf, dest_map, item)? {
            *has_unsafe = true;
            *has_action_js |= has_js;
        }
        Ok(())
    })
}

pub(crate) fn sanitize_outlines(
    pdf: &ReadPdf,
    catalog_dict: &Dict<'_>,
    catalog_val: &mut IncVal,
    catalog_dirty: &mut bool,
    dest_map: &PageDestMap,
    modified_objs: &mut BTreeMap<ObjectIdentifier, IncVal>,
    budget: &mut Budget,
) -> Result<(), String> {
    let outlines_id = catalog_dict
        .get_ref(b"Outlines")
        .map(ObjectIdentifier::from);
    let Some(outlines) = catalog_dict.get::<Dict<'_>>(b"Outlines") else {
        return Ok(());
    };
    let Some(first) = outlines.get::<Dict<'_>>(b"First") else {
        return Ok(());
    };
    if outlines.get_ref(b"First").is_none()
        && inline_outline_needs_sanitize(pdf, dest_map, &first, 0)?
    {
        if let Some(id) = outlines_id {
            let outlines_val = load_obj_val(pdf, id, &outlines, modified_objs, budget)?;
            if let Some(first_val) = outlines_val.get_mut(b"First") {
                sanitize_inline_outline_val(pdf, dest_map, first_val, &first, 0)?;
            }
        } else if let Some(first_val) = catalog_val
            .get_mut(b"Outlines")
            .and_then(|o| o.get_mut(b"First"))
        {
            sanitize_inline_outline_val(pdf, dest_map, first_val, &first, 0)?;
            *catalog_dirty = true;
        }
    }

    walk_outlines(catalog_dict, &mut |item_id, item| {
        if let Some(id) = item_id {
            if inline_outline_needs_sanitize(pdf, dest_map, item, 0)? {
                let item_val = load_obj_val(pdf, id, item, modified_objs, budget)?;
                sanitize_inline_outline_val(pdf, dest_map, item_val, item, 0)?;
            }
        }
        Ok(())
    })
}

fn walk_outlines<F>(catalog: &Dict<'_>, visit: &mut F) -> Result<(), String>
where
    F: FnMut(Option<ObjectIdentifier>, &Dict<'_>) -> Result<(), String>,
{
    let outlines_id = catalog.get_ref(b"Outlines").map(ObjectIdentifier::from);
    let Some(outlines) = catalog.get::<Dict<'_>>(b"Outlines") else {
        return Ok(());
    };
    let first_id = outlines.get_ref(b"First").map(ObjectIdentifier::from);
    let Some(first) = outlines.get::<Dict<'_>>(b"First") else {
        return Ok(());
    };
    let mut visited = HashSet::new();
    if let Some(id) = outlines_id {
        visited.insert(id);
    }
    if first_id.is_some_and(|id| visited.contains(&id)) {
        return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
    }
    let mut total = 0usize;
    walk_outline_siblings(first_id, first, 1, &mut visited, &mut total, visit)
}

fn walk_outline_siblings<F>(
    first_id: Option<ObjectIdentifier>,
    first: Dict<'_>,
    depth: usize,
    visited: &mut HashSet<ObjectIdentifier>,
    total: &mut usize,
    visit: &mut F,
) -> Result<(), String>
where
    F: FnMut(Option<ObjectIdentifier>, &Dict<'_>) -> Result<(), String>,
{
    if depth > MAX_OUTLINE_DEPTH {
        return Err(format!(
            "PDF 북마크(목차) 트리가 허용 깊이({MAX_OUTLINE_DEPTH}단계)를 초과합니다."
        ));
    }
    let mut cur = Some((first_id, first));
    while let Some((item_id, item)) = cur {
        if item_id.is_some_and(|id| !visited.insert(id)) {
            return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
        }
        *total += 1;
        if *total > MAX_OUTLINE_ITEMS {
            return Err(format!(
                "PDF 북마크(목차) 항목 수가 한도({MAX_OUTLINE_ITEMS})를 초과합니다."
            ));
        }
        visit(item_id, &item)?;
        let child_id = item.get_ref(b"First").map(ObjectIdentifier::from);
        if child_id.is_some_and(|id| visited.contains(&id)) {
            return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
        }
        if let Some(child_first) = item.get::<Dict<'_>>(b"First") {
            walk_outline_siblings(child_id, child_first, depth + 1, visited, total, visit)?;
        }
        let next_id = item.get_ref(b"Next").map(ObjectIdentifier::from);
        if next_id.is_some_and(|id| visited.contains(&id)) {
            return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
        }
        cur = item.get::<Dict<'_>>(b"Next").map(|d| (next_id, d));
    }
    Ok(())
}

fn inline_outline_needs_sanitize(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    dict: &Dict<'_>,
    inline_depth: usize,
) -> Result<bool, String> {
    if inline_depth >= MAX_VALUE_DEPTH {
        return Err(Refused::Deep.sentence("PDF 북마크 사전"));
    }
    if has_non_null(dict, b"AA") || inspect_action_entry(pdf, dest_map, dict)?.is_unsafe() {
        return Ok(true);
    }
    for key in [b"First".as_slice(), b"Next".as_slice()] {
        if dict.get_ref(key).is_none() {
            if let Some(sub) = dict.get::<Dict<'_>>(key) {
                if inline_outline_needs_sanitize(pdf, dest_map, &sub, inline_depth + 1)? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn sanitize_inline_outline_val(
    pdf: &ReadPdf,
    dest_map: &PageDestMap,
    val: &mut IncVal,
    dict: &Dict<'_>,
    inline_depth: usize,
) -> Result<(), String> {
    if inline_depth >= MAX_VALUE_DEPTH {
        return Err(Refused::Deep.sentence("PDF 북마크 사전"));
    }
    if has_non_null(dict, b"AA") {
        val.remove(b"AA");
    }
    if inspect_action_entry(pdf, dest_map, dict)?.is_unsafe() {
        val.remove(b"A");
    }
    for key in [b"First".as_slice(), b"Next".as_slice()] {
        if dict.get_ref(key).is_none() {
            if let (Some(sub_dict), Some(sub_val)) = (dict.get::<Dict<'_>>(key), val.get_mut(key)) {
                sanitize_inline_outline_val(pdf, dest_map, sub_val, &sub_dict, inline_depth + 1)?;
            }
        }
    }
    Ok(())
}

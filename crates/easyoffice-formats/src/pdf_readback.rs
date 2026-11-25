//! Structured PDF readback and object-graph inspection (PD-248).
//!
//! Combines [`PdfSerializer::parse`] text/stamp/watermark extraction with a cycle-safe,
//! budget-bounded walk of the indirect object graph for `/MediaBox` page dimensions,
//! `/Annots` (`/Link`, markup, `/Widget`), `/Catalog /Outlines`, and `/AcroForm /Fields`.

use std::collections::{HashMap, HashSet};

use crate::decode_budget::DecodeBudget;
use crate::EasyOfficeError;

use super::pdf_object_graph::{
    decode_and_validate_pdf_streams, decode_text_value, dict_get, dict_has_non_null_key,
    dict_name_eq, expand_pdf_object_streams, resolve_catalog_and_pages_target, resolve_num,
    resolve_value, PageNodeTarget, PdfIndirectObject, PdfObjValue, MAX_PDF_PAGE_TREE_DEPTH,
    MAX_PDF_TOTAL_DECODED_BYTES,
};
use super::{ordered_by_sequence, PdfSerializer, PdfTopLevelScan, MAX_PDF_PAGE_TREE_NODES};

const DEFAULT_A4_WIDTH_PT: f64 = 595.28;
const DEFAULT_A4_HEIGHT_PT: f64 = 841.89;
const MAX_OUTLINE_NODES: usize = 4_096;
const MAX_OUTLINE_TITLES: usize = 256;
const MAX_FORM_NODES: usize = 4_096;
const MAX_ANNOTS_PER_PAGE: usize = 4_096;
const MAX_FORM_VALUE_CHARS: usize = 512;
const MAX_EXTRACTED_FORM_FIELDS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfFormField {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PdfFormReadback {
    pub has_acro_form: bool,
    pub field_count: usize,
    pub applicant: Option<String>,
    pub department: Option<String>,
    pub fields: Vec<PdfFormField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfPageReadback {
    pub page_number: usize,
    pub rotation_degrees: i32,
    pub width_pt: f64,
    pub height_pt: f64,
    pub body_text: String,
    pub approval_stamp: Option<String>,
    pub link_count: usize,
    pub markup_count: usize,
    pub widget_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfOutlineNode {
    pub title: String,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfReadbackReport {
    pub title: String,
    pub watermark_text: Option<String>,
    pub pages: Vec<PdfPageReadback>,
    pub outline_count: usize,
    pub outline_titles: Vec<String>,
    pub outline_nodes: Vec<PdfOutlineNode>,
    pub link_annotation_count: usize,
    pub markup_annotation_count: usize,
    pub form_field_count: usize,
    pub has_acro_form: bool,
    pub has_outlines: bool,
}

#[derive(Clone, Copy)]
struct PageMeta {
    width_pt: f64,
    height_pt: f64,
    link_count: usize,
    markup_count: usize,
    widget_count: usize,
}

impl Default for PageMeta {
    fn default() -> Self {
        Self {
            width_pt: DEFAULT_A4_WIDTH_PT,
            height_pt: DEFAULT_A4_HEIGHT_PT,
            link_count: 0,
            markup_count: 0,
            widget_count: 0,
        }
    }
}

fn resolve_catalog_dict<'a>(
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    ordered_objs: &[((u32, u16), &'a PdfIndirectObject)],
    trailers: &'a [Vec<(String, PdfObjValue)>],
) -> Option<&'a [(String, PdfObjValue)]> {
    let root_val = trailers
        .iter()
        .rev()
        .find_map(|tr| dict_get(tr, "/Root"))
        .or_else(|| {
            ordered_objs.iter().rev().find_map(|(_, o)| {
                if let PdfObjValue::Dict(ref d) = o.val {
                    if dict_name_eq(d, "/Type", "/XRef") {
                        return dict_get(d, "/Root");
                    }
                }
                None
            })
        });
    root_val
        .and_then(|rv| match resolve_value(rv, objects) {
            Some(PdfObjValue::Dict(d)) => Some(d.as_slice()),
            _ => None,
        })
        .or_else(|| {
            ordered_objs.iter().rev().find_map(|(_, o)| {
                if let PdfObjValue::Dict(ref d) = o.val {
                    if dict_name_eq(d, "/Type", "/Catalog") {
                        return Some(d.as_slice());
                    }
                }
                None
            })
        })
}

fn parse_box_dims(
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<(f64, f64)> {
    let raw = dict_get(entries, "/CropBox").or_else(|| dict_get(entries, "/MediaBox"))?;
    let PdfObjValue::Array(items) = resolve_value(raw, objects)? else {
        return None;
    };
    if items.len() < 4 {
        return None;
    }
    let x0 = resolve_num(&items[0], objects)?;
    let y0 = resolve_num(&items[1], objects)?;
    let x1 = resolve_num(&items[2], objects)?;
    let y1 = resolve_num(&items[3], objects)?;
    let w = (x1 - x0).abs();
    let h = (y1 - y0).abs();
    (w > 0.0 && h > 0.0).then_some((w, h))
}

fn count_page_annots(
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> (usize, usize, usize) {
    let Some(raw) = dict_get(entries, "/Annots") else {
        return (0, 0, 0);
    };
    let Some(PdfObjValue::Array(items)) = resolve_value(raw, objects) else {
        return (0, 0, 0);
    };
    let (mut links, mut markups, mut widgets) = (0usize, 0usize, 0usize);
    for item in items.iter().take(MAX_ANNOTS_PER_PAGE) {
        let Some(PdfObjValue::Dict(d)) = resolve_value(item, objects) else {
            continue;
        };
        let Some(PdfObjValue::Name(sub)) = dict_get(d, "/Subtype") else {
            continue;
        };
        match sub.as_str() {
            "/Link" => links += 1,
            "/Widget" => widgets += 1,
            "/Highlight" | "/Underline" | "/Squiggly" | "/StrikeOut" | "/Text" | "/FreeText"
            | "/Stamp" => markups += 1,
            _ => {}
        }
    }
    (links, markups, widgets)
}

struct MetaWalk<'a> {
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    visited: HashSet<(u32, u16)>,
    nodes: usize,
    pages: Vec<PageMeta>,
}

fn walk_page_meta<'a>(
    state: &mut MetaWalk<'a>,
    target: PageNodeTarget<'a>,
    inherited_dims: (f64, f64),
    depth: usize,
) {
    if depth > MAX_PDF_PAGE_TREE_DEPTH || state.nodes >= MAX_PDF_PAGE_TREE_NODES {
        return;
    }
    state.nodes += 1;
    let entries = match target {
        PageNodeTarget::Ref(on, gn) => {
            if !state.visited.insert((on, gn)) {
                return;
            }
            let Some(PdfObjValue::Dict(ref d)) = state.objects.get(&(on, gn)).map(|o| &o.val)
            else {
                return;
            };
            d.as_slice()
        }
        PageNodeTarget::Inline(d) => d,
    };
    let dims = parse_box_dims(entries, state.objects).unwrap_or(inherited_dims);
    let is_pages = dict_name_eq(entries, "/Type", "/Pages")
        || (dict_get(entries, "/Type").is_none() && dict_get(entries, "/Kids").is_some());
    if is_pages {
        let Some(PdfObjValue::Array(kids)) =
            dict_get(entries, "/Kids").and_then(|k| resolve_value(k, state.objects))
        else {
            return;
        };
        for kid in kids {
            match kid {
                PdfObjValue::Ref(ko, kg) => {
                    walk_page_meta(state, PageNodeTarget::Ref(*ko, *kg), dims, depth + 1);
                }
                PdfObjValue::Dict(kd) => {
                    walk_page_meta(
                        state,
                        PageNodeTarget::Inline(kd.as_slice()),
                        dims,
                        depth + 1,
                    );
                }
                _ => {}
            }
        }
        return;
    }
    let (link_count, markup_count, widget_count) = count_page_annots(entries, state.objects);
    state.pages.push(PageMeta {
        width_pt: dims.0,
        height_pt: dims.1,
        link_count,
        markup_count,
        widget_count,
    });
}

fn walk_outline_chain<'a>(
    mut cur: Option<&'a PdfObjValue>,
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    visited: &mut HashSet<(u32, u16)>,
    count: &mut usize,
    titles: &mut Vec<String>,
    nodes: &mut Vec<PdfOutlineNode>,
    depth: usize,
) {
    if depth > MAX_PDF_PAGE_TREE_DEPTH {
        return;
    }
    while let Some(val) = cur {
        if *count >= MAX_OUTLINE_NODES {
            break;
        }
        let entries = match val {
            PdfObjValue::Ref(on, gn) => {
                if !visited.insert((*on, *gn)) {
                    break;
                }
                let Some(PdfObjValue::Dict(ref d)) = objects.get(&(*on, *gn)).map(|o| &o.val)
                else {
                    break;
                };
                d.as_slice()
            }
            PdfObjValue::Dict(d) => d.as_slice(),
            _ => break,
        };
        *count += 1;
        if titles.len() < MAX_OUTLINE_TITLES {
            if let Some(t) = dict_get(entries, "/Title").and_then(|v| decode_text_value(v, objects))
            {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    let level = u8::try_from(depth.saturating_add(1)).unwrap_or(u8::MAX);
                    titles.push(trimmed.to_string());
                    nodes.push(PdfOutlineNode {
                        title: trimmed.to_string(),
                        level,
                    });
                }
            }
        }
        if let Some(first_child) = dict_get(entries, "/First") {
            walk_outline_chain(
                Some(first_child),
                objects,
                visited,
                count,
                titles,
                nodes,
                depth + 1,
            );
        }
        cur = dict_get(entries, "/Next");
    }
}

fn collect_pdf_outlines(
    catalog: Option<&[(String, PdfObjValue)]>,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> (bool, usize, Vec<String>, Vec<PdfOutlineNode>) {
    let mut outline_count = 0usize;
    let mut outline_titles = Vec::new();
    let mut outline_nodes = Vec::new();
    let mut has_outlines = false;
    if let Some(outlines_dict) = catalog
        .and_then(|c| dict_get(c, "/Outlines"))
        .and_then(|v| match resolve_value(v, objects) {
            Some(PdfObjValue::Dict(d)) => Some(d.as_slice()),
            _ => None,
        })
    {
        has_outlines = true;
        let mut visited = HashSet::new();
        walk_outline_chain(
            dict_get(outlines_dict, "/First"),
            objects,
            &mut visited,
            &mut outline_count,
            &mut outline_titles,
            &mut outline_nodes,
            0,
        );
    }
    (has_outlines, outline_count, outline_titles, outline_nodes)
}

type PdfObjDict = Vec<(String, PdfObjValue)>;
type DecodedPdfObjects = (HashMap<(u32, u16), PdfIndirectObject>, Vec<PdfObjDict>);

fn decode_pdf_objects(bytes: &[u8]) -> Result<DecodedPdfObjects, EasyOfficeError> {
    let mut scan = PdfTopLevelScan::new(bytes);
    scan.run()?;
    let PdfTopLevelScan {
        mut objects,
        trailers,
        mut order_seq,
        ..
    } = scan;
    let mut remaining_budget = MAX_PDF_TOTAL_DECODED_BYTES;
    let mut dummy_b64 = None;
    let mut dummy_cnt = 0usize;
    let flate_cache = {
        let ordered_objs = ordered_by_sequence(&objects);
        decode_and_validate_pdf_streams(
            bytes,
            &objects,
            &ordered_objs,
            &mut remaining_budget,
            &mut dummy_b64,
            &mut dummy_cnt,
        )?
    };
    let mut decoded = DecodeBudget::new();
    expand_pdf_object_streams(
        bytes,
        &mut objects,
        &flate_cache,
        &mut order_seq,
        &mut decoded,
    )?;
    Ok((objects, trailers))
}

fn decode_pdf_literal_bytes(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] != b'\\' {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= raw.len() {
            break;
        }
        match raw[i] {
            b'n' => {
                out.push(b'\n');
                i += 1;
            }
            b'r' => {
                out.push(b'\r');
                i += 1;
            }
            b't' => {
                out.push(b'\t');
                i += 1;
            }
            b'b' => {
                out.push(0x08);
                i += 1;
            }
            b'f' => {
                out.push(0x0C);
                i += 1;
            }
            b'\\' | b'(' | b')' => {
                out.push(raw[i]);
                i += 1;
            }
            b'\n' => i += 1,
            b'\r' => {
                i += 1;
                if i < raw.len() && raw[i] == b'\n' {
                    i += 1;
                }
            }
            b'0'..=b'7' => {
                let mut val = 0u16;
                for _ in 0..3 {
                    if i < raw.len() && (b'0'..=b'7').contains(&raw[i]) {
                        val = (val << 3) | u16::from(raw[i] - b'0');
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(val as u8);
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

fn decode_form_text(
    val: &PdfObjValue,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<String> {
    let text = match resolve_value(val, objects)? {
        PdfObjValue::ParenStr(raw) => {
            let bytes = decode_pdf_literal_bytes(raw);
            if bytes.starts_with(&[0xFE, 0xFF]) {
                let u16s: Vec<u16> = bytes[2..]
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&u16s)
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            }
        }
        PdfObjValue::HexStr(_) => decode_text_value(val, objects)?,
        _ => return None,
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_FORM_VALUE_CHARS).collect())
}

fn decode_form_value(
    val: &PdfObjValue,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<String> {
    if let Some(text) = decode_form_text(val, objects) {
        return Some(text);
    }
    if let Some(PdfObjValue::Name(raw)) = resolve_value(val, objects) {
        let trimmed = raw.strip_prefix('/').unwrap_or(raw).trim();
        if !trimmed.is_empty() {
            return Some(trimmed.chars().take(MAX_FORM_VALUE_CHARS).collect());
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormRole {
    Applicant,
    Department,
}

fn match_form_role(name: &str) -> Option<FormRole> {
    let lower = name.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
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
    None
}

struct FormWalkState<'a> {
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    visited: HashSet<(u32, u16)>,
    nodes: usize,
    out: PdfFormReadback,
}

#[derive(Clone, Copy, Default)]
struct FormWalkParent<'a> {
    name: Option<&'a str>,
    alt: Option<&'a str>,
    value: Option<&'a str>,
}

fn walk_form_fields_detailed(
    items: &[PdfObjValue],
    state: &mut FormWalkState<'_>,
    parent: FormWalkParent<'_>,
    depth: usize,
) {
    if depth > MAX_PDF_PAGE_TREE_DEPTH {
        return;
    }
    let objects = state.objects;
    for item in items {
        if state.nodes >= MAX_FORM_NODES {
            return;
        }
        let entries = match item {
            PdfObjValue::Ref(on, gn) => {
                if !state.visited.insert((*on, *gn)) {
                    continue;
                }
                let Some(PdfObjValue::Dict(ref d)) = objects.get(&(*on, *gn)).map(|o| &o.val)
                else {
                    continue;
                };
                d.as_slice()
            }
            PdfObjValue::Dict(d) => d.as_slice(),
            _ => continue,
        };
        state.nodes += 1;
        let own_name = dict_get(entries, "/T").and_then(|v| decode_form_text(v, objects));
        let full_name = match (parent.name, own_name.as_deref()) {
            (Some(p), Some(n)) => Some(format!("{p}.{n}")),
            (Some(p), None) => Some(p.to_string()),
            (None, Some(n)) => Some(n.to_string()),
            (None, None) => None,
        };
        let own_alt = dict_get(entries, "/TU")
            .and_then(|v| decode_form_text(v, objects))
            .or_else(|| parent.alt.map(str::to_string));
        let own_val = dict_get(entries, "/V")
            .and_then(|v| decode_form_value(v, objects))
            .or_else(|| parent.value.map(str::to_string));

        let kids = dict_get(entries, "/Kids").and_then(|k| match resolve_value(k, objects) {
            Some(PdfObjValue::Array(arr)) if !arr.is_empty() => Some(arr.as_slice()),
            _ => None,
        });
        if let Some(kid_arr) = kids {
            // An intermediate field node passes `/FT` down to its kids; a field whose `/Kids` are pure
            // widget annotations has `/FT` on itself and widgets without `/T` below it.
            let kids_are_named_fields = kid_arr.iter().any(|k| {
                matches!(
                    resolve_value(k, objects),
                    Some(PdfObjValue::Dict(kd)) if dict_has_non_null_key(kd, "/T")
                )
            });
            if kids_are_named_fields {
                walk_form_fields_detailed(
                    kid_arr,
                    state,
                    FormWalkParent {
                        name: full_name.as_deref(),
                        alt: own_alt.as_deref(),
                        value: own_val.as_deref(),
                    },
                    depth + 1,
                );
                continue;
            }
        }
        if kids.is_some()
            || dict_has_non_null_key(entries, "/FT")
            || dict_has_non_null_key(entries, "/T")
        {
            state.out.field_count += 1;
            let role = own_name
                .as_deref()
                .and_then(match_form_role)
                .or_else(|| full_name.as_deref().and_then(match_form_role))
                .or_else(|| own_alt.as_deref().and_then(match_form_role));
            let display_name = own_alt
                .as_deref()
                .or(full_name.as_deref())
                .or(own_name.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if let Some(field_name) = display_name {
                if state.out.fields.len() < MAX_EXTRACTED_FORM_FIELDS {
                    state.out.fields.push(PdfFormField {
                        name: field_name.to_string(),
                        value: own_val.clone(),
                    });
                }
            }
            match (role, own_val) {
                (Some(FormRole::Applicant), Some(val)) if state.out.applicant.is_none() => {
                    state.out.applicant = Some(val);
                }
                (Some(FormRole::Department), Some(val)) if state.out.department.is_none() => {
                    state.out.department = Some(val);
                }
                _ => {}
            }
        }
    }
}

fn collect_pdf_form_fields(
    catalog: Option<&[(String, PdfObjValue)]>,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> PdfFormReadback {
    let mut state = FormWalkState {
        objects,
        visited: HashSet::new(),
        nodes: 0,
        out: PdfFormReadback::default(),
    };
    if let Some(acro_dict) = catalog
        .and_then(|c| dict_get(c, "/AcroForm"))
        .and_then(|v| match resolve_value(v, objects) {
            Some(PdfObjValue::Dict(d)) => Some(d.as_slice()),
            _ => None,
        })
    {
        state.out.has_acro_form = true;
        if let Some(PdfObjValue::Array(fields)) =
            dict_get(acro_dict, "/Fields").and_then(|f| resolve_value(f, objects))
        {
            walk_form_fields_detailed(fields, &mut state, FormWalkParent::default(), 0);
        }
    }
    state.out
}

impl PdfSerializer {
    pub fn parse_outlines_and_form_fields(
        bytes: &[u8],
    ) -> Result<(Vec<PdfOutlineNode>, PdfFormReadback), EasyOfficeError> {
        let (objects, trailers) = decode_pdf_objects(bytes)?;
        let ordered_objs = ordered_by_sequence(&objects);
        let catalog = resolve_catalog_dict(&objects, &ordered_objs, &trailers);
        let (_, _, _, nodes) = collect_pdf_outlines(catalog, &objects);
        let form = collect_pdf_form_fields(catalog, &objects);
        Ok((nodes, form))
    }

    pub fn parse_outline_nodes(bytes: &[u8]) -> Result<Vec<PdfOutlineNode>, EasyOfficeError> {
        let (objects, trailers) = decode_pdf_objects(bytes)?;
        let ordered_objs = ordered_by_sequence(&objects);
        let catalog = resolve_catalog_dict(&objects, &ordered_objs, &trailers);
        let (_, _, _, nodes) = collect_pdf_outlines(catalog, &objects);
        Ok(nodes)
    }

    pub fn parse_form_fields(bytes: &[u8]) -> Result<PdfFormReadback, EasyOfficeError> {
        let (objects, trailers) = decode_pdf_objects(bytes)?;
        let ordered_objs = ordered_by_sequence(&objects);
        let catalog = resolve_catalog_dict(&objects, &ordered_objs, &trailers);
        Ok(collect_pdf_form_fields(catalog, &objects))
    }

    pub fn inspect_readback(bytes: &[u8]) -> Result<PdfReadbackReport, EasyOfficeError> {
        let model = Self::parse(bytes)?;
        let (objects, trailers) = decode_pdf_objects(bytes)?;
        let ordered_objs = ordered_by_sequence(&objects);
        let (_, pages_target) =
            resolve_catalog_and_pages_target(&objects, &ordered_objs, &trailers);
        let catalog = resolve_catalog_dict(&objects, &ordered_objs, &trailers);

        let mut meta_walk = MetaWalk {
            objects: &objects,
            visited: HashSet::new(),
            nodes: 0,
            pages: Vec::with_capacity(model.pages.len()),
        };
        if let Some(target) = pages_target {
            walk_page_meta(
                &mut meta_walk,
                target,
                (DEFAULT_A4_WIDTH_PT, DEFAULT_A4_HEIGHT_PT),
                0,
            );
        } else {
            for &(_, obj) in &ordered_objs {
                if let PdfObjValue::Dict(ref d) = obj.val {
                    if dict_name_eq(d, "/Type", "/Page") {
                        let dims = parse_box_dims(d, &objects)
                            .unwrap_or((DEFAULT_A4_WIDTH_PT, DEFAULT_A4_HEIGHT_PT));
                        let (link_count, markup_count, widget_count) =
                            count_page_annots(d, &objects);
                        meta_walk.pages.push(PageMeta {
                            width_pt: dims.0,
                            height_pt: dims.1,
                            link_count,
                            markup_count,
                            widget_count,
                        });
                    }
                }
            }
        }

        let (has_outlines, outline_count, outline_titles, outline_nodes) =
            collect_pdf_outlines(catalog, &objects);

        let form_info = collect_pdf_form_fields(catalog, &objects);
        let form_field_count = form_info.field_count;
        let has_acro_form = form_info.has_acro_form;

        let mut link_annotation_count = 0usize;
        let mut markup_annotation_count = 0usize;
        let mut pages = Vec::with_capacity(model.pages.len());
        for (idx, p) in model.pages.into_iter().enumerate() {
            let meta = meta_walk.pages.get(idx).copied().unwrap_or_default();
            link_annotation_count += meta.link_count;
            markup_annotation_count += meta.markup_count;
            pages.push(PdfPageReadback {
                page_number: p.page_number,
                rotation_degrees: p.rotation_degrees,
                width_pt: meta.width_pt,
                height_pt: meta.height_pt,
                body_text: p.body_text,
                approval_stamp: p.approval_stamp,
                link_count: meta.link_count,
                markup_count: meta.markup_count,
                widget_count: meta.widget_count,
            });
        }

        Ok(PdfReadbackReport {
            title: model.title,
            watermark_text: model.watermark_text,
            pages,
            outline_count,
            outline_titles,
            outline_nodes,
            link_annotation_count,
            markup_annotation_count,
            form_field_count,
            has_acro_form,
            has_outlines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_pdf(body_objects: &[(u32, &str)], trailer_root: u32) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for &(id, body) in body_objects {
            offsets.push((id, out.len()));
            out.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
        }
        let xref_pos = out.len();
        let max_id = body_objects.iter().map(|(id, _)| *id).max().unwrap_or(1);
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", max_id + 1).as_bytes());
        for obj_id in 1..=max_id {
            if let Some(&(_, off)) = offsets.iter().find(|&&(id, _)| id == obj_id) {
                out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
            } else {
                out.extend_from_slice(b"0000000000 65535 f \n");
            }
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {trailer_root} 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
                max_id + 1
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn non_form_pdf_reports_no_acro_form() {
        let pdf = minimal_pdf(
            &[
                (1, "<< /Type /Catalog /Pages 2 0 R >>"),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
                (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>"),
            ],
            1,
        );
        let form = PdfSerializer::parse_form_fields(&pdf).unwrap();
        assert_eq!(form, PdfFormReadback::default());
    }

    #[test]
    fn acro_form_extracts_utf16_values_and_ignores_cycles() {
        let pdf = minimal_pdf(
            &[
                (1, "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>"),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
                (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>"),
                (4, "<< /Fields [5 0 R 6 0 R 7 0 R] >>"),
                (5, "<< /FT /Tx /T (applicant) /V <FEFFCD5CC131> >>"),
                (6, "<< /FT /Tx /T (department) /V <FEFFBD80C11C> >>"),
                // Cyclic kid reference back to 6 0 R must not loop or double-count.
                (7, "<< /T (group) /Kids [6 0 R] >>"),
            ],
            1,
        );
        let form = PdfSerializer::parse_form_fields(&pdf).unwrap();
        assert!(form.has_acro_form);
        assert_eq!(form.field_count, 2);
        assert_eq!(form.applicant.as_deref(), Some("최성"));
        assert_eq!(form.department.as_deref(), Some("부서"));
        assert_eq!(
            form.fields,
            vec![
                PdfFormField {
                    name: "applicant".to_string(),
                    value: Some("최성".to_string()),
                },
                PdfFormField {
                    name: "department".to_string(),
                    value: Some("부서".to_string()),
                },
            ]
        );
    }

    #[test]
    fn acro_form_extracts_arbitrary_tx_and_btn_fields_within_cap() {
        let mut objs = vec![
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>".to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>".to_string(),
            ),
        ];
        let mut field_refs = Vec::new();
        objs.push((
            5,
            "<< /FT /Tx /T (contractNo) /TU (계약 번호) /V (2026-042) >>".to_string(),
        ));
        field_refs.push("5 0 R".to_string());
        objs.push((6, "<< /FT /Btn /T (agreed) /V /Yes >>".to_string()));
        field_refs.push("6 0 R".to_string());
        for i in 0..40u32 {
            let id = 10 + i;
            objs.push((id, format!("<< /FT /Tx /T (extra_{i}) /V (val_{i}) >>")));
            field_refs.push(format!("{id} 0 R"));
        }
        objs.push((4, format!("<< /Fields [{}] >>", field_refs.join(" "))));
        let body: Vec<(u32, &str)> = objs.iter().map(|(id, s)| (*id, s.as_str())).collect();
        let pdf = minimal_pdf(&body, 1);
        let form = PdfSerializer::parse_form_fields(&pdf).unwrap();
        assert!(form.has_acro_form);
        assert_eq!(form.field_count, 42);
        assert_eq!(form.fields.len(), MAX_EXTRACTED_FORM_FIELDS);
        assert_eq!(
            form.fields[0],
            PdfFormField {
                name: "계약 번호".to_string(),
                value: Some("2026-042".to_string()),
            }
        );
        assert_eq!(
            form.fields[1],
            PdfFormField {
                name: "agreed".to_string(),
                value: Some("Yes".to_string()),
            }
        );
    }

    #[test]
    fn single_pass_outlines_and_form_fields_matches_individual_parsers() {
        let pdf = minimal_pdf(
            &[
                (
                    1,
                    "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R /AcroForm 6 0 R >>",
                ),
                (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
                (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>"),
                (4, "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>"),
                (5, "<< /Title (1. 개요) /Parent 4 0 R >>"),
                (6, "<< /Fields [7 0 R] >>"),
                (7, "<< /FT /Tx /T (applicant) /V (홍길동) >>"),
            ],
            1,
        );
        let (nodes, form) = PdfSerializer::parse_outlines_and_form_fields(&pdf).unwrap();
        assert_eq!(nodes, PdfSerializer::parse_outline_nodes(&pdf).unwrap());
        assert_eq!(form, PdfSerializer::parse_form_fields(&pdf).unwrap());
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].title, "1. 개요");
        assert_eq!(form.applicant.as_deref(), Some("홍길동"));
    }
}

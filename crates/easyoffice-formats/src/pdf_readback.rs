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

fn walk_form_fields(
    items: &[PdfObjValue],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
    visited: &mut HashSet<(u32, u16)>,
    nodes: &mut usize,
    leaf_fields: &mut usize,
    depth: usize,
) {
    if depth > MAX_PDF_PAGE_TREE_DEPTH {
        return;
    }
    for item in items {
        if *nodes >= MAX_FORM_NODES {
            return;
        }
        let entries = match item {
            PdfObjValue::Ref(on, gn) => {
                if !visited.insert((*on, *gn)) {
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
        *nodes += 1;
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
                walk_form_fields(kid_arr, objects, visited, nodes, leaf_fields, depth + 1);
            } else {
                *leaf_fields += 1;
            }
        } else if dict_has_non_null_key(entries, "/FT") || dict_has_non_null_key(entries, "/T") {
            *leaf_fields += 1;
        }
    }
}

impl PdfSerializer {
    pub fn parse_outline_nodes(bytes: &[u8]) -> Result<Vec<PdfOutlineNode>, EasyOfficeError> {
        let (objects, trailers) = decode_pdf_objects(bytes)?;
        let ordered_objs = ordered_by_sequence(&objects);
        let catalog = resolve_catalog_dict(&objects, &ordered_objs, &trailers);
        let (_, _, _, nodes) = collect_pdf_outlines(catalog, &objects);
        Ok(nodes)
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

        let mut form_field_count = 0usize;
        let mut has_acro_form = false;
        if let Some(acro_dict) = catalog
            .and_then(|c| dict_get(c, "/AcroForm"))
            .and_then(|v| match resolve_value(v, &objects) {
                Some(PdfObjValue::Dict(d)) => Some(d.as_slice()),
                _ => None,
            })
        {
            has_acro_form = true;
            if let Some(PdfObjValue::Array(fields)) =
                dict_get(acro_dict, "/Fields").and_then(|f| resolve_value(f, &objects))
            {
                let mut visited = HashSet::new();
                let mut nodes = 0usize;
                walk_form_fields(
                    fields,
                    &objects,
                    &mut visited,
                    &mut nodes,
                    &mut form_field_count,
                    0,
                );
            }
        }

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

//! Preserves safe links, existing text/markup annotations and document outlines (bookmarks)
//! across page selection, rotation, merge and split in the PDF studio (PD-245).
//!
//! Reference implementations (structure only): `pdf-writer` 0.15 `Annotation`, `Outline` and
//! `Destination` builders (MIT OR Apache-2.0) and ISO 32000-1 §12.3.2 (destinations), §12.3.3
//! (document outline) and §12.5.6 (link and text markup annotations). External URIs pass through
//! `crate::open_link::open_target` (`http`, `https`, `mailto` without attachments); executable
//! actions (`/JavaScript`, `/Launch`, `/SubmitForm`, `/ImportData`, `/GoToR`) are stripped.

use crate::open_link::open_target;
use crate::pdf_edit_budget::{
    Deadline, MAX_DEST_ENTRIES, MAX_NAME_TREE_NODES, MAX_OUTLINE_DEPTH, MAX_PAGE_ANNOTS,
    MAX_TOTAL_ANNOTS,
};
use crate::pdf_edit_wire::PagePlan;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, ObjRef, Object, ObjectIdentifier};
use hayro_syntax::page::Page;
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationIcon, AnnotationType, BlendMode,
};
use hayro_write::pdf_writer::{
    Array as WArray, Content, Finish, Name as WName, Null, Pdf, Rect, Ref, Str, TextStr,
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const MAX_TITLE_CHARS: usize = 512;
const MAX_NOTE_CHARS: usize = 4_096;
const UNDERLINE_WIDTH_PT: f32 = 1.0;

/// Decodes a PDF text string (UTF-16BE/LE with BOM or PDFDocEncoding) and strips control bytes.
pub fn decode_pdf_text(bytes: &[u8], allow_newlines: bool, max_chars: usize) -> String {
    let raw = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        decode_utf16(rest, true)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        decode_utf16(rest, false)
    } else {
        bytes.iter().copied().map(pdf_doc_char).collect()
    };
    let mut out = String::with_capacity(raw.len().min(max_chars));
    let mut count = 0usize;
    for ch in raw.chars() {
        if count >= max_chars {
            break;
        }
        let keep = match ch {
            '\n' | '\r' => allow_newlines,
            '\t' => true,
            c if c.is_control() => false,
            _ => true,
        };
        if keep {
            out.push(ch);
            count += 1;
        }
    }
    out
}

fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let pairs = bytes.len() / 2;
    let units = (0..pairs).map(|i| {
        let (a, b) = (bytes[i * 2], bytes[i * 2 + 1]);
        if big_endian {
            u16::from_be_bytes([a, b])
        } else {
            u16::from_le_bytes([a, b])
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

/// ISO 32000-1 Annex D.2 `PDFDocEncoding` table for `0x80..=0xA0` (all other bytes match Latin-1).
fn pdf_doc_char(byte: u8) -> char {
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

#[derive(Clone, Copy, Debug)]
struct PageTarget {
    page_ref: Ref,
    dx: f32,
    dy: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum DestView {
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
    },
    Fit,
    FitH(Option<f32>),
    FitV(Option<f32>),
    FitR([f32; 4]),
    FitB,
    FitBH(Option<f32>),
    FitBV(Option<f32>),
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedDest {
    pub page_ref: Ref,
    pub view: DestView,
}

impl ResolvedDest {
    pub fn write_into(self, mut arr: WArray<'_>) {
        arr.item(self.page_ref);
        match self.view {
            DestView::Xyz { left, top, zoom } => {
                arr.item(WName(b"XYZ"));
                opt_num(&mut arr, left);
                opt_num(&mut arr, top);
                opt_num(&mut arr, zoom);
            }
            DestView::Fit => {
                arr.item(WName(b"Fit"));
            }
            DestView::FitH(top) => {
                arr.item(WName(b"FitH"));
                opt_num(&mut arr, top);
            }
            DestView::FitV(left) => {
                arr.item(WName(b"FitV"));
                opt_num(&mut arr, left);
            }
            DestView::FitR([x0, y0, x1, y1]) => {
                arr.item(WName(b"FitR"));
                arr.items([x0, y0, x1, y1]);
            }
            DestView::FitB => {
                arr.item(WName(b"FitB"));
            }
            DestView::FitBH(top) => {
                arr.item(WName(b"FitBH"));
                opt_num(&mut arr, top);
            }
            DestView::FitBV(left) => {
                arr.item(WName(b"FitBV"));
                opt_num(&mut arr, left);
            }
        }
        arr.finish();
    }
}

fn opt_num(arr: &mut WArray<'_>, value: Option<f32>) {
    match value.filter(|v| v.is_finite()) {
        Some(v) => {
            arr.item(v);
        }
        None => {
            arr.item(Null);
        }
    }
}

/// Maps source page objects, page indexes and named destinations to output page references.
pub struct PageDestMap {
    by_obj: HashMap<(usize, ObjectIdentifier), PageTarget>,
    by_index: HashMap<(usize, usize), PageTarget>,
    named: HashMap<(usize, Vec<u8>), ResolvedDest>,
}

impl PageDestMap {
    pub fn build(
        parsed: &BTreeMap<usize, ReadPdf>,
        pages: &[PagePlan],
        page_refs: &[Ref],
        deadline: &Deadline,
    ) -> Result<Self, String> {
        let mut by_obj = HashMap::with_capacity(pages.len());
        let mut by_index = HashMap::with_capacity(pages.len());
        for (plan, &page_ref) in pages.iter().zip(page_refs) {
            let pdf = parsed
                .get(&plan.source)
                .ok_or("쪽의 원본 PDF가 없습니다.")?;
            let page = pdf.pages().get(plan.index).ok_or("없는 쪽을 가리킵니다.")?;
            let crop = page.intersected_crop_box();
            #[allow(clippy::cast_possible_truncation)]
            let target = PageTarget {
                page_ref,
                dx: -(crop.x0 as f32),
                dy: -(crop.y0 as f32),
            };
            by_index.entry((plan.source, plan.index)).or_insert(target);
            if let Some(id) = page.raw().obj_id() {
                by_obj.entry((plan.source, id)).or_insert(target);
            }
        }
        let mut map = Self {
            by_obj,
            by_index,
            named: HashMap::new(),
        };
        for (&source, pdf) in parsed {
            deadline.check()?;
            map.load_source_dests(source, pdf)?;
        }
        Ok(map)
    }

    fn load_source_dests(&mut self, source: usize, pdf: &ReadPdf) -> Result<(), String> {
        let xref = pdf.xref();
        let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
            return Ok(());
        };
        let mut count = 0usize;
        if let Some(dests_dict) = catalog.get::<Dict<'_>>(b"Dests") {
            for key in dests_dict.keys() {
                count += 1;
                if count > MAX_DEST_ENTRIES {
                    return Err(format!(
                        "PDF의 이름 있는 목적지 수가 한도({MAX_DEST_ENTRIES})를 초과합니다."
                    ));
                }
                if let Some(dest) = self.parse_dest_value(source, pdf, &dests_dict, key.as_ref()) {
                    self.named.insert((source, key.as_ref().to_vec()), dest);
                }
            }
        }
        if let Some(names) = catalog.get::<Dict<'_>>(b"Names") {
            if let Some(tree) = names.get::<Dict<'_>>(b"Dests") {
                let mut seen = HashSet::new();
                let mut nodes = 0usize;
                self.walk_name_tree(source, pdf, &tree, 1, &mut seen, &mut nodes, &mut count)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_name_tree(
        &mut self,
        source: usize,
        pdf: &ReadPdf,
        node: &Dict<'_>,
        depth: usize,
        seen: &mut HashSet<ObjectIdentifier>,
        nodes: &mut usize,
        count: &mut usize,
    ) -> Result<(), String> {
        if depth > MAX_OUTLINE_DEPTH {
            return Err(format!(
                "PDF 목적지 이름 트리가 허용 깊이({MAX_OUTLINE_DEPTH}단계)를 초과합니다."
            ));
        }
        *nodes += 1;
        if *nodes > MAX_NAME_TREE_NODES {
            return Err(format!(
                "PDF 목적지 이름 트리 노드 수가 한도({MAX_NAME_TREE_NODES})를 초과합니다."
            ));
        }
        if node.obj_id().is_some_and(|id| !seen.insert(id)) {
            return Err("PDF 목적지 이름 트리에 순환 참조가 있습니다.".to_string());
        }
        if let Some(arr) = node.get::<Array<'_>>(b"Names") {
            let items: Vec<MaybeRef<Object<'_>>> =
                arr.raw_iter().take(MAX_DEST_ENTRIES * 2 + 2).collect();
            let mut idx = 0usize;
            while idx + 1 < items.len() {
                *count += 1;
                if *count > MAX_DEST_ENTRIES {
                    return Err(format!(
                        "PDF의 이름 있는 목적지 수가 한도({MAX_DEST_ENTRIES})를 초과합니다."
                    ));
                }
                if let Some(key_bytes) = resolve_bytes(pdf, &items[idx]) {
                    if let Some(dest) = self.resolve_dest_raw(source, pdf, &items[idx + 1]) {
                        self.named.insert((source, key_bytes), dest);
                    }
                }
                idx += 2;
            }
        }
        if let Some(kids) = node.get::<Array<'_>>(b"Kids") {
            for raw_kid in kids.raw_iter() {
                if let MaybeRef::Ref(r) = raw_kid {
                    if seen.contains(&ObjectIdentifier::from(r)) {
                        return Err("PDF 목적지 이름 트리에 순환 참조가 있습니다.".to_string());
                    }
                }
            }
            for kid in kids.iter::<Dict<'_>>() {
                self.walk_name_tree(source, pdf, &kid, depth + 1, seen, nodes, count)?;
            }
        }
        Ok(())
    }

    fn parse_dest_value(
        &self,
        source: usize,
        pdf: &ReadPdf,
        dict: &Dict<'_>,
        key: &[u8],
    ) -> Option<ResolvedDest> {
        let raw = dict.get_raw::<Object<'_>>(key)?;
        self.resolve_dest_raw(source, pdf, &raw)
    }

    pub fn resolve_dest_entry(
        &self,
        source: usize,
        pdf: &ReadPdf,
        dict: &Dict<'_>,
        key: &[u8],
    ) -> Option<ResolvedDest> {
        let raw = dict.get_raw::<Object<'_>>(key)?;
        match &raw {
            MaybeRef::NotRef(Object::Name(n)) => {
                self.named.get(&(source, n.as_ref().to_vec())).copied()
            }
            MaybeRef::NotRef(Object::String(s)) => {
                self.named.get(&(source, s.as_bytes().to_vec())).copied()
            }
            MaybeRef::Ref(r) => {
                let obj = pdf.xref().get::<Object<'_>>((*r).into())?;
                match obj {
                    Object::Name(n) => self.named.get(&(source, n.as_ref().to_vec())).copied(),
                    Object::String(s) => self.named.get(&(source, s.as_bytes().to_vec())).copied(),
                    other => self.resolve_dest_object(source, &other),
                }
            }
            _ => self.resolve_dest_raw(source, pdf, &raw),
        }
    }

    fn resolve_dest_raw(
        &self,
        source: usize,
        pdf: &ReadPdf,
        raw: &MaybeRef<Object<'_>>,
    ) -> Option<ResolvedDest> {
        match raw {
            MaybeRef::Ref(r) => {
                let obj = pdf.xref().get::<Object<'_>>((*r).into())?;
                self.resolve_dest_object(source, &obj)
            }
            MaybeRef::NotRef(obj) => self.resolve_dest_object(source, obj),
        }
    }

    fn resolve_dest_object(&self, source: usize, obj: &Object<'_>) -> Option<ResolvedDest> {
        match obj {
            Object::Array(arr) => self.parse_explicit_dest(source, arr),
            Object::Dict(dict) => {
                let arr = dict.get::<Array<'_>>(b"D")?;
                self.parse_explicit_dest(source, &arr)
            }
            _ => None,
        }
    }

    fn parse_explicit_dest(&self, source: usize, arr: &Array<'_>) -> Option<ResolvedDest> {
        let items: Vec<MaybeRef<Object<'_>>> = arr.raw_iter().take(8).collect();
        let target = match items.first()? {
            MaybeRef::Ref(ObjRef {
                obj_number,
                gen_number,
            }) => {
                let id = ObjectIdentifier::new(*obj_number, *gen_number);
                *self.by_obj.get(&(source, id))?
            }
            MaybeRef::NotRef(Object::Number(n)) => {
                let idx = usize::try_from(n.as_i64()).ok()?;
                *self.by_index.get(&(source, idx))?
            }
            _ => return None,
        };
        let mode = match items.get(1) {
            Some(MaybeRef::NotRef(Object::Name(n))) => n.as_str(),
            _ => "Fit",
        };
        let num_at = |i: usize| match items.get(i) {
            Some(MaybeRef::NotRef(Object::Number(n))) => Some(n.as_f32()),
            _ => None,
        };
        let view = match mode {
            "XYZ" => DestView::Xyz {
                left: num_at(2).map(|x| x + target.dx),
                top: num_at(3).map(|y| y + target.dy),
                zoom: num_at(4),
            },
            "Fit" => DestView::Fit,
            "FitH" => DestView::FitH(num_at(2).map(|y| y + target.dy)),
            "FitV" => DestView::FitV(num_at(2).map(|x| x + target.dx)),
            "FitR" => DestView::FitR([
                num_at(2)? + target.dx,
                num_at(3)? + target.dy,
                num_at(4)? + target.dx,
                num_at(5)? + target.dy,
            ]),
            "FitB" => DestView::FitB,
            "FitBH" => DestView::FitBH(num_at(2).map(|y| y + target.dy)),
            "FitBV" => DestView::FitBV(num_at(2).map(|x| x + target.dx)),
            _ => DestView::Fit,
        };
        Some(ResolvedDest {
            page_ref: target.page_ref,
            view,
        })
    }
}

fn resolve_bytes(pdf: &ReadPdf, item: &MaybeRef<Object<'_>>) -> Option<Vec<u8>> {
    match item {
        MaybeRef::NotRef(Object::String(s)) => Some(s.as_bytes().to_vec()),
        MaybeRef::NotRef(Object::Name(n)) => Some(n.as_ref().to_vec()),
        MaybeRef::Ref(r) => match pdf.xref().get::<Object<'_>>((*r).into())? {
            Object::String(s) => Some(s.as_bytes().to_vec()),
            Object::Name(n) => Some(n.as_ref().to_vec()),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub enum NavTarget {
    Dest(ResolvedDest),
    Uri(String),
}

pub fn resolve_nav_target(
    dest_map: &PageDestMap,
    source: usize,
    pdf: &ReadPdf,
    dict: &Dict<'_>,
) -> Option<NavTarget> {
    if let Some(dest) = dest_map.resolve_dest_entry(source, pdf, dict, b"Dest") {
        return Some(NavTarget::Dest(dest));
    }
    let action = dict.get::<Dict<'_>>(b"A")?;
    let kind = action.get::<Name<'_>>(b"S")?;
    match kind.as_str() {
        "GoTo" => dest_map
            .resolve_dest_entry(source, pdf, &action, b"D")
            .map(NavTarget::Dest),
        "URI" => {
            let raw_uri = action.get::<hayro_syntax::object::String<'_>>(b"URI")?;
            let text = decode_pdf_text(raw_uri.as_bytes(), false, 2_048);
            open_target(&text).map(NavTarget::Uri)
        }
        // Reject Launch, JavaScript, SubmitForm, ImportData, GoToR, GoToE and unknown actions.
        _ => None,
    }
}

/// Oriented quadrilateral `[tl_x, tl_y, tr_x, tr_y, bl_x, bl_y, br_x, br_y]` in output page points.
pub type OrientedQuad = [f32; 8];

/// Quantized `(subtype_tag, x0, y0, x1, y1)` key (tenth of a point) used to avoid duplicate markups.
pub type MarkupKey = (u8, i32, i32, i32, i32);

pub fn markup_key(kind_tag: u8, rect: [f32; 4]) -> MarkupKey {
    #[allow(clippy::cast_possible_truncation)]
    let q = |v: f32| (v * 10.0).round() as i32;
    (kind_tag, q(rect[0]), q(rect[1]), q(rect[2]), q(rect[3]))
}

/// Preserves safe `/Link` and existing text/markup annotations on one page.
#[allow(clippy::too_many_arguments)]
pub fn preserve_page_annots(
    out: &mut Pdf,
    next: &mut Ref,
    dest_map: &PageDestMap,
    source: usize,
    pdf: &ReadPdf,
    src_page: &Page<'_>,
    out_page_ref: Ref,
    total_annots: &mut usize,
    seen_markups: &mut HashSet<MarkupKey>,
) -> Result<Vec<Ref>, String> {
    let Some(annots) = src_page.raw().get::<Array<'_>>(b"Annots") else {
        return Ok(Vec::new());
    };
    let crop = src_page.intersected_crop_box();
    #[allow(clippy::cast_possible_truncation)]
    let (dx, dy) = (-(crop.x0 as f32), -(crop.y0 as f32));
    let mut kept = Vec::new();
    for (idx, annot) in annots.iter::<Dict<'_>>().enumerate() {
        if idx >= MAX_PAGE_ANNOTS {
            return Err(format!(
                "쪽 하나의 주석 수가 한도({MAX_PAGE_ANNOTS})를 초과합니다."
            ));
        }
        let Some(subtype) = annot.get::<Name<'_>>(b"Subtype") else {
            continue;
        };
        let Some(rect) = read_shifted_rect(&annot, dx, dy) else {
            continue;
        };
        let added = match subtype.as_str() {
            "Link" => write_link_annot(out, next, dest_map, source, pdf, &annot, rect, dx, dy),
            "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => write_existing_markup_annot(
                out,
                next,
                subtype.as_str(),
                &annot,
                rect,
                dx,
                dy,
                out_page_ref,
                seen_markups,
            ),
            "Text" => Some(write_text_note_annot(out, next, &annot, rect, out_page_ref)),
            _ => None,
        };
        if let Some(annot_ref) = added {
            *total_annots += 1;
            if *total_annots > MAX_TOTAL_ANNOTS {
                return Err(format!(
                    "문서 전체의 주석 수가 한도({MAX_TOTAL_ANNOTS})를 초과합니다."
                ));
            }
            kept.push(annot_ref);
        }
    }
    Ok(kept)
}

pub fn read_shifted_rect(dict: &Dict<'_>, dx: f32, dy: f32) -> Option<[f32; 4]> {
    let arr = dict.get::<Array<'_>>(b"Rect")?;
    let mut nums = arr.iter::<Number>();
    let (a, b, c, d) = (
        nums.next()?.as_f32() + dx,
        nums.next()?.as_f32() + dy,
        nums.next()?.as_f32() + dx,
        nums.next()?.as_f32() + dy,
    );
    if ![a, b, c, d].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

fn read_shifted_quads(dict: &Dict<'_>, dx: f32, dy: f32) -> Vec<OrientedQuad> {
    let Some(arr) = dict.get::<Array<'_>>(b"QuadPoints") else {
        return Vec::new();
    };
    let vals: Vec<f32> = arr
        .iter::<Number>()
        .take(2_048 * 8)
        .map(|n| n.as_f32())
        .collect();
    if vals.len() < 8 || (vals.len() & 7) != 0 || !vals.iter().all(|v| v.is_finite()) {
        return Vec::new();
    }
    let count = vals.len() >> 3;
    (0..count)
        .map(|i| {
            let c = &vals[i * 8..i * 8 + 8];
            [
                c[0] + dx,
                c[1] + dy,
                c[2] + dx,
                c[3] + dy,
                c[4] + dx,
                c[5] + dy,
                c[6] + dx,
                c[7] + dy,
            ]
        })
        .collect()
}

pub fn read_rgb_color(dict: &Dict<'_>) -> Option<[f32; 3]> {
    let arr = dict.get::<Array<'_>>(b"C")?;
    let vals: Vec<f32> = arr
        .iter::<Number>()
        .take(4)
        .map(|n| n.as_f32().clamp(0.0, 1.0))
        .collect();
    match vals.as_slice() {
        [g] => Some([*g, *g, *g]),
        [r, g, b] => Some([*r, *g, *b]),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn write_link_annot(
    out: &mut Pdf,
    next: &mut Ref,
    dest_map: &PageDestMap,
    source: usize,
    pdf: &ReadPdf,
    src: &Dict<'_>,
    rect: [f32; 4],
    dx: f32,
    dy: f32,
) -> Option<Ref> {
    let nav = resolve_nav_target(dest_map, source, pdf, src)?;
    let annot_ref = next.bump();
    let mut annot = out.annotation(annot_ref);
    annot
        .subtype(AnnotationType::Link)
        .rect(Rect::new(rect[0], rect[1], rect[2], rect[3]));
    if let Some(flags) = src.get::<u32>(b"F") {
        annot.flags(AnnotationFlags::from_bits_truncate(flags));
    }
    if let Some(border) = src.get::<Array<'_>>(b"Border") {
        let nums: Vec<f32> = border
            .iter::<Number>()
            .take(3)
            .map(|n| n.as_f32())
            .collect();
        if let [h, v, w] = nums.as_slice() {
            annot.border(*h, *v, *w, None);
        } else {
            annot.border(0.0, 0.0, 0.0, None);
        }
    } else {
        annot.border(0.0, 0.0, 0.0, None);
    }
    if let Some([r, g, b]) = read_rgb_color(src) {
        annot.color_rgb(r, g, b);
    }
    let quads = read_shifted_quads(src, dx, dy);
    if !quads.is_empty() {
        annot.quad_points(quads.iter().flatten().copied());
    }
    match nav {
        NavTarget::Dest(dest) => {
            dest.write_into(annot.insert(WName(b"Dest")).array());
        }
        NavTarget::Uri(uri) => {
            annot
                .action()
                .action_type(ActionType::Uri)
                .uri(Str(uri.as_bytes()));
        }
    }
    annot.finish();
    Some(annot_ref)
}

#[allow(clippy::too_many_arguments)]
fn write_existing_markup_annot(
    out: &mut Pdf,
    next: &mut Ref,
    subtype: &str,
    src: &Dict<'_>,
    rect: [f32; 4],
    dx: f32,
    dy: f32,
    page_ref: Ref,
    seen_markups: &mut HashSet<MarkupKey>,
) -> Option<Ref> {
    let (annot_type, tag, default_rgb) = match subtype {
        "Highlight" => (AnnotationType::Highlight, 1u8, [1.0, 1.0, 0.0]),
        "Underline" => (AnnotationType::Underline, 2u8, [0.85, 0.18, 0.18]),
        "StrikeOut" => (AnnotationType::StrikeOut, 3u8, [0.85, 0.18, 0.18]),
        "Squiggly" => (AnnotationType::Squiggly, 4u8, [0.85, 0.18, 0.18]),
        _ => return None,
    };
    let mut quads = read_shifted_quads(src, dx, dy);
    if quads.is_empty() {
        quads.push([
            rect[0], rect[3], rect[2], rect[3], rect[0], rect[1], rect[2], rect[1],
        ]);
    }
    let color = read_rgb_color(src).unwrap_or(default_rgb);
    let contents = src
        .get::<hayro_syntax::object::String<'_>>(b"Contents")
        .map(|s| decode_pdf_text(s.as_bytes(), true, MAX_NOTE_CHARS))
        .unwrap_or_default();
    let author = src
        .get::<hayro_syntax::object::String<'_>>(b"T")
        .map(|s| decode_pdf_text(s.as_bytes(), false, MAX_TITLE_CHARS));
    let flags = src
        .get::<u32>(b"F")
        .map(AnnotationFlags::from_bits_truncate)
        .unwrap_or(AnnotationFlags::PRINT);
    seen_markups.insert(markup_key(tag, rect));
    Some(write_markup_annotation(
        out,
        next,
        annot_type,
        rect,
        &quads,
        color,
        &contents,
        author.as_deref(),
        flags,
        page_ref,
    ))
}

/// Writes a text markup annotation (`/Highlight`, `/Underline`, `/StrikeOut`, `/Squiggly`) and its
/// self-contained `/AP /N` form XObject stream.
#[allow(clippy::too_many_arguments)]
pub fn write_markup_annotation(
    out: &mut Pdf,
    next: &mut Ref,
    annot_type: AnnotationType,
    rect: [f32; 4],
    quads: &[OrientedQuad],
    color: [f32; 3],
    contents: &str,
    author: Option<&str>,
    flags: AnnotationFlags,
    page_ref: Ref,
) -> Ref {
    let (annot_ref, ap_ref) = (next.bump(), next.bump());
    let stream_bytes = markup_appearance_stream(annot_type, quads, color);
    {
        let mut form = out.form_xobject(ap_ref, &stream_bytes);
        form.bbox(Rect::new(rect[0], rect[1], rect[2], rect[3]));
        if annot_type == AnnotationType::Highlight {
            form.resources()
                .ext_g_states()
                .insert(WName(b"GS"))
                .start::<hayro_write::pdf_writer::writers::ExtGraphicsState<'_>>()
                .blend_mode(BlendMode::Multiply);
        }
        form.finish();
    }
    let mut annot = out.annotation(annot_ref);
    annot
        .subtype(annot_type)
        .rect(Rect::new(rect[0], rect[1], rect[2], rect[3]))
        .quad_points(quads.iter().flatten().copied())
        .color_rgb(color[0], color[1], color[2])
        .flags(flags)
        .page(page_ref);
    if !contents.is_empty() {
        annot.contents(TextStr(contents));
    }
    if let Some(author) = author.filter(|s| !s.is_empty()) {
        annot.author(TextStr(author));
    }
    annot.appearance().normal().stream(ap_ref);
    annot.finish();
    annot_ref
}

fn markup_appearance_stream(
    annot_type: AnnotationType,
    quads: &[OrientedQuad],
    color: [f32; 3],
) -> Vec<u8> {
    let mut content = Content::new();
    match annot_type {
        AnnotationType::Highlight => {
            content
                .set_parameters(WName(b"GS"))
                .set_fill_rgb(color[0], color[1], color[2]);
            for q in quads {
                content
                    .move_to(q[0], q[1])
                    .line_to(q[2], q[3])
                    .line_to(q[6], q[7])
                    .line_to(q[4], q[5])
                    .close_path()
                    .fill_nonzero();
            }
        }
        AnnotationType::StrikeOut => {
            content
                .set_stroke_rgb(color[0], color[1], color[2])
                .set_line_width(UNDERLINE_WIDTH_PT);
            for q in quads {
                let (mx0, my0) = ((q[0] + q[4]) * 0.5, (q[1] + q[5]) * 0.5);
                let (mx1, my1) = ((q[2] + q[6]) * 0.5, (q[3] + q[7]) * 0.5);
                content.move_to(mx0, my0).line_to(mx1, my1).stroke();
            }
        }
        _ => {
            content
                .set_stroke_rgb(color[0], color[1], color[2])
                .set_line_width(UNDERLINE_WIDTH_PT);
            for q in quads {
                let ux = (q[0] - q[4]) * 0.12;
                let uy = (q[1] - q[5]) * 0.12;
                content
                    .move_to(q[4] + ux, q[5] + uy)
                    .line_to(q[6] + ux, q[7] + uy)
                    .stroke();
            }
        }
    }
    content.finish().to_vec()
}

fn write_text_note_annot(
    out: &mut Pdf,
    next: &mut Ref,
    src: &Dict<'_>,
    rect: [f32; 4],
    page_ref: Ref,
) -> Ref {
    let annot_ref = next.bump();
    let mut annot = out.annotation(annot_ref);
    annot
        .subtype(AnnotationType::Text)
        .rect(Rect::new(rect[0], rect[1], rect[2], rect[3]))
        .flags(
            src.get::<u32>(b"F")
                .map(AnnotationFlags::from_bits_truncate)
                .unwrap_or(AnnotationFlags::PRINT),
        )
        .page(page_ref);
    if let Some([r, g, b]) = read_rgb_color(src) {
        annot.color_rgb(r, g, b);
    }
    if let Some(s) = src.get::<hayro_syntax::object::String<'_>>(b"Contents") {
        let text = decode_pdf_text(s.as_bytes(), true, MAX_NOTE_CHARS);
        if !text.is_empty() {
            annot.contents(TextStr(&text));
        }
    }
    if let Some(s) = src.get::<hayro_syntax::object::String<'_>>(b"T") {
        let author = decode_pdf_text(s.as_bytes(), false, MAX_TITLE_CHARS);
        if !author.is_empty() {
            annot.author(TextStr(&author));
        }
    }
    let icon = match src.get::<Name<'_>>(b"Name").as_ref().map(Name::as_str) {
        Some("Comment") => AnnotationIcon::Comment,
        Some("Key") => AnnotationIcon::Key,
        Some("Help") => AnnotationIcon::Help,
        Some("NewParagraph") => AnnotationIcon::NewParagraph,
        Some("Paragraph") => AnnotationIcon::Paragraph,
        Some("Insert") => AnnotationIcon::Insert,
        _ => AnnotationIcon::Note,
    };
    annot.icon(icon);
    annot.finish();
    annot_ref
}

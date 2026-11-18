//! Preserves `/Catalog /Outlines` (bookmarks) across page selection, rotation, merge and split in
//! the PDF studio (PD-245).
//!
//! Each used source PDF's outline hierarchy is read within [`MAX_OUTLINE_DEPTH`] and
//! [`MAX_OUTLINE_ITEMS`], checked against object-ID cycles, and pruned so items whose target page
//! was excluded by a split are dropped while parent chapters with surviving children remain.

use crate::pdf_edit_budget::{Deadline, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS};
use crate::pdf_edit_preserve::{
    decode_pdf_text, read_rgb_color, resolve_nav_target, NavTarget, PageDestMap, MAX_TITLE_CHARS,
};
use crate::pdf_edit_wire::PagePlan;
use hayro_syntax::object::{Dict, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::types::{ActionType, OutlineItemFlags};
use hayro_write::pdf_writer::{Finish, Name as WName, Pdf, Ref, Str, TextStr};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

struct OutlineNode {
    title: String,
    target: Option<NavTarget>,
    color: Option<[f32; 3]>,
    flags: u32,
    collapsed: bool,
    children: Vec<OutlineNode>,
}

/// Reads outlines from every used source PDF, prunes items whose destination page was excluded,
/// and writes `/Outlines` + `/OutlineItem` objects. Returns the root `/Outlines` reference if non-empty.
pub fn preserve_outlines(
    out: &mut Pdf,
    next: &mut Ref,
    dest_map: &PageDestMap,
    parsed: &BTreeMap<usize, ReadPdf>,
    pages: &[PagePlan],
    deadline: &Deadline,
) -> Result<Option<Ref>, String> {
    let mut ordered_sources = Vec::new();
    let mut seen_sources = HashSet::new();
    for p in pages {
        if seen_sources.insert(p.source) {
            ordered_sources.push(p.source);
        }
    }
    let mut total_items = 0usize;
    let mut roots = Vec::new();
    for source in ordered_sources {
        deadline.check()?;
        let Some(pdf) = parsed.get(&source) else {
            continue;
        };
        let xref = pdf.xref();
        let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
            continue;
        };
        let Some(outlines) = catalog.get::<Dict<'_>>(b"Outlines") else {
            continue;
        };
        let Some(first) = outlines.get::<Dict<'_>>(b"First") else {
            continue;
        };
        let mut visited = HashSet::new();
        if let Some(id) = outlines.obj_id() {
            visited.insert(id);
        }
        let siblings = read_outline_siblings(
            dest_map,
            source,
            pdf,
            first,
            1,
            &mut visited,
            &mut total_items,
        )?;
        roots.extend(siblings);
    }
    if roots.is_empty() {
        return Ok(None);
    }
    let root_ref = next.bump();
    let (first_ref, last_ref, visible) = write_outline_siblings(out, next, root_ref, &roots);
    out.outline(root_ref)
        .first(first_ref)
        .last(last_ref)
        .count(visible);
    Ok(Some(root_ref))
}

fn read_outline_siblings(
    dest_map: &PageDestMap,
    source: usize,
    pdf: &ReadPdf,
    first: Dict<'_>,
    depth: usize,
    visited: &mut HashSet<ObjectIdentifier>,
    total: &mut usize,
) -> Result<Vec<OutlineNode>, String> {
    if depth > MAX_OUTLINE_DEPTH {
        return Err(format!(
            "PDF 북마크(목차) 트리가 허용 깊이({MAX_OUTLINE_DEPTH}단계)를 초과합니다."
        ));
    }
    let mut kept = Vec::new();
    let mut cur = Some(first);
    while let Some(item) = cur {
        if item.obj_id().is_some_and(|id| !visited.insert(id)) {
            return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
        }
        *total += 1;
        if *total > MAX_OUTLINE_ITEMS {
            return Err(format!(
                "PDF 북마크(목차) 항목 수가 한도({MAX_OUTLINE_ITEMS})를 초과합니다."
            ));
        }
        let raw_title = item
            .get::<hayro_syntax::object::String<'_>>(b"Title")
            .map(|s| decode_pdf_text(s.as_bytes(), false, MAX_TITLE_CHARS))
            .unwrap_or_default();
        let mut title = String::new();
        let _ = write!(&mut title, "{}", raw_title.trim());
        if title.is_empty() {
            title.push_str("항목");
        }
        let target = resolve_nav_target(dest_map, source, pdf, &item);
        if let Some(child_ref) = item.get_ref(b"First") {
            if visited.contains(&ObjectIdentifier::from(child_ref)) {
                return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
            }
        }
        let children = match item.get::<Dict<'_>>(b"First") {
            Some(child_first) => read_outline_siblings(
                dest_map,
                source,
                pdf,
                child_first,
                depth + 1,
                visited,
                total,
            )?,
            None => Vec::new(),
        };
        if target.is_some() || !children.is_empty() {
            kept.push(OutlineNode {
                title,
                target,
                color: read_rgb_color(&item),
                flags: item.get::<u32>(b"F").unwrap_or(0),
                collapsed: item.get::<i32>(b"Count").is_some_and(|c| c < 0),
                children,
            });
        }
        if let Some(next_ref) = item.get_ref(b"Next") {
            if visited.contains(&ObjectIdentifier::from(next_ref)) {
                return Err("PDF 북마크(목차) 트리에 순환 참조가 있습니다.".to_string());
            }
        }
        cur = item.get::<Dict<'_>>(b"Next");
    }
    Ok(kept)
}

/// Writes one sibling chain under `parent_ref`; returns `(first_ref, last_ref, visible_count)`.
fn write_outline_siblings(
    out: &mut Pdf,
    next: &mut Ref,
    parent_ref: Ref,
    nodes: &[OutlineNode],
) -> (Ref, Ref, i32) {
    let refs: Vec<Ref> = (0..nodes.len()).map(|_| next.bump()).collect();
    let mut visible_total = 0i32;
    for (i, node) in nodes.iter().enumerate() {
        let child_info = (!node.children.is_empty())
            .then(|| write_outline_siblings(out, next, refs[i], &node.children));
        let mut item = out.outline_item(refs[i]);
        item.title(TextStr(&node.title)).parent(parent_ref);
        if i > 0 {
            item.prev(refs[i - 1]);
        }
        if i + 1 < refs.len() {
            item.next(refs[i + 1]);
        }
        if let Some((c_first, c_last, c_vis)) = child_info {
            item.first(c_first).last(c_last);
            if node.collapsed {
                item.count(-c_vis.max(1));
            } else {
                item.count(c_vis);
                visible_total = visible_total.saturating_add(c_vis);
            }
        }
        if let Some([r, g, b]) = node.color {
            item.color_rgb(r, g, b);
        }
        let flags = OutlineItemFlags::from_bits_truncate(node.flags);
        if !flags.is_empty() {
            item.flags(flags);
        }
        match &node.target {
            Some(NavTarget::Dest(dest)) => {
                dest.write_into(item.insert(WName(b"Dest")).array());
            }
            Some(NavTarget::Uri(uri)) => {
                item.insert(WName(b"A"))
                    .start::<hayro_write::pdf_writer::writers::Action<'_>>()
                    .action_type(ActionType::Uri)
                    .uri(Str(uri.as_bytes()));
            }
            None => {}
        }
        item.finish();
        visible_total = visible_total.saturating_add(1);
    }
    (refs[0], *refs.last().unwrap(), visible_total)
}

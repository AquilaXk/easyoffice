//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md), src/table.rs, src/text.rs, src/drawable.rs
//!
//! Rich-text table cell storage (`TSWP.StorageArchive`, type 2001) decoding: extracts cleaned cell
//! text, UTF-8 byte-indexed hyperlinks (`SMARTFIELD_TABLE` 11 / `LANGUAGE_OR_FIELD_TABLE` 19),
//! inline equations (`3005` extensions `100..=103` without raster, or `2015`), and picture
//! drawable IDs (`3005`) while counting any unimported attachment marks.

use crate::pages_proto::{read_reference, Message, ObjectTable};
use crate::pages_schema::{attachment, hyperlink, storage, ty};
use crate::rich_read_pages::equations::{
    image_equation_text, is_image_equation, standalone_equation_text,
};
use crate::rich_read_pages_image::PageImages;
use crate::EasyOfficeError;

pub(crate) const MAX_CELL_MARK_ENTRIES: usize = 50_000;

#[derive(Debug, Default)]
pub(crate) struct RichCellDecoded {
    pub text: String,
    pub links: Vec<(u32, u32, String)>,
    pub images: Vec<(u64, Option<String>)>,
    pub cell_objects: usize,
    pub unsafe_links: usize,
}

fn ref_of(m: &Message<'_>, field: u32) -> Option<u64> {
    m.bytes(field).and_then(read_reference)
}

fn parse_entry_table(
    st: &Message<'_>,
    field: u32,
    budget_label: &str,
) -> Result<Vec<(u64, Option<u64>)>, EasyOfficeError> {
    let Some(tbl) = st.sub(field)? else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    tbl.for_each_sub(storage::TABLE_ENTRIES, |e| {
        if entries.len() >= MAX_CELL_MARK_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "Pages {budget_label} 항목 수 한도 초과"
            )));
        }
        if let Some(idx) = e.varint(storage::ENTRY_INDEX) {
            entries.push((idx, ref_of(&e, storage::ENTRY_OBJECT)));
        }
        Ok(())
    })?;
    entries.sort_by_key(|(i, _)| *i);
    Ok(entries)
}

fn valid_cell_hyperlink_url(objects: &ObjectTable, id: u64) -> Option<String> {
    let raw = objects
        .get(id)
        .filter(|o| o.type_id == ty::HYPERLINK)
        .and_then(|o| Message::parse(&o.payload).ok())
        .and_then(|m| m.string(hyperlink::URL).ok().flatten())?;
    let url = raw.trim();
    let bad_char = |c: char| c <= ' ' || ('\u{7f}'..='\u{9f}').contains(&c);
    if url.is_empty() || url.len() > hyperlink::MAX_URL_BYTES || url.chars().any(bad_char) {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    let scheme = hyperlink::ALLOWED_SCHEMES
        .iter()
        .find(|s| lower.starts_with(**s))?;
    let rest = &url[scheme.len()..];
    (!rest.is_empty() && !rest.starts_with(['/', '?', '#'])).then(|| url.to_string())
}

type CellLinkCut = (u64, Option<String>);

fn cell_link_runs(
    objects: &ObjectTable,
    st: &Message<'_>,
) -> Result<(Vec<CellLinkCut>, usize), EasyOfficeError> {
    let mut chosen: Vec<(u64, Option<u64>)> = Vec::new();
    for field in [storage::SMARTFIELD_TABLE, storage::LANGUAGE_OR_FIELD_TABLE] {
        let entries = parse_entry_table(st, field, "표 셀 하이퍼링크 표")?;
        let has_link = entries.iter().any(|(_, id)| {
            id.is_some_and(|id| objects.get(id).is_some_and(|o| o.type_id == ty::HYPERLINK))
        });
        if has_link {
            chosen = entries;
            break;
        }
    }
    if chosen.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let mut unsafe_links = 0usize;
    let mut runs: Vec<(u64, Option<String>)> = Vec::with_capacity(chosen.len());
    for (idx, id) in chosen {
        let link_id = id.filter(|&i| objects.get(i).is_some_and(|o| o.type_id == ty::HYPERLINK));
        let kept = link_id.and_then(|i| valid_cell_hyperlink_url(objects, i));
        if link_id.is_some() && kept.is_none() {
            unsafe_links += 1;
        }
        if runs
            .last()
            .map_or(kept.is_some(), |(_, prev)| *prev != kept)
        {
            runs.push((idx, kept));
        }
    }
    Ok((runs, unsafe_links))
}

fn exact_attachment(entries: &[(u64, Option<u64>)], idx: u64) -> Option<u64> {
    entries
        .binary_search_by_key(&idx, |(i, _)| *i)
        .ok()
        .and_then(|pos| entries[pos].1)
}

fn resolve_cell_attachment(
    objects: &ObjectTable,
    images: &mut PageImages<'_>,
    att_id: u64,
    out: &mut String,
    cell_images: &mut Vec<(u64, Option<String>)>,
    cell_objects: &mut usize,
) -> Result<(), EasyOfficeError> {
    let drawable_id = match objects.get(att_id) {
        Some(o) if o.type_id == ty::EQUATION => Some(att_id),
        Some(o) if o.type_id == ty::ATTACHMENT => Message::parse(&o.payload)
            .ok()
            .and_then(|m| ref_of(&m, attachment::DRAWABLE)),
        _ => None,
    };
    let Some(did) = drawable_id else {
        *cell_objects += 1;
        return Ok(());
    };
    let Some(obj) = objects.get(did) else {
        *cell_objects += 1;
        return Ok(());
    };
    match obj.type_id {
        ty::EQUATION => {
            let msg = Message::parse(&obj.payload)?;
            match standalone_equation_text(&msg)? {
                Some(eq) => out.push_str(&eq),
                None => *cell_objects += 1,
            }
        }
        ty::IMAGE => {
            let Ok(msg) = Message::parse(&obj.payload) else {
                *cell_objects += 1;
                return Ok(());
            };
            if is_image_equation(&msg) {
                let eq = image_equation_text(&msg)?;
                if images.has_supported_raster(obj) {
                    cell_images.push((did, eq));
                } else if let Some(eq) = eq {
                    out.push_str(&eq);
                } else {
                    *cell_objects += 1;
                }
            } else {
                cell_images.push((did, None));
            }
        }
        _ => *cell_objects += 1,
    }
    Ok(())
}

pub(crate) fn decode_rich_cell_storage(
    objects: &ObjectTable,
    images: &mut PageImages<'_>,
    st: &Message<'_>,
) -> Result<RichCellDecoded, EasyOfficeError> {
    let raw = st.text(storage::TEXT)?;
    let attachments = parse_entry_table(st, storage::ATTACHMENT_TABLE, "표 셀 첨부 표")?;
    let (link_runs, unsafe_links) = cell_link_runs(objects, st)?;

    let mut out = String::with_capacity(raw.len());
    let mut links: Vec<(u32, u32, String)> = Vec::new();
    let mut cell_images: Vec<(u64, Option<String>)> = Vec::new();
    let mut cell_objects = 0usize;

    let mut link_cursor = 0usize;
    let mut active_link: Option<(usize, String)> = None;
    let mut utf16 = 0u64;

    for ch in raw.chars() {
        let mut changed = false;
        let mut next_url: Option<String> = None;
        while link_cursor < link_runs.len() && link_runs[link_cursor].0 <= utf16 {
            next_url = link_runs[link_cursor].1.clone();
            link_cursor += 1;
            changed = true;
        }
        if changed && active_link.as_ref().map(|(_, u)| u) != next_url.as_ref() {
            if let Some((start_b, url)) = active_link.take() {
                if out.len() > start_b {
                    links.push((start_b as u32, out.len() as u32, url));
                }
            }
            if let Some(url) = next_url {
                active_link = Some((out.len(), url));
            }
        }
        match ch {
            '\u{2028}' => out.push(' '),
            '\n' | '\t' => out.push(ch),
            '\u{FFFC}' => match exact_attachment(&attachments, utf16) {
                Some(att_id) => resolve_cell_attachment(
                    objects,
                    images,
                    att_id,
                    &mut out,
                    &mut cell_images,
                    &mut cell_objects,
                )?,
                None => cell_objects += 1,
            },
            c if c.is_control() => {}
            c => out.push(c),
        }
        utf16 += ch.len_utf16() as u64;
    }
    if let Some((start_b, url)) = active_link.take() {
        if out.len() > start_b {
            links.push((start_b as u32, out.len() as u32, url));
        }
    }
    let trimmed_len = out.trim_end_matches('\n').len();
    out.truncate(trimmed_len);
    links.retain_mut(|(s, e, _)| {
        *e = (*e).min(trimmed_len as u32);
        *s < *e
    });

    Ok(RichCellDecoded {
        text: out,
        links,
        images: cell_images,
        cell_objects,
        unsafe_links,
    })
}

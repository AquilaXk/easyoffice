//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md), src/annotations.rs
//!
//! Pages tracked changes (`TSWP.ChangeArchive` 2060, `TSWP.ChangeSessionArchive` 2062,
//! `TSK.AnnotationAuthorArchive` 212): deleted spans are omitted from body paragraphs while
//! keeping UTF-16 style/link offsets intact, and body insertions/deletions are listed under
//! `## 변경 추적` in the appendix.

use super::marks::MAX_MARK_ENTRIES;
use super::{ends_paragraph, reference, Reader};
use crate::pages_proto::Message;
use crate::pages_schema::{change, storage, ty};
use crate::{EasyOfficeError, RichBlock, RichParagraph, RichRun};

/// Parses a run-anchored change table (`21` or `22`), enforcing `MAX_MARK_ENTRIES` fail-closed.
fn parse_change_entries(
    st: &Message<'_>,
    field: u32,
) -> Result<Vec<(u64, Option<u64>)>, EasyOfficeError> {
    let Some(table) = st.sub(field)? else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    table.for_each_sub(storage::TABLE_ENTRIES, |e| {
        if entries.len() >= MAX_MARK_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 변경 추적 표 항목 수 한도 초과".to_string(),
            ));
        }
        if let Some(idx) = e.varint(storage::ENTRY_INDEX) {
            entries.push((idx, reference(&e, storage::ENTRY_OBJECT)));
        }
        Ok(())
    })?;
    entries.sort_by_key(|(idx, _)| *idx);
    Ok(entries)
}

/// Maps every UTF-16 code-unit index `0..=total_u16` of `text` to a UTF-8 byte offset in O(N).
fn build_u16_byte_map(text: &str) -> Vec<usize> {
    let mut map = Vec::with_capacity(text.len() + 1);
    for (byte_idx, ch) in text.char_indices() {
        map.push(byte_idx);
        if ch.len_utf16() == 2 {
            map.push(byte_idx);
        }
    }
    map.push(text.len());
    map
}

/// UTF-16 index of the end of the paragraph containing `start_u16` (used when the final entry in
/// a change table has no explicit `(end, None)` terminator).
fn paragraph_end_u16(text: &str, u16_map: &[usize], start_u16: u64) -> u64 {
    let total_u16 = u16_map.len().saturating_sub(1) as u64;
    if start_u16 >= total_u16 {
        return total_u16;
    }
    let mut u16_pos = start_u16;
    let mut byte_idx = u16_map[start_u16 as usize];
    while !text.is_char_boundary(byte_idx) && byte_idx > 0 {
        byte_idx -= 1;
    }
    for ch in text[byte_idx..].chars() {
        if ends_paragraph(ch) {
            break;
        }
        u16_pos += ch.len_utf16() as u64;
    }
    u16_pos.min(total_u16)
}

fn slice_u16<'a>(text: &'a str, u16_map: &[usize], start: u64, end: u64) -> &'a str {
    let max_idx = u16_map.len().saturating_sub(1);
    let s = usize::try_from(start).unwrap_or(max_idx).min(max_idx);
    let e = usize::try_from(end).unwrap_or(max_idx).min(max_idx);
    if s >= e {
        return "";
    }
    let mut bs = u16_map[s];
    while !text.is_char_boundary(bs) && bs < text.len() {
        bs += 1;
    }
    let mut be = u16_map[e];
    while !text.is_char_boundary(be) && be < text.len() {
        be += 1;
    }
    if bs >= be {
        ""
    } else {
        &text[bs..be]
    }
}

fn clean_excerpt(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_space = false;
    for ch in raw.chars() {
        if ch == '\u{FFFC}' || ch == storage::FOOTNOTE_MARK_CHAR {
            continue;
        }
        if matches!(ch, '\n' | '\r' | '\t' | '\u{2028}' | ' ') {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else if !ch.is_control() {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

impl Reader<'_> {
    /// Returns `TSWP.ChangeArchive.kind` (`1` = insertion, `2` = deletion, or raw unknown value).
    pub(super) fn change_kind(&self, id: u64) -> Option<u64> {
        let obj = self.object(id, ty::CHANGE)?;
        let msg = Message::parse(&obj.payload).ok()?;
        msg.varint(change::KIND)
    }

    fn change_author(&self, change_id: u64) -> Option<String> {
        let chg_obj = self.object(change_id, ty::CHANGE)?;
        let chg_msg = Message::parse(&chg_obj.payload).ok()?;
        let sess_id = reference(&chg_msg, change::SESSION)?;
        let sess_obj = self.object(sess_id, ty::CHANGE_SESSION)?;
        let sess_msg = Message::parse(&sess_obj.payload).ok()?;
        let author_id = reference(&sess_msg, change::SESSION_AUTHOR)?;
        let author_obj = self
            .objects
            .get(author_id)
            .filter(|o| matches!(o.type_id, ty::AUTHOR | ty::DEPRECATED_AUTHOR))?;
        let author_msg = Message::parse(&author_obj.payload).ok()?;
        author_msg
            .string(change::AUTHOR_NAME)
            .ok()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Sorted, merged UTF-16 `[start, end)` intervals of tracked deletions (`DELETION_TABLE` 22,
    /// `kind == 2`). Also validates `INSERTION_TABLE` (21) against `MAX_MARK_ENTRIES`.
    pub(super) fn deleted_ranges(
        &self,
        st: &Message<'_>,
        text: &str,
    ) -> Result<Vec<(u64, u64)>, EasyOfficeError> {
        let _ = parse_change_entries(st, storage::INSERTION_TABLE)?;
        let entries = parse_change_entries(st, storage::DELETION_TABLE)?;
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let u16_map = build_u16_byte_map(text);
        let mut raw_ranges: Vec<(u64, u64)> = Vec::new();
        for (k, &(start, maybe_id)) in entries.iter().enumerate() {
            let Some(chg_id) = maybe_id else {
                continue;
            };
            if self.change_kind(chg_id) != Some(change::KIND_DELETION) {
                continue;
            }
            let end = if k + 1 < entries.len() {
                entries[k + 1].0
            } else {
                paragraph_end_u16(text, &u16_map, start)
            };
            if end > start {
                raw_ranges.push((start, end));
            }
        }
        raw_ranges.sort_unstable_by_key(|&(s, _)| s);
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(raw_ranges.len());
        for (s, e) in raw_ranges {
            if let Some(last) = merged.last_mut() {
                if s <= last.1 {
                    last.1 = last.1.max(e);
                    continue;
                }
            }
            merged.push((s, e));
        }
        Ok(merged)
    }

    /// Appendix paragraphs for `## 변경 추적`, ordered by UTF-16 position in the body storage.
    pub(super) fn tracked_change_blocks(
        &mut self,
        body: &Message<'_>,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let ins_entries = parse_change_entries(body, storage::INSERTION_TABLE)?;
        let del_entries = parse_change_entries(body, storage::DELETION_TABLE)?;
        if ins_entries.is_empty() && del_entries.is_empty() {
            return Ok(Vec::new());
        }
        let text = body.text(storage::TEXT)?;
        let u16_map = build_u16_byte_map(&text);
        let mut spans: Vec<(u64, u8, u64, u64)> = Vec::new();

        for (table_order, entries) in [(0u8, &ins_entries), (1u8, &del_entries)] {
            for (k, &(start, maybe_id)) in entries.iter().enumerate() {
                let Some(chg_id) = maybe_id else {
                    continue;
                };
                if !matches!(
                    self.change_kind(chg_id),
                    Some(change::KIND_INSERTION | change::KIND_DELETION)
                ) {
                    continue;
                }
                let end = if k + 1 < entries.len() {
                    entries[k + 1].0
                } else {
                    paragraph_end_u16(&text, &u16_map, start)
                };
                if end > start {
                    spans.push((start, table_order, end, chg_id));
                }
            }
        }
        spans.sort_by_key(|&(start, order, _, _)| (start, order));

        let mut out = Vec::new();
        for (start, _, end, chg_id) in spans {
            let raw = slice_u16(&text, &u16_map, start, end);
            // Spans may overlap, so the scan is charged even when the excerpt turns out empty.
            self.text_budget.charge(raw.len(), "Pages 변경 추적 범위")?;
            let excerpt = clean_excerpt(raw);
            if excerpt.is_empty() {
                continue;
            }
            let kind_label = match self.change_kind(chg_id) {
                Some(change::KIND_DELETION) => "삭제",
                _ => "삽입",
            };
            let line = match self.change_author(chg_id) {
                Some(author) => format!("[{kind_label} · {author}] {excerpt}"),
                None => format!("[{kind_label}] {excerpt}"),
            };
            self.text_budget
                .charge_block(line.len(), "Pages 변경 추적 부록")?;
            self.count_run()?;
            out.push(RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(line)],
                ..RichParagraph::default()
            }));
        }
        Ok(out)
    }
}

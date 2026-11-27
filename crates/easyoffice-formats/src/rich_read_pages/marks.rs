//! Marks inside a text storage: hyperlinks, footnote marks (with continuous, per-section and
//! per-page numbering) and tracked changes are imported, comments are listed in an appendix, and
//! what the rich model cannot carry (bookmarks, date/merge/placeholder/ruby fields) is counted
//! and reported as warnings instead of being dropped silently.

use super::numbering::{list_number, note_label};
use super::{
    ends_paragraph, reference, AttrTable, Flow, Reader, PAGE_BREAK, SECTION_BREAK, TWIPS_PER_POINT,
};
use crate::pages_proto::Message;
use crate::pages_schema::{change, hyperlink, settings, storage, style, ty};
use crate::pdf_text_wrap::wrap_spans;
use crate::{EasyOfficeError, RichLineSpacing, RichListNumber};

/// Height of one text line in points, for the page estimate that numbers per-page footnotes.
fn line_height_pt(size_pt: f32, spacing: Option<RichLineSpacing>) -> f32 {
    let single = size_pt * DEFAULT_LINE_HEIGHT_FACTOR;
    let pt = |twips: u16| f32::from(twips) / TWIPS_PER_POINT;
    let height = match spacing {
        None => single,
        Some(RichLineSpacing::Multiple(pct)) => (f32::from(pct) / 100.0) * single,
        Some(RichLineSpacing::AtLeast(twips)) => single.max(pt(twips)),
        Some(RichLineSpacing::Exact(twips)) => pt(twips),
        Some(RichLineSpacing::Between(twips)) => single + pt(twips),
    };
    height.max(1.0)
}

pub(super) const MAX_MARK_ENTRIES: usize = 50_000;
const MAX_COMMENTS: usize = 50_000;
const DEFAULT_FONT_SIZE_PT: f32 = 12.0;
const DEFAULT_LINE_HEIGHT_FACTOR: f32 = 1.2;
const MIN_WRAP_WIDTH_PT: f32 = 36.0;
const MIN_BODY_HEIGHT_PT: f32 = 72.0;

/// Warning kinds, one per mark family (reported as `kind N개`).
pub(super) const UNSAFE_LINK: &str = "허용되지 않는 형식의 하이퍼링크 주소(글자만 가져옴)";
const UNREADABLE_COMMENT: &str = "읽을 수 없는 댓글";
const DATE_FIELD: &str = "날짜·시각 필드(저장된 글자만 가져옴)";
const BOOKMARK: &str = "책갈피";
const CHANGE: &str = "변경 추적";
const OTHER_FIELD: &str = "글 속 필드(자리표시자·병합·루비)";
const NESTED_COMMENT: &str = "도형·각주·머리말·꼬리말 안의 댓글";
const NESTED_FOOTNOTE: &str = "도형·각주·머리말·꼬리말 안의 각주";
const UNKNOWN_LIST_FORMAT: &str = "알 수 없는 목록 번호 형식(1. 형식으로 가져옴)";

/// Entries `(character index, object id)` of an object attribute table that point at an object.
/// Malformed tables and entries are ignored, while entry caps fail closed.
fn table_objects(st: &Message<'_>, field: u32) -> Result<Vec<(u64, u64)>, EasyOfficeError> {
    let Some(table) = st.sub(field).ok().flatten() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let res = table.for_each_sub(storage::TABLE_ENTRIES, |e| {
        if out.len() >= MAX_MARK_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표식 테이블 항목 수 한도 초과".to_string(),
            ));
        }
        if let (Some(idx), Some(obj)) = (
            e.varint(storage::ENTRY_INDEX),
            reference(&e, storage::ENTRY_OBJECT),
        ) {
            out.push((idx, obj));
        }
        Ok(())
    });
    if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = res {
        return Err(err);
    }
    Ok(out)
}

const fn is_comment_anchor(type_id: u32) -> bool {
    matches!(
        type_id,
        ty::COMMENT_FIELD | ty::HIGHLIGHT | ty::COMMENT_INFO
    )
}

fn is_in_ranges(ranges: &[(u64, u64)], idx: u64) -> bool {
    let pos = ranges.partition_point(|&(s, _)| s <= idx);
    pos.checked_sub(1).is_some_and(|k| idx < ranges[k].1)
}

/// A footnote mark of a storage with the number shown at it.
#[derive(Debug, Clone, Copy)]
pub(super) struct FootnoteMark {
    pub index: u64,
    pub id: u64,
    pub number: usize,
}

const fn is_footnote(type_id: u32) -> bool {
    type_id == ty::FOOTNOTE || type_id == ty::FOOTNOTE_ATTACHMENT
}

impl Reader<'_> {
    /// Assigns 1-based per-page footnote numbers by walking explicit page/section breaks
    /// (`\u{000C}`, `\u{0004}`) and wrapped visual line heights (`pdf_text_wrap::wrap_spans`
    /// against `PageSetup` body width/height and active `RichColumns`).
    fn number_footnotes_per_page(
        &mut self,
        st: &Message<'_>,
        marks: &[(u64, u64)],
    ) -> Result<Vec<FootnoteMark>, EasyOfficeError> {
        let text = st.text(storage::TEXT)?;
        let para_styles = AttrTable::parse(st, storage::PARA_STYLE_TABLE)?;
        let char_styles = AttrTable::parse(st, storage::CHAR_STYLE_TABLE)?;
        let layout_styles = AttrTable::parse(st, storage::LAYOUT_STYLE_TABLE)?;

        let mut out = Vec::with_capacity(marks.len());
        let mut mark_cursor = 0usize;
        let mut current_page = 0usize;
        let mut mark_page_seen: Option<usize> = None;
        let mut number = 0usize;
        let mut used_height_pt = 0.0f32;
        let mut u16_pos = 0u64;
        let mut byte_offset = 0usize;

        let mut emit_until = |limit_u16: u64, page: usize| {
            while mark_cursor < marks.len() && marks[mark_cursor].0 < limit_u16 {
                if mark_page_seen != Some(page) {
                    mark_page_seen = Some(page);
                    number = 0;
                }
                number += 1;
                let (index, id) = marks[mark_cursor];
                out.push(FootnoteMark { index, id, number });
                mark_cursor += 1;
            }
        };

        for line in text.split(ends_paragraph) {
            let term = text[byte_offset + line.len()..].chars().next();
            byte_offset += line.len() + term.map_or(0, char::len_utf8);

            let para_start = u16_pos;
            let pfmt = para_styles.at(para_start).map(|id| self.style_fmt(id));
            let cfmt = char_styles.at(para_start).map(|id| self.style_fmt(id));
            let cols = layout_styles
                .at(para_start)
                .and_then(|id| self.resolve_columns(id));
            let col_count = cols.map_or(1u32, |c| u32::from(c.count.max(1)));
            let gap_pt = cols.map_or(0.0f32, |c| f32::from(c.gap_twips) / TWIPS_PER_POINT);
            let indent_pt = pfmt
                .as_ref()
                .and_then(|f| f.left_indent_twips)
                .map_or(0.0f32, |tw| tw as f32 / TWIPS_PER_POINT);
            let col_width_pt = ((self.body_width_pt
                - gap_pt * (col_count.saturating_sub(1) as f32))
                / (col_count as f32)
                - indent_pt)
                .max(MIN_WRAP_WIDTH_PT);

            let size_pt = cfmt
                .as_ref()
                .and_then(|f| f.size_half_pt)
                .or_else(|| pfmt.as_ref().and_then(|f| f.size_half_pt))
                .map_or(DEFAULT_FONT_SIZE_PT, |hp| f32::from(hp) * 0.5);
            let line_h_pt = line_height_pt(size_pt, pfmt.as_ref().and_then(|f| f.line_spacing));
            let page_cap_pt = self.body_height_pt.max(MIN_BODY_HEIGHT_PT) * (col_count as f32);
            let max_units = ((col_width_pt / size_pt) * 1000.0).round().max(1000.0) as u32;

            let spans = wrap_spans(line, max_units);
            let mut chars = line.char_indices().peekable();
            let mut cur_u16 = para_start;

            for (span_idx, _) in spans.iter().enumerate() {
                if used_height_pt + line_h_pt > page_cap_pt && used_height_pt > 0.0 {
                    current_page += 1;
                    used_height_pt = line_h_pt;
                } else {
                    used_height_pt += line_h_pt;
                }
                let boundary_byte = spans
                    .get(span_idx + 1)
                    .map_or(line.len(), |next| next.start);
                while let Some(&(_, ch)) = chars.peek().filter(|&&(b, _)| b < boundary_byte) {
                    cur_u16 += ch.len_utf16() as u64;
                    chars.next();
                }
                emit_until(cur_u16, current_page);
            }
            for (_, ch) in chars {
                cur_u16 += ch.len_utf16() as u64;
            }
            let next_para_start = cur_u16.saturating_add(u64::from(term.is_some()));
            emit_until(next_para_start, current_page);
            u16_pos = next_para_start;

            if matches!(term, Some(PAGE_BREAK | SECTION_BREAK)) {
                current_page += 1;
                used_height_pt = 0.0;
            }
        }
        emit_until(u64::MAX, current_page);
        Ok(out)
    }

    /// Footnote marks of a storage in text order with the number shown at each. Marks inside
    /// tracked deletions are skipped.
    pub(super) fn footnote_marks(
        &mut self,
        st: &Message<'_>,
        deleted_ranges: &[(u64, u64)],
    ) -> Result<Vec<FootnoteMark>, EasyOfficeError> {
        let mut marks: Vec<(u64, u64)> = Vec::new();
        for field in [storage::FOOTNOTE_TABLE, storage::FOOTNOTE_TABLE_V2] {
            for (idx, id) in table_objects(st, field)? {
                if is_in_ranges(deleted_ranges, idx) {
                    continue;
                }
                if self.objects.get(id).is_some_and(|o| is_footnote(o.type_id)) {
                    if marks.len() >= MAX_MARK_ENTRIES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Pages 각주 표식 수 한도 초과".to_string(),
                        ));
                    }
                    marks.push((idx, id));
                }
            }
        }
        marks.sort_unstable_by_key(|(i, _)| *i);
        marks.dedup_by_key(|(i, _)| *i);
        if marks.is_empty() {
            return Ok(Vec::new());
        }
        if self.settings.notes.numbering == settings::NUMBERING_PER_PAGE {
            return self.number_footnotes_per_page(st, &marks);
        }
        let restart_at_sections = self.settings.notes.numbering == settings::NUMBERING_PER_SECTION;
        let mut sections: Vec<u64> = if restart_at_sections {
            table_objects(st, storage::SECTION_TABLE)?
                .into_iter()
                .map(|(i, _)| i)
                .collect()
        } else {
            Vec::new()
        };
        sections.sort_unstable();
        let mut out = Vec::with_capacity(marks.len());
        let (mut number, mut section) = (0usize, 0usize);
        for (index, id) in marks {
            let current = sections.partition_point(|s| *s <= index);
            if current != section {
                section = current;
                number = 0;
            }
            number += 1;
            out.push(FootnoteMark { index, id, number });
        }
        Ok(out)
    }

    /// Text shown at a footnote mark and in front of its note.
    pub(super) fn note_label(&self, number: usize) -> String {
        note_label(self.settings.notes.format, number)
    }

    /// Hyperlink run table of a storage: the value in effect at a character is the hyperlink
    /// object there, `None` where an entry ends the link. Only addresses of a known safe kind are
    /// kept; the others are counted.
    pub(super) fn link_table(&mut self, st: &Message<'_>) -> Result<AttrTable, EasyOfficeError> {
        let mut best: Vec<(u64, Option<u64>)> = Vec::new();
        for field in [storage::SMARTFIELD_TABLE, storage::LANGUAGE_OR_FIELD_TABLE] {
            let Some(table) = st.sub(field).ok().flatten() else {
                continue;
            };
            let mut entries: Vec<(u64, Option<u64>)> = Vec::new();
            let res = table.for_each_sub(storage::TABLE_ENTRIES, |e| {
                if entries.len() >= MAX_MARK_ENTRIES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 하이퍼링크 표 항목 수 한도 초과".to_string(),
                    ));
                }
                if let Some(index) = e.varint(storage::ENTRY_INDEX) {
                    entries.push((index, reference(&e, storage::ENTRY_OBJECT)));
                }
                Ok(())
            });
            if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = res {
                return Err(err);
            }
            let has_link = entries.iter().any(|(_, id)| {
                id.is_some_and(|id| {
                    self.objects
                        .get(id)
                        .is_some_and(|o| o.type_id == ty::HYPERLINK)
                })
            });
            if has_link {
                best = entries;
                break;
            }
        }
        let mut rejected = 0usize;
        let mut entries = Vec::with_capacity(best.len());
        for (index, id) in best {
            let link = id.filter(|id| {
                self.objects
                    .get(*id)
                    .is_some_and(|o| o.type_id == ty::HYPERLINK)
            });
            let kept = link.filter(|id| self.link_url(*id).is_some());
            rejected += usize::from(link.is_some() && kept.is_none());
            if entries
                .last()
                .map_or(kept.is_some(), |(_, last)| *last != kept)
            {
                entries.push((index, kept));
            }
        }
        self.skip_many(UNSAFE_LINK, rejected);
        Ok(AttrTable::from_entries(entries))
    }

    /// Target of a hyperlink object when it is a plain web, mail, phone or ftp address.
    pub(super) fn link_url(&self, id: u64) -> Option<String> {
        let url = self
            .object(id, ty::HYPERLINK)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| m.string(hyperlink::URL).ok().flatten())?;
        let url = url.trim();
        let lower = url.to_ascii_lowercase();
        let allowed = hyperlink::ALLOWED_SCHEMES
            .iter()
            .any(|s| lower.starts_with(s));
        (allowed && url.len() <= hyperlink::MAX_URL_BYTES && !url.chars().any(char::is_control))
            .then(|| url.to_string())
    }

    /// Comment texts of the main flow in mark order; unreadable ones are counted.
    pub(super) fn comment_texts(
        &mut self,
        st: &Message<'_>,
    ) -> Result<Vec<String>, EasyOfficeError> {
        let mut found: Vec<(u64, u64)> = Vec::new();
        for table in storage::MARK_TABLES {
            for (idx, id) in table_objects(st, table)? {
                if self
                    .objects
                    .get(id)
                    .is_some_and(|o| is_comment_anchor(o.type_id))
                {
                    if found.len() >= MAX_COMMENTS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Pages 댓글 수 한도 초과".to_string(),
                        ));
                    }
                    found.push((idx, id));
                }
            }
        }
        found.sort_unstable_by_key(|(i, _)| *i);
        found.dedup_by_key(|(i, _)| *i);
        let mut out = Vec::new();
        let mut unreadable = 0usize;
        // Anchors may name one comment text over and over: it is read once, each use is still charged.
        let mut read_texts: std::collections::HashMap<u64, Option<String>> =
            std::collections::HashMap::new();
        for (_, id) in found {
            let text = self
                .objects
                .get(id)
                .and_then(|o| Message::parse(&o.payload).ok())
                .and_then(|m| reference(&m, 1))
                .and_then(|cid| {
                    read_texts
                        .entry(cid)
                        .or_insert_with(|| self.comment_text_of(cid))
                        .clone()
                });
            match text {
                Some(t) => {
                    self.text_budget
                        .charge_block(t.len(), "Pages 댓글 텍스트")?;
                    out.push(t);
                }
                None => unreadable += 1,
            }
        }
        self.skip_many(UNREADABLE_COMMENT, unreadable);
        Ok(out)
    }

    /// Trimmed text of the comment text object `cid`, when it is one and holds any.
    fn comment_text_of(&self, cid: u64) -> Option<String> {
        self.objects
            .get(cid)
            .filter(|o| o.type_id == ty::COMMENT_TEXT || o.type_id == ty::COMMENT_TEXT_V2)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| m.string(1).ok().flatten())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    }

    fn mark_kind(&self, table: u32, id: u64, type_id: u32, flow: Flow) -> Option<String> {
        let kind = match type_id {
            ty::HYPERLINK => return None,
            ty::DATE_FIELD => DATE_FIELD,
            ty::BOOKMARK => BOOKMARK,
            t if is_comment_anchor(t) => return None,
            ty::CHANGE
                if flow == Flow::Body
                    && matches!(table, storage::INSERTION_TABLE | storage::DELETION_TABLE)
                    && matches!(
                        self.change_kind(id),
                        Some(change::KIND_INSERTION | change::KIND_DELETION)
                    ) =>
            {
                return None
            }
            ty::CHANGE => CHANGE,
            ty::PLACEHOLDER_FIELD | ty::MERGE_FIELD | ty::RUBY_FIELD => OTHER_FIELD,
            t if is_footnote(t) => return None,
            t => return Some(format!("글 속 표시(형식 {t})")),
        };
        Some(kind.to_string())
    }

    /// Counts the marks of one storage that are not imported. Footnote marks of nested text are
    /// counted too, since only the main flow numbers and lists footnotes.
    pub(super) fn note_marks(
        &mut self,
        st: &Message<'_>,
        flow: Flow,
    ) -> Result<(), EasyOfficeError> {
        for table in storage::MARK_TABLES {
            for (_, id) in table_objects(st, table)? {
                let Some(type_id) = self.objects.get(id).map(|o| o.type_id) else {
                    continue;
                };
                if is_footnote(type_id) {
                    if flow == Flow::Nested {
                        self.skip(NESTED_FOOTNOTE);
                    }
                } else if is_comment_anchor(type_id) {
                    if flow == Flow::Nested {
                        self.skip(NESTED_COMMENT);
                    }
                } else if let Some(kind) = self.mark_kind(table, id, type_id, flow) {
                    self.skip(kind);
                }
            }
        }
        Ok(())
    }

    /// Number format of an ordered list paragraph at `level`; `None` for the plain `1.` and,
    /// with a warning, for a format value this reader does not know.
    pub(super) fn list_number_of(&mut self, list_style: u64, level: u64) -> Option<RichListNumber> {
        let value = self
            .object(list_style, ty::LIST_STYLE)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| m.repeated_varints(style::LIST_LEVEL_NUMBER_FORMATS).ok())
            .and_then(|formats| {
                let at = usize::try_from(level)
                    .ok()?
                    .min(formats.len().checked_sub(1)?);
                formats.get(at).copied()
            })?;
        match list_number(value) {
            Some(n) if n == RichListNumber::default() => None,
            Some(n) => Some(n),
            None => {
                self.skip(UNKNOWN_LIST_FORMAT);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_budget::DecodeBudget;
    use crate::pages_proto::{ArchiveObject, ObjectTable};
    use crate::pages_table::TableBudget;
    use crate::rich_read_pages::numbering::DocumentSettings;
    use crate::rich_read_pages::sections::{DEFAULT_BODY_HEIGHT_PT, DEFAULT_BODY_WIDTH_PT};
    use crate::rich_read_pages_image::PageImages;
    use std::collections::{BTreeMap, HashMap};

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    fn f_varint(num: u32, v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        varint(u64::from(num) << 3, &mut out);
        varint(v, &mut out);
        out
    }

    fn f_bytes(num: u32, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint((u64::from(num) << 3) | 2, &mut out);
        varint(bytes.len() as u64, &mut out);
        out.extend_from_slice(bytes);
        out
    }

    fn f_ref(num: u32, id: u64) -> Vec<u8> {
        f_bytes(num, &f_varint(1, id))
    }

    #[test]
    fn table_objects_rejects_entries_beyond_cap() {
        let entry = [
            f_varint(storage::ENTRY_INDEX, 0),
            f_ref(storage::ENTRY_OBJECT, 10),
        ]
        .concat();
        let packed_entry = f_bytes(storage::TABLE_ENTRIES, &entry);
        let mut table_bytes = Vec::with_capacity(packed_entry.len() * (MAX_MARK_ENTRIES + 1));
        for _ in 0..=MAX_MARK_ENTRIES {
            table_bytes.extend_from_slice(&packed_entry);
        }
        let st_bytes = f_bytes(storage::FOOTNOTE_TABLE, &table_bytes);
        let msg = Message::parse(&st_bytes).unwrap();
        assert!(matches!(
            table_objects(&msg, storage::FOOTNOTE_TABLE),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }

    #[test]
    fn comment_texts_charge_decode_budget_and_fail_closed() {
        let mut objects = ObjectTable::default();
        objects.objects.insert(
            1,
            ArchiveObject {
                type_id: ty::COMMENT_TEXT,
                payload: f_bytes(1, &"C".repeat(16 * 1024).into_bytes()).into(),
                extra: Vec::new(),
            },
        );
        objects.objects.insert(
            2,
            ArchiveObject {
                type_id: ty::COMMENT_FIELD,
                payload: f_ref(1, 1).into(),
                extra: Vec::new(),
            },
        );
        let mut table_bytes = Vec::new();
        for i in 0..10u64 {
            let entry = [
                f_varint(storage::ENTRY_INDEX, i),
                f_ref(storage::ENTRY_OBJECT, 2),
            ]
            .concat();
            table_bytes.extend_from_slice(&f_bytes(storage::TABLE_ENTRIES, &entry));
        }
        let st_bytes = f_bytes(storage::MARK_TABLES[0], &table_bytes);
        let st_msg = Message::parse(&st_bytes).unwrap();
        let data_files = HashMap::new();
        let mut reader = Reader {
            objects: &objects,
            skipped: BTreeMap::new(),
            paragraphs: 0,
            runs: 0,
            tables: TableBudget::default(),
            fmt_cache: crate::pages_proto::FastU64Map::default(),
            nested_reads: std::collections::HashMap::new(),
            images: PageImages::new(&objects, &data_files),
            quiet: false,
            text_budget: DecodeBudget::with_limit(64 * 1024),
            settings: DocumentSettings::default(),
            body_width_pt: DEFAULT_BODY_WIDTH_PT,
            body_height_pt: DEFAULT_BODY_HEIGHT_PT,
        };
        assert!(matches!(
            reader.comment_texts(&st_msg),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}

//! Marks inside a text storage: hyperlinks and footnote marks are imported, comments are listed
//! in an appendix, and what the rich model cannot carry (bookmarks, tracked changes, fields,
//! multi-column layouts) is counted and reported as warnings instead of being dropped silently.
//! The tables that hold these marks differ between file versions, so entries are classified by
//! the type of the object they point at.

use super::numbering::list_number;
use super::numbering::note_label;
use super::{reference, AttrTable, Flow, Reader};
use crate::pages_proto::Message;
use crate::pages_schema::{column, hyperlink, settings, storage, style, ty};
use crate::{EasyOfficeError, RichListNumber};

const MAX_MARK_ENTRIES: usize = 50_000;
const MAX_COMMENTS: usize = 50_000;

/// Warning kinds, one per mark family (reported as `kind N개`).
const UNSAFE_LINK: &str = "허용되지 않는 형식의 하이퍼링크 주소(글자만 가져옴)";
const UNREADABLE_COMMENT: &str = "읽을 수 없는 댓글";
const PER_PAGE_RESTART: &str = "쪽마다 다시 시작하는 각주 번호(연속 번호로 표시)";
const DATE_FIELD: &str = "날짜·시각 필드(저장된 글자만 가져옴)";
const BOOKMARK: &str = "책갈피";
const INSERTION: &str = "변경 추적 삽입 표시(글자는 그대로 가져옴)";
const DELETION: &str = "변경 추적 삭제 표시(삭제된 글자도 그대로 가져옴)";
const CHANGE: &str = "변경 추적";
const OTHER_FIELD: &str = "글 속 필드(자리표시자·병합·루비)";
const NESTED_COMMENT: &str = "도형·각주·머리글·바닥글 안의 댓글";
const NESTED_FOOTNOTE: &str = "도형·각주·머리글·바닥글 안의 각주";
const UNKNOWN_LIST_FORMAT: &str = "알 수 없는 목록 번호 형식(1. 형식으로 가져옴)";
const MULTI_COLUMN: &str = "다단 문단 구간(단 나누기 없이 한 단으로 가져옴)";

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
    /// Footnote marks of a storage in text order with the number shown at each. Older files keep
    /// them in one table and newer ones in another; entries are told apart from hyperlinks and
    /// other fields by the type of the object they point at.
    pub(super) fn footnote_marks(
        &self,
        st: &Message<'_>,
    ) -> Result<Vec<FootnoteMark>, EasyOfficeError> {
        let mut marks: Vec<(u64, u64)> = Vec::new();
        for field in [storage::FOOTNOTE_TABLE, storage::FOOTNOTE_TABLE_V2] {
            for (idx, id) in table_objects(st, field)? {
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
            // Entries that do not change the link (rejected addresses, other fields) must not
            // split runs.
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
        for (_, id) in found {
            let text = self
                .objects
                .get(id)
                .and_then(|o| Message::parse(&o.payload).ok())
                .and_then(|m| reference(&m, 1))
                .and_then(|cid| self.objects.get(cid))
                .filter(|o| o.type_id == ty::COMMENT_TEXT || o.type_id == ty::COMMENT_TEXT_V2)
                .and_then(|o| Message::parse(&o.payload).ok())
                .and_then(|m| m.string(1).ok().flatten())
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
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

    fn mark_kind(table: u32, type_id: u32) -> Option<String> {
        let kind = match type_id {
            ty::HYPERLINK => return None,
            ty::DATE_FIELD => DATE_FIELD,
            ty::BOOKMARK => BOOKMARK,
            t if is_comment_anchor(t) => return None,
            ty::CHANGE if table == storage::INSERTION_TABLE => INSERTION,
            ty::CHANGE if table == storage::DELETION_TABLE => DELETION,
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
                } else if let Some(kind) = Self::mark_kind(table, type_id) {
                    self.skip(kind);
                }
            }
        }
        Ok(())
    }

    /// Columns of a column style, following its parent chain to the first style that sets them.
    fn column_count(&self, style_id: u64) -> u64 {
        let chain = self.objects.chain(style_id, super::MAX_STYLE_CHAIN, |o| {
            let m = Message::parse(&o.payload).ok()?;
            let header = m.sub(style::HEADER).ok().flatten()?;
            reference(&header, style::HEADER_PARENT)
        });
        for id in chain {
            let Some(columns) = self
                .object(id, ty::COLUMN_STYLE)
                .and_then(|o| Message::parse(&o.payload).ok())
                .and_then(|m| m.sub(column::PROPS).ok().flatten())
                .and_then(|p| p.sub(column::COLUMNS).ok().flatten())
            else {
                continue;
            };
            if let Ok(Some(equal)) = columns.sub(column::EQUAL) {
                return equal.varint(column::EQUAL_COUNT).unwrap_or(1);
            }
            if let Ok(Some(unequal)) = columns.sub(column::UNEQUAL) {
                let following = unequal
                    .subs(column::UNEQUAL_FOLLOWING)
                    .map_or(0, |v| v.len() as u64);
                return following.saturating_add(1);
            }
        }
        1
    }

    /// Reports a per-page restart rule, which cannot be applied without page layout.
    pub(super) fn note_numbering_rule(&mut self, any_footnotes: bool) {
        if any_footnotes && self.settings.notes.numbering == settings::NUMBERING_PER_PAGE {
            self.skip(PER_PAGE_RESTART);
        }
    }

    /// Counts paragraph ranges of the body that are set in more than one column.
    pub(super) fn note_columns(&mut self, st: &Message<'_>) -> Result<(), EasyOfficeError> {
        let multi = table_objects(st, storage::LAYOUT_STYLE_TABLE)?
            .into_iter()
            .filter(|(_, id)| self.column_count(*id) > 1)
            .count();
        self.skip_many(MULTI_COLUMN, multi);
        Ok(())
    }
}

impl Reader<'_> {
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
                payload: f_bytes(1, &"C".repeat(16 * 1024).into_bytes()),
                extra: Vec::new(),
            },
        );
        objects.objects.insert(
            2,
            ArchiveObject {
                type_id: ty::COMMENT_FIELD,
                payload: f_ref(1, 1),
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
            images: PageImages::new(&objects, &data_files),
            quiet: false,
            text_budget: DecodeBudget::with_limit(64 * 1024),
            settings: DocumentSettings::default(),
        };
        assert!(matches!(
            reader.comment_texts(&st_msg),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}

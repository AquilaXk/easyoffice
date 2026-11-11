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
use crate::RichListNumber;

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
/// Malformed tables and entries are ignored: the marks only feed warnings.
fn table_objects(st: &Message<'_>, field: u32) -> Vec<(u64, u64)> {
    let Some(table) = st.sub(field).ok().flatten() else {
        return Vec::new();
    };
    table
        .subs(storage::TABLE_ENTRIES)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            Some((
                e.varint(storage::ENTRY_INDEX)?,
                reference(e, storage::ENTRY_OBJECT)?,
            ))
        })
        .collect()
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
    pub(super) fn footnote_marks(&self, st: &Message<'_>) -> Vec<FootnoteMark> {
        let mut marks: Vec<(u64, u64)> = [storage::FOOTNOTE_TABLE, storage::FOOTNOTE_TABLE_V2]
            .into_iter()
            .flat_map(|field| table_objects(st, field))
            .filter(|(_, id)| {
                self.objects
                    .get(*id)
                    .is_some_and(|o| is_footnote(o.type_id))
            })
            .collect();
        marks.sort_unstable_by_key(|(i, _)| *i);
        marks.dedup_by_key(|(i, _)| *i);
        let restart_at_sections = self.settings.notes.numbering == settings::NUMBERING_PER_SECTION;
        let mut sections: Vec<u64> = if restart_at_sections {
            table_objects(st, storage::SECTION_TABLE)
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
        out
    }

    /// Text shown at a footnote mark and in front of its note.
    pub(super) fn note_label(&self, number: usize) -> String {
        note_label(self.settings.notes.format, number)
    }

    /// Hyperlink run table of a storage: the value in effect at a character is the hyperlink
    /// object there, `None` where an entry ends the link. Only addresses of a known safe kind are
    /// kept; the others are counted.
    pub(super) fn link_table(&mut self, st: &Message<'_>) -> AttrTable {
        let mut best: Vec<(u64, Option<u64>)> = Vec::new();
        for field in [storage::SMARTFIELD_TABLE, storage::LANGUAGE_OR_FIELD_TABLE] {
            let Some(table) = st.sub(field).ok().flatten() else {
                continue;
            };
            let entries: Vec<(u64, Option<u64>)> = table
                .subs(storage::TABLE_ENTRIES)
                .unwrap_or_default()
                .iter()
                .filter_map(|e| {
                    Some((
                        e.varint(storage::ENTRY_INDEX)?,
                        reference(e, storage::ENTRY_OBJECT),
                    ))
                })
                .collect();
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
        AttrTable::from_entries(entries)
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
    pub(super) fn comment_texts(&mut self, st: &Message<'_>) -> Vec<String> {
        let mut found: Vec<(u64, u64)> = storage::MARK_TABLES
            .into_iter()
            .flat_map(|table| table_objects(st, table))
            .filter(|(_, id)| {
                self.objects
                    .get(*id)
                    .is_some_and(|o| is_comment_anchor(o.type_id))
            })
            .collect();
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
                Some(t) => out.push(t),
                None => unreadable += 1,
            }
        }
        self.skip_many(UNREADABLE_COMMENT, unreadable);
        out
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
    pub(super) fn note_marks(&mut self, st: &Message<'_>, flow: Flow) {
        for table in storage::MARK_TABLES {
            for (_, id) in table_objects(st, table) {
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
    pub(super) fn note_columns(&mut self, st: &Message<'_>) {
        let multi = table_objects(st, storage::LAYOUT_STYLE_TABLE)
            .into_iter()
            .filter(|(_, id)| self.column_count(*id) > 1)
            .count();
        self.skip_many(MULTI_COLUMN, multi);
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

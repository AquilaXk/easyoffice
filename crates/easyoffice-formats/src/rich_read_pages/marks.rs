//! Marks inside a text storage that the rich model cannot carry: hyperlink addresses, bookmarks,
//! tracked changes, comments, fields and multi-column layouts. They are counted by the type of
//! the object they point at and reported as warnings instead of being dropped silently. Footnote
//! marks are collected here as well, because the same tables hold them in different file
//! versions.

use super::{reference, Flow, Reader};
use crate::pages_proto::Message;
use crate::pages_schema::{column, storage, style, ty};

/// Warning kinds, one per mark family (reported as `kind N개`).
const HYPERLINK: &str = "하이퍼링크의 주소(글자만 가져옴)";
const DATE_FIELD: &str = "날짜·시각 필드(저장된 글자만 가져옴)";
const BOOKMARK: &str = "책갈피";
const COMMENT: &str = "댓글";
const INSERTION: &str = "변경 추적 삽입 표시(글자는 그대로 가져옴)";
const DELETION: &str = "변경 추적 삭제 표시(삭제된 글자도 그대로 가져옴)";
const CHANGE: &str = "변경 추적";
const OTHER_FIELD: &str = "글 속 필드(자리표시자·병합·루비)";
const NESTED_FOOTNOTE: &str = "도형·각주·머리글·바닥글 안의 각주";
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

const fn is_footnote(type_id: u32) -> bool {
    type_id == ty::FOOTNOTE || type_id == ty::FOOTNOTE_ATTACHMENT
}

impl Reader<'_> {
    /// Footnote marks `(character index, footnote id)` of a storage in text order. Older files
    /// keep them in one table and newer ones in another; entries are told apart from hyperlinks
    /// and other fields by the type of the object they point at.
    pub(super) fn footnote_marks(&self, st: &Message<'_>) -> Vec<(u64, u64)> {
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
        marks
    }

    fn mark_kind(table: u32, type_id: u32) -> Option<String> {
        let kind = match type_id {
            ty::HYPERLINK => HYPERLINK,
            ty::DATE_FIELD => DATE_FIELD,
            ty::BOOKMARK => BOOKMARK,
            ty::COMMENT_FIELD | ty::HIGHLIGHT | ty::COMMENT_INFO => COMMENT,
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

    /// Counts paragraph ranges of the body that are set in more than one column.
    pub(super) fn note_columns(&mut self, st: &Message<'_>) {
        let multi = table_objects(st, storage::LAYOUT_STYLE_TABLE)
            .into_iter()
            .filter(|(_, id)| self.column_count(*id) > 1)
            .count();
        self.skip_many(MULTI_COLUMN, multi);
    }
}

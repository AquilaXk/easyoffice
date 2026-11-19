//! Document-level notes (footnotes and endnotes), comments, and tracked-change revisions (PD-252).
//!
//! Defines the structured model types and Fail-Closed resource bounds shared by the OOXML (DOCX),
//! ODF (ODT), HWPX, Markdown, TXT, PDF, RTF, and Pages codecs as well as the IPC layer.

use std::collections::{HashMap, HashSet};

use crate::{RichBlock, RichDocument, RichRun};

/// Maximum footnotes plus endnotes accepted or emitted for one document.
pub const MAX_NOTES_PER_DOCUMENT: usize = 4_096;
/// Maximum comments accepted or emitted for one document.
pub const MAX_COMMENTS_PER_DOCUMENT: usize = 4_096;
/// Maximum tracked-change revision occurrences accepted for one document.
pub const MAX_REVISIONS_PER_DOCUMENT: usize = 16_384;
/// Maximum UTF-8 byte length of one footnote, endnote, or comment body text.
pub const MAX_NOTE_TEXT_BYTES: usize = 65_536;
/// Maximum UTF-8 byte length of one comment or revision author name.
pub const MAX_AUTHOR_BYTES: usize = 256;
/// Maximum UTF-8 byte length of one comment or revision ISO timestamp.
pub const MAX_DATE_BYTES: usize = 64;
/// Maximum nested active comment ranges allowed simultaneously while parsing.
pub const MAX_ACTIVE_COMMENT_RANGES: usize = 64;

/// Kind of a document note (`Footnote` or `Endnote`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RichNoteKind {
    Footnote = 1,
    Endnote = 2,
}

impl RichNoteKind {
    /// Canonical JSON / wire identifier (`"footnote"` or `"endnote"`).
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Footnote => "footnote",
            Self::Endnote => "endnote",
        }
    }

    /// Parses `"footnote"` or `"endnote"`.
    pub fn from_wire_name(raw: &str) -> Option<Self> {
        match raw {
            "footnote" => Some(Self::Footnote),
            "endnote" => Some(Self::Endnote),
            _ => None,
        }
    }
}

/// A footnote or endnote entry owned by [`RichDocument`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichNote {
    pub id: u32,
    pub kind: RichNoteKind,
    pub text: String,
}

/// A review comment anchored to runs (or a point anchor run) via [`RichRun::comment_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichComment {
    pub id: u32,
    pub author: String,
    pub date: String,
    pub text: String,
}

/// Kind of a tracked-change revision (`Insert` or `Delete`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RichRevisionKind {
    Insert = 1,
    Delete = 2,
}

impl RichRevisionKind {
    /// Canonical JSON / wire identifier (`"insert"` or `"delete"`).
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Delete => "delete",
        }
    }

    /// Parses `"insert"` or `"delete"`.
    pub fn from_wire_name(raw: &str) -> Option<Self> {
        match raw {
            "insert" => Some(Self::Insert),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Tracked-change metadata attached to a [`RichRun`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RichRevision {
    pub id: u32,
    pub kind: RichRevisionKind,
    pub author: String,
    pub date: String,
}

/// Truncates `s` to at most `max_bytes` on a UTF-8 character boundary (MSRV 1.85 compatible).
pub(crate) fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Returns true when `run` represents deleted tracked-change text.
#[inline]
pub(crate) fn is_deleted_run(run: &RichRun) -> bool {
    run.revision
        .as_ref()
        .is_some_and(|r| r.kind == RichRevisionKind::Delete)
}

/// Deduplicates notes by `(kind, id)` (first wins) and enforces document and text byte bounds.
pub(crate) fn validate_notes(notes: &[RichNote]) -> Vec<RichNote> {
    if notes.is_empty() {
        return Vec::new();
    }
    let mut seen: HashSet<(RichNoteKind, u32)> = HashSet::with_capacity(notes.len().min(64));
    let mut out = Vec::with_capacity(notes.len().min(MAX_NOTES_PER_DOCUMENT));
    for note in notes {
        if out.len() >= MAX_NOTES_PER_DOCUMENT {
            break;
        }
        if seen.insert((note.kind, note.id)) {
            out.push(RichNote {
                id: note.id,
                kind: note.kind,
                text: truncate_utf8_bytes(&note.text, MAX_NOTE_TEXT_BYTES),
            });
        }
    }
    out
}

/// Deduplicates comments by `id` (first wins) and enforces document, author, date, and text bounds.
pub(crate) fn validate_comments(comments: &[RichComment]) -> Vec<RichComment> {
    if comments.is_empty() {
        return Vec::new();
    }
    let mut seen: HashSet<u32> = HashSet::with_capacity(comments.len().min(64));
    let mut out = Vec::with_capacity(comments.len().min(MAX_COMMENTS_PER_DOCUMENT));
    for comment in comments {
        if out.len() >= MAX_COMMENTS_PER_DOCUMENT {
            break;
        }
        if seen.insert(comment.id) {
            out.push(RichComment {
                id: comment.id,
                author: truncate_utf8_bytes(&comment.author, MAX_AUTHOR_BYTES),
                date: truncate_utf8_bytes(&comment.date, MAX_DATE_BYTES),
                text: truncate_utf8_bytes(&comment.text, MAX_NOTE_TEXT_BYTES),
            });
        }
    }
    out
}

pub(crate) type CollectedNoteOrdinals =
    (Vec<(RichNote, usize)>, HashMap<(RichNoteKind, u32), usize>);

/// Collects ordered `(note, 1-based ordinal)` pairs and an O(1) lookup map from `(kind, id)` to
/// its display ordinal for flat/non-DOCX/ODT writers (TXT, PDF, RTF, Pages).
pub(crate) fn collect_note_ordinals(doc: &RichDocument) -> CollectedNoteOrdinals {
    let notes = doc.validated_notes();
    let by_key: HashMap<(RichNoteKind, u32), &RichNote> =
        notes.iter().map(|n| ((n.kind, n.id), n)).collect();
    let mut ordinals: HashMap<(RichNoteKind, u32), usize> = HashMap::new();
    let mut ordered: Vec<(RichNote, usize)> = Vec::new();

    for block in &doc.blocks {
        let RichBlock::Paragraph(p) = block else {
            continue;
        };
        for run in &p.runs {
            if is_deleted_run(run) {
                continue;
            }
            if let Some(key) = run.note_ref {
                if !ordinals.contains_key(&key) && ordered.len() < MAX_NOTES_PER_DOCUMENT {
                    let ord = ordered.len() + 1;
                    ordinals.insert(key, ord);
                    let text = by_key.get(&key).map_or(String::new(), |n| n.text.clone());
                    ordered.push((
                        RichNote {
                            id: key.1,
                            kind: key.0,
                            text,
                        },
                        ord,
                    ));
                }
            }
        }
    }
    for note in &notes {
        let key = (note.kind, note.id);
        if !ordinals.contains_key(&key) && ordered.len() < MAX_NOTES_PER_DOCUMENT {
            let ord = ordered.len() + 1;
            ordinals.insert(key, ord);
            ordered.push((note.clone(), ord));
        }
    }
    (ordered, ordinals)
}

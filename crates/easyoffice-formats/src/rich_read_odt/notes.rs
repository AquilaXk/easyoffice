//! ODT parsers for `<text:note>`, `<office:annotation>`, `<office:annotation-end>`, and
//! `<text:tracked-changes>` / `<text:change*>` (`PD-252`).

use std::collections::{HashMap, HashSet};

use super::{Para, SPACES_64};
use crate::decode_budget::DecodeBudget;
use crate::rich_notes::{
    RichComment, RichNote, RichNoteKind, RichRevision, RichRevisionKind, MAX_ACTIVE_COMMENT_RANGES,
    MAX_AUTHOR_BYTES, MAX_COMMENTS_PER_DOCUMENT, MAX_DATE_BYTES, MAX_NOTES_PER_DOCUMENT,
    MAX_NOTE_TEXT_BYTES, MAX_REVISIONS_PER_DOCUMENT,
};
use crate::rich_xml::{push_annotated_run, CharFmt};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichRun, RichScript};

#[derive(Clone)]
struct OdtChangedRegion {
    rev: RichRevision,
    deleted_text: String,
}

/// Tracks notes, comments, active annotation ranges, and tracked changes while walking `content.xml`.
#[derive(Default)]
pub(super) struct OdtNotesState {
    pub(super) track_changes: bool,
    pub(super) notes: Vec<RichNote>,
    pub(super) comments: Vec<RichComment>,
    changed_regions: HashMap<String, OdtChangedRegion>,
    active_revisions: Vec<(String, RichRevision)>,
    revision_events: usize,
    active_comments: Vec<(u32, String)>,
    comment_has_runs: HashSet<u32>,
    annotation_end_names: HashSet<String>,
    used_note_ids: HashSet<(RichNoteKind, u32)>,
    used_comment_ids: HashSet<u32>,
    next_footnote_id: u32,
    next_endnote_id: u32,
    next_comment_id: u32,
    next_rev_id: u32,
}

fn append_note_text(
    out: &mut String,
    piece: &str,
    what: &str,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    if piece.is_empty() {
        return Ok(());
    }
    if out.len().saturating_add(piece.len()) > MAX_NOTE_TEXT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "{what} 본문이 상한({MAX_NOTE_TEXT_BYTES} bytes)을 초과했습니다."
        )));
    }
    budget.charge(piece.len(), what)?;
    out.push_str(piece);
    Ok(())
}

fn append_bounded_meta(
    out: &mut String,
    piece: &str,
    limit: usize,
    what: &str,
) -> Result<(), EasyOfficeError> {
    if out.len().saturating_add(piece.len()) > limit {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "{what} 길이가 상한({limit} bytes)을 초과했습니다."
        )));
    }
    out.push_str(piece);
    Ok(())
}

fn parse_prefixed_id(raw: &str, prefix: &str) -> Option<u32> {
    raw.strip_prefix(prefix)
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&n| n > 0)
}

fn push_spaces(
    out: &mut String,
    attrs: &Attrs<'_>,
    what: &str,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    let mut rem = attr(attrs, "text:c")
        .as_deref()
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(1)
        .min(super::MAX_REPEATED_SPACES);
    while rem > 0 {
        let chunk = rem.min(SPACES_64.len());
        append_note_text(out, &SPACES_64[..chunk], what, budget)?;
        rem -= chunk;
    }
    Ok(())
}

impl OdtNotesState {
    pub(super) fn init_annotation_ends(&mut self, xml: &str) -> Result<(), EasyOfficeError> {
        if !xml.contains("annotation-end") {
            return Ok(());
        }
        for ev in XmlReader::new(xml) {
            if let XmlEvent::Start {
                name: "office:annotation-end",
                attrs,
            } = ev?
            {
                if let Some(name) = attr(&attrs, "office:name") {
                    self.annotation_end_names.insert(name.into_owned());
                }
            }
        }
        Ok(())
    }

    pub(super) fn current_comment_id(&self) -> Option<u32> {
        self.active_comments.last().map(|(id, _)| *id)
    }

    pub(super) fn current_revision(&self) -> Option<&RichRevision> {
        self.active_revisions.last().map(|(_, rev)| rev)
    }

    pub(super) fn mark_current_comment_used(&mut self) {
        if let Some((cid, _)) = self.active_comments.last() {
            self.comment_has_runs.insert(*cid);
        }
    }

    fn alloc_note_id(&mut self, kind: RichNoteKind, raw: Option<&str>) -> u32 {
        let prefix = match kind {
            RichNoteKind::Footnote => "ftn",
            RichNoteKind::Endnote => "edn",
        };
        if let Some(id) = raw.and_then(|s| {
            parse_prefixed_id(s, prefix).or_else(|| s.parse().ok().filter(|&n| n > 0))
        }) {
            if self.used_note_ids.insert((kind, id)) {
                return id;
            }
        }
        let cursor = match kind {
            RichNoteKind::Footnote => &mut self.next_footnote_id,
            RichNoteKind::Endnote => &mut self.next_endnote_id,
        };
        loop {
            *cursor = cursor.saturating_add(1).max(1);
            if self.used_note_ids.insert((kind, *cursor)) {
                return *cursor;
            }
        }
    }

    fn alloc_comment_id(&mut self, raw: Option<&str>) -> u32 {
        if let Some(id) = raw.and_then(|s| parse_prefixed_id(s, "cmt").or_else(|| s.parse().ok())) {
            if self.used_comment_ids.insert(id) {
                return id;
            }
        }
        loop {
            self.next_comment_id = self.next_comment_id.saturating_add(1).max(1);
            if self.used_comment_ids.insert(self.next_comment_id) {
                return self.next_comment_id;
            }
        }
    }

    fn alloc_rev_id(&mut self, raw: &str) -> u32 {
        parse_prefixed_id(raw, "ct").unwrap_or_else(|| {
            self.next_rev_id = self.next_rev_id.saturating_add(1).max(1);
            self.next_rev_id
        })
    }

    pub(super) fn on_tracked_changes(
        &mut self,
        events: &[XmlEvent<'_>],
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        if let Some(XmlEvent::Start { attrs, .. }) = events.first() {
            self.track_changes = attr(attrs, "text:track-changes").as_deref() != Some("false");
        }
        let mut cur_region_key: Option<String> = None;
        let mut kind = RichRevisionKind::Insert;
        let mut author = String::new();
        let mut date = String::new();
        let mut deleted_text = String::new();
        let mut in_change_info = false;
        let mut in_creator = false;
        let mut in_date = false;
        let mut in_para = false;
        let mut para_count = 0usize;

        for ev in events.iter().skip(1) {
            match ev {
                XmlEvent::Start { name, attrs } => match *name {
                    "text:changed-region" => {
                        if self.changed_regions.len() >= MAX_REVISIONS_PER_DOCUMENT {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                "ODT 변경 이력 수가 상한({MAX_REVISIONS_PER_DOCUMENT})을 초과했습니다."
                            )));
                        }
                        cur_region_key = attr(attrs, "text:id")
                            .or_else(|| attr(attrs, "xml:id"))
                            .map(|s| s.into_owned());
                        kind = RichRevisionKind::Insert;
                        author.clear();
                        date.clear();
                        deleted_text.clear();
                        in_change_info = false;
                        in_creator = false;
                        in_date = false;
                        in_para = false;
                        para_count = 0;
                    }
                    "text:insertion" => kind = RichRevisionKind::Insert,
                    "text:deletion" => kind = RichRevisionKind::Delete,
                    "office:change-info" => in_change_info = true,
                    "dc:creator" if in_change_info => in_creator = true,
                    "dc:date" if in_change_info => in_date = true,
                    "text:p" | "text:h" if !in_change_info && cur_region_key.is_some() => {
                        in_para = true;
                        para_count += 1;
                        if para_count > 1 {
                            append_note_text(&mut deleted_text, "\n", "ODT 삭제 이력", budget)?;
                        }
                    }
                    "text:line-break" if in_para => {
                        append_note_text(&mut deleted_text, "\n", "ODT 삭제 이력", budget)?;
                    }
                    "text:tab" if in_para => {
                        append_note_text(&mut deleted_text, "\t", "ODT 삭제 이력", budget)?;
                    }
                    "text:s" if in_para => {
                        push_spaces(&mut deleted_text, attrs, "ODT 삭제 이력", budget)?;
                    }
                    _ => {}
                },
                XmlEvent::Text(text) => {
                    if in_creator {
                        append_bounded_meta(
                            &mut author,
                            text,
                            MAX_AUTHOR_BYTES,
                            "ODT 변경 작성자",
                        )?;
                    } else if in_date {
                        append_bounded_meta(&mut date, text, MAX_DATE_BYTES, "ODT 변경 날짜")?;
                    } else if in_para && kind == RichRevisionKind::Delete {
                        append_note_text(&mut deleted_text, text, "ODT 삭제 이력", budget)?;
                    }
                }
                XmlEvent::End(name) => match *name {
                    "office:change-info" => in_change_info = false,
                    "dc:creator" => in_creator = false,
                    "dc:date" => in_date = false,
                    "text:p" | "text:h" if in_para => in_para = false,
                    "text:changed-region" => {
                        if let Some(key) = cur_region_key.take() {
                            let id = self.alloc_rev_id(&key);
                            self.changed_regions.insert(
                                key,
                                OdtChangedRegion {
                                    rev: RichRevision {
                                        id,
                                        kind,
                                        author: std::mem::take(&mut author),
                                        date: std::mem::take(&mut date),
                                    },
                                    deleted_text: std::mem::take(&mut deleted_text),
                                },
                            );
                        }
                    }
                    _ => {}
                },
            }
        }
        Ok(())
    }

    pub(super) fn on_note(
        &mut self,
        events: &[XmlEvent<'_>],
        paras: &mut [Para],
        link: Option<&str>,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        if self.notes.len() >= MAX_NOTES_PER_DOCUMENT {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "ODT 각주·미주 수가 상한({MAX_NOTES_PER_DOCUMENT})을 초과했습니다."
            )));
        }
        let Some(XmlEvent::Start { attrs, .. }) = events.first() else {
            return Ok(());
        };
        let kind = match attr(attrs, "text:note-class").as_deref() {
            Some("endnote") => RichNoteKind::Endnote,
            _ => RichNoteKind::Footnote,
        };
        let id = self.alloc_note_id(kind, attr(attrs, "text:id").as_deref());
        let mut in_body = false;
        let mut in_para = false;
        let mut skip_depth = 0usize;
        let mut para_count = 0usize;
        let mut text_out = String::new();

        for ev in events.iter().skip(1) {
            match ev {
                XmlEvent::Start { name, attrs } => {
                    if skip_depth > 0 {
                        skip_depth += 1;
                        continue;
                    }
                    match *name {
                        "text:note-citation" => skip_depth = 1,
                        "text:note-body" => in_body = true,
                        "text:p" | "text:h" if in_body => {
                            in_para = true;
                            para_count += 1;
                            if para_count > 1 {
                                append_note_text(&mut text_out, "\n", "ODT 각주·미주", budget)?;
                            }
                        }
                        "text:line-break" if in_para => {
                            append_note_text(&mut text_out, "\n", "ODT 각주·미주", budget)?;
                        }
                        "text:tab" if in_para => {
                            append_note_text(&mut text_out, "\t", "ODT 각주·미주", budget)?;
                        }
                        "text:s" if in_para => {
                            push_spaces(&mut text_out, attrs, "ODT 각주·미주", budget)?;
                        }
                        _ => {}
                    }
                }
                XmlEvent::Text(text) => {
                    if skip_depth == 0 && in_para {
                        append_note_text(&mut text_out, text, "ODT 각주·미주", budget)?;
                    }
                }
                XmlEvent::End(name) => {
                    if skip_depth > 0 {
                        skip_depth -= 1;
                        continue;
                    }
                    match *name {
                        "text:note-body" => in_body = false,
                        "text:p" | "text:h" if in_para => in_para = false,
                        _ => {}
                    }
                }
            }
        }

        self.notes.push(RichNote {
            id,
            kind,
            text: text_out,
        });
        if let Some(top) = paras.last_mut() {
            let comment_id = self.current_comment_id();
            self.mark_current_comment_used();
            budget.charge_run(0, "ODT 각주 참조")?;
            top.runs.push(RichRun {
                script: RichScript::Superscript,
                link: link.map(str::to_string),
                note_ref: Some((kind, id)),
                comment_id,
                revision: self.current_revision().cloned(),
                ..RichRun::default()
            });
        }
        Ok(())
    }

    pub(super) fn on_annotation(
        &mut self,
        events: &[XmlEvent<'_>],
        paras: &mut [Para],
        link: Option<&str>,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        if self.comments.len() >= MAX_COMMENTS_PER_DOCUMENT {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "ODT 메모 수가 상한({MAX_COMMENTS_PER_DOCUMENT})을 초과했습니다."
            )));
        }
        let Some(XmlEvent::Start { attrs, .. }) = events.first() else {
            return Ok(());
        };
        let office_name = attr(attrs, "office:name").map(|s| s.into_owned());
        let id = self.alloc_comment_id(office_name.as_deref());
        let mut author = String::new();
        let mut date = String::new();
        let mut text_out = String::new();
        let mut skip_depth = 0usize;
        let mut in_creator = false;
        let mut in_date = false;
        let mut in_para = false;
        let mut para_count = 0usize;

        for ev in events.iter().skip(1) {
            match ev {
                XmlEvent::Start { name, attrs } => {
                    if skip_depth > 0 {
                        skip_depth += 1;
                        continue;
                    }
                    match *name {
                        "meta:creator-initials" | "loext:sender-initials" => skip_depth = 1,
                        "dc:creator" => in_creator = true,
                        "dc:date" => in_date = true,
                        "text:p" | "text:h" => {
                            in_para = true;
                            para_count += 1;
                            if para_count > 1 {
                                append_note_text(&mut text_out, "\n", "ODT 메모", budget)?;
                            }
                        }
                        "text:line-break" if in_para => {
                            append_note_text(&mut text_out, "\n", "ODT 메모", budget)?;
                        }
                        "text:tab" if in_para => {
                            append_note_text(&mut text_out, "\t", "ODT 메모", budget)?;
                        }
                        "text:s" if in_para => {
                            push_spaces(&mut text_out, attrs, "ODT 메모", budget)?;
                        }
                        _ => {}
                    }
                }
                XmlEvent::Text(text) => {
                    if skip_depth > 0 {
                        continue;
                    }
                    if in_creator {
                        append_bounded_meta(
                            &mut author,
                            text,
                            MAX_AUTHOR_BYTES,
                            "ODT 메모 작성자",
                        )?;
                    } else if in_date {
                        append_bounded_meta(&mut date, text, MAX_DATE_BYTES, "ODT 메모 날짜")?;
                    } else if in_para {
                        append_note_text(&mut text_out, text, "ODT 메모", budget)?;
                    }
                }
                XmlEvent::End(name) => {
                    if skip_depth > 0 {
                        skip_depth -= 1;
                        continue;
                    }
                    match *name {
                        "dc:creator" => in_creator = false,
                        "dc:date" => in_date = false,
                        "text:p" | "text:h" => in_para = false,
                        _ => {}
                    }
                }
            }
        }

        self.comments.push(RichComment {
            id,
            author,
            date,
            text: text_out,
        });

        let has_matching_end = office_name
            .as_deref()
            .is_some_and(|n| self.annotation_end_names.contains(n));
        if has_matching_end {
            if self.active_comments.len() >= MAX_ACTIVE_COMMENT_RANGES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "ODT 활성 메모 범위 중첩이 상한({MAX_ACTIVE_COMMENT_RANGES})을 초과했습니다."
                )));
            }
            if let Some(name) = office_name {
                self.active_comments.push((id, name));
            }
        } else if let Some(top) = paras.last_mut() {
            self.comment_has_runs.insert(id);
            budget.charge_run(0, "ODT 메모 앵커")?;
            top.runs.push(RichRun {
                link: link.map(str::to_string),
                comment_id: Some(id),
                revision: self.current_revision().cloned(),
                ..RichRun::default()
            });
        }
        Ok(())
    }

    pub(super) fn on_annotation_end(
        &mut self,
        attrs: &Attrs<'_>,
        paras: &mut [Para],
        link: Option<&str>,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let Some(name) = attr(attrs, "office:name") else {
            return Ok(());
        };
        let Some(pos) = self
            .active_comments
            .iter()
            .rposition(|(_, n)| n == name.as_ref())
        else {
            return Ok(());
        };
        let (id, _) = self.active_comments.remove(pos);
        if !self.comment_has_runs.contains(&id) {
            self.comment_has_runs.insert(id);
            budget.charge_run(0, "ODT 메모 앵커")?;
            if let Some(top) = paras.last_mut() {
                top.runs.push(RichRun {
                    link: link.map(str::to_string),
                    comment_id: Some(id),
                    revision: self.current_revision().cloned(),
                    ..RichRun::default()
                });
            }
        }
        Ok(())
    }

    pub(super) fn on_change_marker(
        &mut self,
        name: &str,
        attrs: &Attrs<'_>,
        paras: &mut [Para],
        span_fmts: &[CharFmt],
        link: Option<&str>,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let Some(cid) = attr(attrs, "text:change-id") else {
            return Ok(());
        };
        match name {
            "text:change-start" => {
                if let Some(region) = self.changed_regions.get(cid.as_ref()).cloned() {
                    self.revision_events += 1;
                    if self.revision_events > MAX_REVISIONS_PER_DOCUMENT {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "ODT 변경 이력 참조 수가 상한({MAX_REVISIONS_PER_DOCUMENT})을 초과했습니다."
                        )));
                    }
                    self.active_revisions.push((cid.into_owned(), region.rev));
                }
            }
            "text:change-end" => {
                if let Some(pos) = self
                    .active_revisions
                    .iter()
                    .rposition(|(id, _)| id == cid.as_ref())
                {
                    self.active_revisions.remove(pos);
                }
            }
            "text:change" => {
                if let Some(region) = self.changed_regions.get(cid.as_ref()).cloned() {
                    if region.rev.kind == RichRevisionKind::Delete {
                        self.revision_events += 1;
                        if self.revision_events > MAX_REVISIONS_PER_DOCUMENT {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                "ODT 변경 이력 참조 수가 상한({MAX_REVISIONS_PER_DOCUMENT})을 초과했습니다."
                            )));
                        }
                        let comment_id = self.current_comment_id();
                        self.mark_current_comment_used();
                        budget.charge_run(region.deleted_text.len(), "ODT 삭제 이력")?;
                        if let Some(top) = paras.last_mut() {
                            let fmt = span_fmts.last().cloned().unwrap_or_default();
                            if region.deleted_text.is_empty() {
                                top.runs.push(RichRun {
                                    link: link.map(str::to_string),
                                    bold: fmt.bold,
                                    italic: fmt.italic,
                                    underline: fmt.underline,
                                    strike: fmt.strike,
                                    size_half_pt: fmt.size_half_pt,
                                    font: fmt.font,
                                    color: fmt.color,
                                    highlight: fmt.highlight,
                                    script: fmt.script,
                                    comment_id,
                                    revision: Some(region.rev),
                                    ..RichRun::default()
                                });
                            } else {
                                push_annotated_run(
                                    &mut top.runs,
                                    &region.deleted_text,
                                    &fmt,
                                    link,
                                    comment_id,
                                    Some(&region.rev),
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn on_subtree(
        &mut self,
        name: &str,
        sub: &[XmlEvent<'_>],
        paras: &mut [Para],
        link: Option<&str>,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        match name {
            "text:tracked-changes" => self.on_tracked_changes(sub, budget),
            "text:note" => self.on_note(sub, paras, link, budget),
            "office:annotation" => self.on_annotation(sub, paras, link, budget),
            _ => Ok(()),
        }
    }

    pub(super) fn on_marker_start(
        &mut self,
        name: &str,
        attrs: &Attrs<'_>,
        paras: &mut [Para],
        span_fmts: &[CharFmt],
        link: Option<&str>,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        match name {
            "office:annotation-end" => self.on_annotation_end(attrs, paras, link, budget),
            _ => self.on_change_marker(name, attrs, paras, span_fmts, link, budget),
        }
    }

    pub(super) fn push_text(
        &mut self,
        paras: &mut [Para],
        span_fmts: &[CharFmt],
        link: Option<&str>,
        text: &str,
    ) {
        let cid = self.current_comment_id();
        self.mark_current_comment_used();
        let rev = self.current_revision();
        if let Some(p) = paras.last_mut() {
            push_annotated_run(
                &mut p.runs,
                text,
                &span_fmts.last().cloned().unwrap_or_default(),
                link,
                cid,
                rev,
            );
        }
    }
}

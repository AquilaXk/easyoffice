//! DOCX parsers for `word/footnotes.xml`, `word/endnotes.xml`, `word/comments.xml`, and
//! inline revision / comment-range helpers (`PD-252`).

use super::DocxWalker;
use crate::decode_budget::DecodeBudget;
use crate::rich_notes::{
    RichComment, RichNote, RichNoteKind, RichRevision, RichRevisionKind, MAX_ACTIVE_COMMENT_RANGES,
    MAX_AUTHOR_BYTES, MAX_COMMENTS_PER_DOCUMENT, MAX_DATE_BYTES, MAX_NOTES_PER_DOCUMENT,
    MAX_NOTE_TEXT_BYTES, MAX_REVISIONS_PER_DOCUMENT,
};
use crate::rich_xml::{push_marked_run, RunMarks};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichRun, RichScript};

fn append_bounded_text(
    out: &mut String,
    piece: &str,
    strip_leading_space: &mut bool,
    what: &str,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    let s = if *strip_leading_space && !piece.is_empty() {
        *strip_leading_space = false;
        piece.strip_prefix(' ').unwrap_or(piece)
    } else {
        piece
    };
    if s.is_empty() {
        return Ok(());
    }
    if out.len().saturating_add(s.len()) > MAX_NOTE_TEXT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "{what} 본문이 상한({MAX_NOTE_TEXT_BYTES} bytes)을 초과했습니다."
        )));
    }
    budget.charge(s.len(), what)?;
    out.push_str(s);
    Ok(())
}

/// Parses `word/footnotes.xml` or `word/endnotes.xml`, ignoring separator notes and stripping the
/// single separator space immediately following `<w:footnoteRef/>` / `<w:endnoteRef/>`.
pub(super) fn read_docx_notes(
    xml: Option<&str>,
    kind: RichNoteKind,
    notes: &mut Vec<RichNote>,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    let Some(xml) = xml else {
        return Ok(());
    };
    let (note_tag, ref_tag, label) = match kind {
        RichNoteKind::Footnote => ("w:footnote", "w:footnoteRef", "DOCX 각주"),
        RichNoteKind::Endnote => ("w:endnote", "w:endnoteRef", "DOCX 미주"),
    };
    let mut current_id: Option<u32> = None;
    let mut skip_depth = 0usize;
    let mut in_t = false;
    let mut strip_leading_space = false;
    let mut para_count = 0usize;
    let mut cur_text = String::new();

    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start { name, attrs } => {
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if name == note_tag {
                    let kind_attr = attr(&attrs, "w:type");
                    let is_sep = matches!(
                        kind_attr.as_deref(),
                        Some("separator" | "continuationSeparator" | "continuationNotice")
                    );
                    let id_ok = attr(&attrs, "w:id")
                        .and_then(|v| v.trim().parse::<i64>().ok())
                        .filter(|&id| id > 0)
                        .and_then(|id| u32::try_from(id).ok());
                    let Some(id) = (!is_sep).then_some(id_ok).flatten() else {
                        skip_depth = 1;
                        continue;
                    };
                    if notes.len() >= MAX_NOTES_PER_DOCUMENT {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "DOCX 각주·미주 수가 상한({MAX_NOTES_PER_DOCUMENT})을 초과했습니다."
                        )));
                    }
                    current_id = Some(id);
                    para_count = 0;
                    strip_leading_space = false;
                    in_t = false;
                    cur_text.clear();
                } else if current_id.is_some() {
                    match name {
                        "w:del" | "w:delText" | "w:instrText" | "w:rPrChange" | "w:pPrChange" => {
                            skip_depth = 1;
                        }
                        "w:p" => {
                            para_count += 1;
                            if para_count > 1 {
                                let mut no_strip = false;
                                append_bounded_text(
                                    &mut cur_text,
                                    "\n",
                                    &mut no_strip,
                                    label,
                                    budget,
                                )?;
                            }
                        }
                        t if t == ref_tag => {
                            if para_count <= 1 && cur_text.is_empty() {
                                strip_leading_space = true;
                            }
                        }
                        "w:t" => in_t = true,
                        "w:br" | "w:cr" => {
                            append_bounded_text(
                                &mut cur_text,
                                "\n",
                                &mut strip_leading_space,
                                label,
                                budget,
                            )?;
                        }
                        "w:tab" => {
                            append_bounded_text(
                                &mut cur_text,
                                "\t",
                                &mut strip_leading_space,
                                label,
                                budget,
                            )?;
                        }
                        _ => {}
                    }
                }
            }
            XmlEvent::Text(text) => {
                if skip_depth == 0 && current_id.is_some() && in_t {
                    append_bounded_text(
                        &mut cur_text,
                        &text,
                        &mut strip_leading_space,
                        label,
                        budget,
                    )?;
                }
            }
            XmlEvent::End(name) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if name == "w:t" {
                    in_t = false;
                } else if name == note_tag {
                    if let Some(id) = current_id.take() {
                        notes.push(RichNote {
                            id,
                            kind,
                            text: std::mem::take(&mut cur_text),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Parses `word/comments.xml`, enforcing comment count and author/date/text byte ceilings.
pub(super) fn read_docx_comments(
    xml: Option<&str>,
    budget: &mut DecodeBudget,
) -> Result<Vec<RichComment>, EasyOfficeError> {
    let Some(xml) = xml else {
        return Ok(Vec::new());
    };
    let mut comments = Vec::new();
    let mut open: Option<(u32, String, String)> = None;
    let mut skip_depth = 0usize;
    let mut in_t = false;
    let mut para_count = 0usize;
    let mut cur_text = String::new();
    let mut no_strip = false;

    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start { name, attrs } => {
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if name == "w:comment" {
                    let Some(id) = attr(&attrs, "w:id").and_then(|v| v.trim().parse::<u32>().ok())
                    else {
                        skip_depth = 1;
                        continue;
                    };
                    if comments.len() >= MAX_COMMENTS_PER_DOCUMENT {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "DOCX 메모 수가 상한({MAX_COMMENTS_PER_DOCUMENT})을 초과했습니다."
                        )));
                    }
                    let author =
                        attr(&attrs, "w:author").map_or_else(String::new, |v| v.into_owned());
                    if author.len() > MAX_AUTHOR_BYTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "DOCX 메모 작성자 길이가 상한({MAX_AUTHOR_BYTES} bytes)을 초과했습니다."
                        )));
                    }
                    let date = attr(&attrs, "w:date").map_or_else(String::new, |v| v.into_owned());
                    if date.len() > MAX_DATE_BYTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "DOCX 메모 날짜 길이가 상한({MAX_DATE_BYTES} bytes)을 초과했습니다."
                        )));
                    }
                    open = Some((id, author, date));
                    para_count = 0;
                    in_t = false;
                    cur_text.clear();
                } else if open.is_some() {
                    match name {
                        "w:del" | "w:delText" | "w:instrText" | "w:rPrChange" | "w:pPrChange" => {
                            skip_depth = 1;
                        }
                        "w:p" => {
                            para_count += 1;
                            if para_count > 1 {
                                append_bounded_text(
                                    &mut cur_text,
                                    "\n",
                                    &mut no_strip,
                                    "DOCX 메모",
                                    budget,
                                )?;
                            }
                        }
                        "w:t" => in_t = true,
                        "w:br" | "w:cr" => {
                            append_bounded_text(
                                &mut cur_text,
                                "\n",
                                &mut no_strip,
                                "DOCX 메모",
                                budget,
                            )?;
                        }
                        "w:tab" => {
                            append_bounded_text(
                                &mut cur_text,
                                "\t",
                                &mut no_strip,
                                "DOCX 메모",
                                budget,
                            )?;
                        }
                        _ => {}
                    }
                }
            }
            XmlEvent::Text(text) => {
                if skip_depth == 0 && open.is_some() && in_t {
                    append_bounded_text(&mut cur_text, &text, &mut no_strip, "DOCX 메모", budget)?;
                }
            }
            XmlEvent::End(name) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if name == "w:t" {
                    in_t = false;
                } else if name == "w:comment" {
                    if let Some((id, author, date)) = open.take() {
                        comments.push(RichComment {
                            id,
                            author,
                            date,
                            text: std::mem::take(&mut cur_text),
                        });
                    }
                }
            }
        }
    }
    Ok(comments)
}

impl DocxWalker {
    pub(super) fn flush_current_run_text(&mut self) -> Result<(), EasyOfficeError> {
        if self.cur_text.is_empty() {
            return Ok(());
        }
        let comment_id = self.active_comment_ranges.last().copied();
        if let Some(cid) = comment_id {
            self.comment_has_runs.insert(cid);
        }
        let link_len = self.link.as_deref().map_or(0, str::len);
        self.text_budget.charge_run(link_len, "DOCX 런")?;
        let marks = RunMarks {
            field_code: self.field_code.as_deref(),
            bookmark_start: self.pending_bookmark_start.take(),
        };
        if let Some(top) = self.paras.last_mut() {
            push_marked_run(
                &mut top.runs,
                &self.cur_text,
                &self.fmt,
                self.link.as_deref(),
                comment_id,
                self.revision_stack.last(),
                marks,
            );
        }
        self.cur_text.clear();
        Ok(())
    }

    pub(super) fn on_start_note_or_review(
        &mut self,
        name: &str,
        attrs: &Attrs<'_>,
    ) -> Result<(), EasyOfficeError> {
        match name {
            "w:ins" | "w:del" if !self.in_ppr && !self.in_rpr => {
                self.revision_count += 1;
                if self.revision_count > MAX_REVISIONS_PER_DOCUMENT {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "DOCX 변경 추적 수가 상한({MAX_REVISIONS_PER_DOCUMENT})을 초과했습니다."
                    )));
                }
                let id = attr(attrs, "w:id")
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .unwrap_or(0);
                let author = attr(attrs, "w:author").map_or_else(String::new, |v| v.into_owned());
                if author.len() > MAX_AUTHOR_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "DOCX 변경 작성자 길이가 상한({MAX_AUTHOR_BYTES} bytes)을 초과했습니다."
                    )));
                }
                let date = attr(attrs, "w:date").map_or_else(String::new, |v| v.into_owned());
                if date.len() > MAX_DATE_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "DOCX 변경 날짜 길이가 상한({MAX_DATE_BYTES} bytes)을 초과했습니다."
                    )));
                }
                let kind = if name == "w:del" {
                    RichRevisionKind::Delete
                } else {
                    RichRevisionKind::Insert
                };
                self.revision_stack.push(RichRevision {
                    id,
                    kind,
                    author,
                    date,
                });
            }
            "w:commentRangeStart" if !self.in_ppr && !self.in_rpr => {
                if let Some(id) = attr(attrs, "w:id").and_then(|v| v.trim().parse::<u32>().ok()) {
                    if self.active_comment_ranges.len() >= MAX_ACTIVE_COMMENT_RANGES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "DOCX 활성 메모 범위 중첩이 상한({MAX_ACTIVE_COMMENT_RANGES})을 초과했습니다."
                        )));
                    }
                    self.active_comment_ranges.push(id);
                }
            }
            "w:commentRangeEnd" if !self.in_ppr && !self.in_rpr => {
                if let Some(id) = attr(attrs, "w:id").and_then(|v| v.trim().parse::<u32>().ok()) {
                    if let Some(pos) = self.active_comment_ranges.iter().rposition(|&c| c == id) {
                        self.active_comment_ranges.remove(pos);
                    }
                }
            }
            "w:footnoteReference" | "w:endnoteReference"
                if self.in_run && !self.in_ppr && !self.in_rpr =>
            {
                let id_opt = attr(attrs, "w:id")
                    .and_then(|v| v.trim().parse::<i64>().ok())
                    .filter(|&id| id > 0)
                    .and_then(|id| u32::try_from(id).ok());
                if let Some(id) = id_opt {
                    self.flush_current_run_text()?;
                    let kind = if name == "w:endnoteReference" {
                        RichNoteKind::Endnote
                    } else {
                        RichNoteKind::Footnote
                    };
                    let comment_id = self.active_comment_ranges.last().copied();
                    if let Some(cid) = comment_id {
                        self.comment_has_runs.insert(cid);
                    }
                    self.text_budget.charge_run(0, "DOCX 각주 참조")?;
                    if let Some(top) = self.paras.last_mut() {
                        top.runs.push(RichRun {
                            script: RichScript::Superscript,
                            link: self.link.clone(),
                            note_ref: Some((kind, id)),
                            comment_id,
                            revision: self.revision_stack.last().cloned(),
                            ..RichRun::default()
                        });
                    }
                }
            }
            "w:commentReference" if self.in_run && !self.in_ppr && !self.in_rpr => {
                if let Some(id) = attr(attrs, "w:id").and_then(|v| v.trim().parse::<u32>().ok()) {
                    if !self.comment_has_runs.contains(&id) {
                        self.flush_current_run_text()?;
                        self.comment_has_runs.insert(id);
                        self.text_budget.charge_run(0, "DOCX 메모 앵커")?;
                        if let Some(top) = self.paras.last_mut() {
                            top.runs.push(RichRun {
                                link: self.link.clone(),
                                comment_id: Some(id),
                                revision: self.revision_stack.last().cloned(),
                                ..RichRun::default()
                            });
                        }
                    }
                }
            }
            "w:delText" if self.in_run => self.in_del_text = true,
            _ => {}
        }
        Ok(())
    }
}

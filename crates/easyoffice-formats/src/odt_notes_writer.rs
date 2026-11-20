//! ODT (ODF Text) writers for footnotes, endnotes, annotations (comments), and tracked changes.

use std::collections::HashMap;
use std::fmt::Write;

use crate::rich_document::{safe_link_target, RichBlock, RichDocument, RichParagraph};
use crate::rich_notes::{
    is_deleted_run, truncate_utf8_bytes, RichComment, RichNoteKind, RichRevision, RichRevisionKind,
    MAX_AUTHOR_BYTES, MAX_DATE_BYTES,
};
use crate::rich_page_frame::odt_field;
use crate::rich_run_writers::{odt_text_key, odt_text_key_is_plain, OdtTextKey};
use crate::rich_writers::{push_odt_text, StyleTable};
use crate::XmlSanitizer;

/// Collects `<text:changed-region>` entries while writing body paragraphs and renders
/// `<text:tracked-changes>` for the start of `<office:text>`.
#[derive(Default)]
pub(crate) struct OdtNotesContext {
    pub(crate) track_changes: bool,
    regions: Vec<(String, RichRevision, String)>,
    /// Bodies not yet written: the first reference takes its body out, a repeat is written without one,
    /// so a note referenced N times costs one body, not N (PD-262).
    note_by_key: HashMap<(RichNoteKind, u32), String>,
    /// Same rule for comment annotations, which a comment with many anchors opens again and again.
    comment_by_id: HashMap<u32, RichComment>,
    /// Body paragraphs still to come that carry each comment. A comment over several paragraphs gets one
    /// `office:annotation` and its `office:annotation-end` in the last of them.
    comment_paras_left: HashMap<u32, usize>,
    /// The comment whose annotation is still open at the end of the previous paragraph.
    open_comment: Option<u32>,
}

impl OdtNotesContext {
    pub(crate) fn new(doc: &RichDocument) -> Self {
        let note_by_key = doc
            .validated_notes()
            .into_iter()
            .map(|n| ((n.kind, n.id), n.text))
            .collect();
        let comment_by_id = doc
            .validated_comments()
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        let mut comment_paras_left: HashMap<u32, usize> = HashMap::new();
        for block in &doc.blocks {
            if let RichBlock::Paragraph(p) = block {
                for cid in comment_ids_of(p) {
                    *comment_paras_left.entry(cid).or_default() += 1;
                }
            }
        }
        Self {
            track_changes: doc.track_changes,
            regions: Vec::new(),
            note_by_key,
            comment_by_id,
            comment_paras_left,
            open_comment: None,
        }
    }

    fn alloc_change(&mut self, rev: &RichRevision, deleted_text: &str) -> String {
        let seq = self.regions.len() + 1;
        let id = format!("ct{seq}");
        self.regions
            .push((id.clone(), rev.clone(), deleted_text.to_string()));
        id
    }

    /// Emits `<text:tracked-changes ...>` when `track_changes` is enabled or any revision exists.
    pub(crate) fn tracked_changes_xml(&self) -> String {
        if !self.track_changes && self.regions.is_empty() {
            return String::new();
        }
        let mut out = format!(
            "<text:tracked-changes text:track-changes=\"{}\">",
            if self.track_changes { "true" } else { "false" }
        );
        for (cid, rev, deleted_text) in &self.regions {
            let author = XmlSanitizer::escape(&truncate_utf8_bytes(&rev.author, MAX_AUTHOR_BYTES));
            let date = XmlSanitizer::escape(&truncate_utf8_bytes(&rev.date, MAX_DATE_BYTES));
            let _ = write!(
                out,
                "<text:changed-region xml:id=\"{cid}\" text:id=\"{cid}\">"
            );
            match rev.kind {
                RichRevisionKind::Insert => {
                    let _ = write!(
                        out,
                        "<text:insertion><office:change-info><dc:creator>{author}</dc:creator><dc:date>{date}</dc:date></office:change-info></text:insertion>"
                    );
                }
                RichRevisionKind::Delete => {
                    let _ = write!(
                        out,
                        "<text:deletion><office:change-info><dc:creator>{author}</dc:creator><dc:date>{date}</dc:date></office:change-info><text:p>"
                    );
                    push_odt_text(&mut out, deleted_text);
                    out.push_str("</text:p></text:deletion>");
                }
            }
            out.push_str("</text:changed-region>");
        }
        out.push_str("</text:tracked-changes>");
        out
    }
}

/// The distinct comments whose anchors are in `p`, in order of appearance.
fn comment_ids_of(p: &RichParagraph) -> Vec<u32> {
    let mut ids: Vec<u32> = Vec::new();
    for cid in p.runs.iter().filter_map(|r| r.comment_id) {
        if !ids.contains(&cid) {
            ids.push(cid);
        }
    }
    ids
}

fn push_odt_note_paragraphs(out: &mut String, text: &str) {
    for line in text.split('\n') {
        out.push_str("<text:p>");
        push_odt_text(out, line);
        out.push_str("</text:p>");
    }
}

fn open_odt_comment(body: &mut String, cid: u32, comment_by_id: &mut HashMap<u32, RichComment>) {
    let _ = write!(body, "<office:annotation office:name=\"cmt{cid}\">");
    if let Some(c) = comment_by_id.remove(&cid) {
        let author = XmlSanitizer::escape(&c.author);
        let date = XmlSanitizer::escape(&c.date);
        let _ = write!(
            body,
            "<dc:creator>{author}</dc:creator><dc:date>{date}</dc:date>"
        );
        push_odt_note_paragraphs(body, &c.text);
    } else {
        body.push_str("<text:p/>");
    }
    body.push_str("</office:annotation>");
}

fn close_odt_comment(body: &mut String, open_comment: &mut Option<u32>) {
    if let Some(cid) = open_comment.take() {
        let _ = write!(body, "<office:annotation-end office:name=\"cmt{cid}\"/>");
    }
}

fn close_odt_insert_rev(body: &mut String, open_ins: &mut Option<(&RichRevision, String)>) {
    if let Some((_, cid)) = open_ins.take() {
        let _ = write!(body, "<text:change-end text:change-id=\"{cid}\"/>");
    }
}

/// Emits runs of one ODT paragraph with inline notes, comment annotations, and tracked changes.
pub(crate) fn push_odt_paragraph_runs(
    body: &mut String,
    p: &RichParagraph,
    text_styles: &mut StyleTable<OdtTextKey>,
    ctx: &mut OdtNotesContext,
) {
    let comment_ids = comment_ids_of(p);
    let mut cur_comment: Option<u32> = ctx.open_comment.take();
    if cur_comment.is_some_and(|cid| !comment_ids.contains(&cid)) {
        close_odt_comment(body, &mut cur_comment);
    }
    let mut cur_ins: Option<(&RichRevision, String)> = None;
    let mut cur_link: Option<&str> = None;

    for run in p.runs.iter().filter(|r| {
        !r.text.is_empty() || r.field.is_some() || r.note_ref.is_some() || r.comment_id.is_some()
    }) {
        let want_comment = run.comment_id;
        let want_ins = run
            .revision
            .as_ref()
            .filter(|r| r.kind == RichRevisionKind::Insert);
        let want_del = is_deleted_run(run);
        let want_link = run.link.as_deref().and_then(safe_link_target);

        if cur_link.is_some()
            && (cur_link != want_link
                || cur_ins.as_ref().map(|(r, _)| *r) != want_ins
                || cur_comment != want_comment
                || want_del)
        {
            body.push_str("</text:a>");
            cur_link = None;
        }
        if cur_ins.is_some()
            && (cur_ins.as_ref().map(|(r, _)| *r) != want_ins
                || cur_comment != want_comment
                || want_del)
        {
            close_odt_insert_rev(body, &mut cur_ins);
        }
        if cur_comment != want_comment {
            close_odt_comment(body, &mut cur_comment);
            if let Some(cid) = want_comment {
                open_odt_comment(body, cid, &mut ctx.comment_by_id);
                cur_comment = Some(cid);
            }
        }
        if cur_ins.is_none() {
            if let Some(rev) = want_ins {
                let cid = ctx.alloc_change(rev, "");
                let _ = write!(body, "<text:change-start text:change-id=\"{cid}\"/>");
                cur_ins = Some((rev, cid));
            }
        }
        if cur_link.is_none() {
            if let Some(url) = want_link {
                let _ = write!(
                    body,
                    "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
                    XmlSanitizer::escape(url)
                );
                cur_link = Some(url);
            }
        }

        if let Some((kind, id)) = run.note_ref {
            let (prefix, class) = match kind {
                RichNoteKind::Footnote => ("ftn", "footnote"),
                RichNoteKind::Endnote => ("edn", "endnote"),
            };
            let note_text = ctx.note_by_key.remove(&(kind, id)).unwrap_or_default();
            let _ = write!(
                body,
                "<text:note text:id=\"{prefix}{id}\" text:note-class=\"{class}\"><text:note-citation>{id}</text:note-citation><text:note-body>"
            );
            push_odt_note_paragraphs(body, &note_text);
            body.push_str("</text:note-body></text:note>");
            continue;
        }

        if let Some(rev) = run
            .revision
            .as_ref()
            .filter(|r| r.kind == RichRevisionKind::Delete)
        {
            let cid = ctx.alloc_change(rev, &run.text);
            let key = odt_text_key(run);
            let elem = format!("<text:change text:change-id=\"{cid}\"/>");
            if odt_text_key_is_plain(&key) {
                body.push_str(&elem);
            } else {
                let t = text_styles.intern(&key);
                let _ = write!(
                    body,
                    "<text:span text:style-name=\"T{t}\">{elem}</text:span>"
                );
            }
            continue;
        }

        let key = odt_text_key(run);
        if let Some(field) = run.field {
            let style = (!odt_text_key_is_plain(&key)).then(|| text_styles.intern(&key));
            body.push_str(&odt_field(field, style));
        } else if run.text.is_empty() {
            // Empty point comment anchor run handled by annotation + annotation-end.
        } else if odt_text_key_is_plain(&key) {
            push_odt_text(body, &run.text);
        } else {
            let t = text_styles.intern(&key);
            let _ = write!(body, "<text:span text:style-name=\"T{t}\">");
            push_odt_text(body, &run.text);
            body.push_str("</text:span>");
        }
    }

    if cur_link.is_some() {
        body.push_str("</text:a>");
    }
    close_odt_insert_rev(body, &mut cur_ins);
    for cid in &comment_ids {
        if let Some(left) = ctx.comment_paras_left.get_mut(cid) {
            *left = left.saturating_sub(1);
        }
    }
    match cur_comment {
        Some(cid)
            if ctx
                .comment_paras_left
                .get(&cid)
                .is_some_and(|&left| left > 0) =>
        {
            ctx.open_comment = Some(cid);
        }
        _ => close_odt_comment(body, &mut cur_comment),
    }
}

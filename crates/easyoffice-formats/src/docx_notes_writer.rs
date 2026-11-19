//! DOCX (WordprocessingML) writers for footnotes, endnotes, comments, and tracked-change
//! revisions (`word/footnotes.xml`, `word/endnotes.xml`, `word/comments.xml`, inline runs).

use std::collections::HashSet;
use std::fmt::Write;

use crate::rich_document::{safe_link_target, RichDocument, RichParagraph};
use crate::rich_link_writers::{docx_linked_run_properties, DocxLinks};
use crate::rich_notes::{
    is_deleted_run, truncate_utf8_bytes, RichNoteKind, RichRevision, RichRevisionKind,
    MAX_AUTHOR_BYTES, MAX_DATE_BYTES, MAX_NOTES_PER_DOCUMENT,
};
use crate::rich_page_frame::docx_field_run;
use crate::rich_run_writers::docx_run_properties;
use crate::{EasyOfficeError, XmlSanitizer};

const DOCX_FOOTNOTES_PART: &str = "word/footnotes.xml";
const DOCX_ENDNOTES_PART: &str = "word/endnotes.xml";
const DOCX_COMMENTS_PART: &str = "word/comments.xml";

const DOCX_FOOTNOTES_REL_ID: &str = "rIdFootnotes";
const DOCX_ENDNOTES_REL_ID: &str = "rIdEndnotes";
const DOCX_COMMENTS_REL_ID: &str = "rIdComments";

const DOCX_FOOTNOTES_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
const DOCX_ENDNOTES_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
const DOCX_COMMENTS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";

const DOCX_FOOTNOTES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
const DOCX_ENDNOTES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml";
const DOCX_COMMENTS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";

pub(crate) struct DocxNoteParts {
    pub(crate) entries: Vec<(String, Vec<u8>)>,
    pub(crate) relationships: String,
    pub(crate) content_types: String,
}

/// Emits `<w:t>` or `<w:delText>` segments inside `<w:r>`, splitting `\n` -> `<w:br/>` and
/// `\t` -> `<w:tab/>`.
pub(crate) fn push_docx_text_tagged(out: &mut String, text: &str, tag: &str) {
    for (l_idx, line) in text.split('\n').enumerate() {
        if l_idx > 0 {
            out.push_str("<w:br/>");
        }
        for (t_idx, seg) in line.split('\t').enumerate() {
            if t_idx > 0 {
                out.push_str("<w:tab/>");
            }
            if !seg.is_empty() {
                let _ = write!(out, "<{tag} xml:space=\"preserve\">");
                out.push_str(&XmlSanitizer::escape(seg));
                let _ = write!(out, "</{tag}>");
            }
        }
    }
}

fn build_docx_notes_xml(
    root: &str,
    item_tag: &str,
    ref_tag: &str,
    notes: &[(u32, String)],
) -> String {
    let mut xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<{root} xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<{item_tag} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></{item_tag}>",
            "<{item_tag} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></{item_tag}>"
        ),
        root = root,
        item_tag = item_tag,
    );
    for (id, text) in notes {
        let _ = write!(
            xml,
            "<{item_tag} w:id=\"{id}\"><w:p><w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><{ref_tag}/></w:r><w:r>"
        );
        let with_space = format!(" {text}");
        push_docx_text_tagged(&mut xml, &with_space, "w:t");
        let _ = write!(xml, "</w:r></w:p></{item_tag}>");
    }
    let _ = write!(xml, "</{root}>");
    xml
}

fn build_docx_comments_xml(doc: &RichDocument) -> Option<String> {
    let comments = doc.validated_comments();
    if comments.is_empty() {
        return None;
    }
    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">"
    ));
    for c in comments {
        let author = XmlSanitizer::escape(&c.author);
        let date = XmlSanitizer::escape(&c.date);
        let _ = write!(
            xml,
            "<w:comment w:id=\"{}\" w:author=\"{author}\" w:date=\"{date}\"><w:p><w:r>",
            c.id
        );
        if c.text.is_empty() {
            xml.push_str("<w:t/>");
        } else {
            push_docx_text_tagged(&mut xml, &c.text, "w:t");
        }
        xml.push_str("</w:r></w:p></w:comment>");
    }
    xml.push_str("</w:comments>");
    Some(xml)
}

/// Builds `word/footnotes.xml`, `word/endnotes.xml`, and `word/comments.xml` plus their
/// relationships and content-type overrides when present.
pub(crate) fn build_docx_note_parts(doc: &RichDocument) -> Result<DocxNoteParts, EasyOfficeError> {
    let mut parts = DocxNoteParts {
        entries: Vec::new(),
        relationships: String::new(),
        content_types: String::new(),
    };

    let validated = doc.validated_notes();
    let mut seen: HashSet<(RichNoteKind, u32)> = HashSet::with_capacity(validated.len());
    let mut footnotes: Vec<(u32, String)> = Vec::new();
    let mut endnotes: Vec<(u32, String)> = Vec::new();

    for n in validated {
        if n.id == 0 {
            continue;
        }
        seen.insert((n.kind, n.id));
        match n.kind {
            RichNoteKind::Footnote => footnotes.push((n.id, n.text)),
            RichNoteKind::Endnote => endnotes.push((n.id, n.text)),
        }
    }
    for block in &doc.blocks {
        let crate::RichBlock::Paragraph(p) = block else {
            continue;
        };
        for run in &p.runs {
            if let Some((kind, id)) = run.note_ref {
                if id > 0 && seen.len() < MAX_NOTES_PER_DOCUMENT && seen.insert((kind, id)) {
                    match kind {
                        RichNoteKind::Footnote => footnotes.push((id, String::new())),
                        RichNoteKind::Endnote => endnotes.push((id, String::new())),
                    }
                }
            }
        }
    }

    if !footnotes.is_empty() {
        let xml = build_docx_notes_xml("w:footnotes", "w:footnote", "w:footnoteRef", &footnotes);
        XmlSanitizer::validate_safe_xml(&xml)?;
        parts
            .entries
            .push((DOCX_FOOTNOTES_PART.to_string(), xml.into_bytes()));
        let _ = write!(
            parts.relationships,
            "<Relationship Id=\"{DOCX_FOOTNOTES_REL_ID}\" Type=\"{DOCX_FOOTNOTES_REL_TYPE}\" Target=\"footnotes.xml\"/>"
        );
        let _ = write!(
            parts.content_types,
            "<Override PartName=\"/{DOCX_FOOTNOTES_PART}\" ContentType=\"{DOCX_FOOTNOTES_CONTENT_TYPE}\"/>"
        );
    }

    if !endnotes.is_empty() {
        let xml = build_docx_notes_xml("w:endnotes", "w:endnote", "w:endnoteRef", &endnotes);
        XmlSanitizer::validate_safe_xml(&xml)?;
        parts
            .entries
            .push((DOCX_ENDNOTES_PART.to_string(), xml.into_bytes()));
        let _ = write!(
            parts.relationships,
            "<Relationship Id=\"{DOCX_ENDNOTES_REL_ID}\" Type=\"{DOCX_ENDNOTES_REL_TYPE}\" Target=\"endnotes.xml\"/>"
        );
        let _ = write!(
            parts.content_types,
            "<Override PartName=\"/{DOCX_ENDNOTES_PART}\" ContentType=\"{DOCX_ENDNOTES_CONTENT_TYPE}\"/>"
        );
    }

    if let Some(xml) = build_docx_comments_xml(doc) {
        XmlSanitizer::validate_safe_xml(&xml)?;
        parts
            .entries
            .push((DOCX_COMMENTS_PART.to_string(), xml.into_bytes()));
        let _ = write!(
            parts.relationships,
            "<Relationship Id=\"{DOCX_COMMENTS_REL_ID}\" Type=\"{DOCX_COMMENTS_REL_TYPE}\" Target=\"comments.xml\"/>"
        );
        let _ = write!(
            parts.content_types,
            "<Override PartName=\"/{DOCX_COMMENTS_PART}\" ContentType=\"{DOCX_COMMENTS_CONTENT_TYPE}\"/>"
        );
    }

    Ok(parts)
}

fn close_revision(out: &mut String, open_rev: &mut Option<&RichRevision>) {
    if let Some(rev) = open_rev.take() {
        out.push_str(match rev.kind {
            RichRevisionKind::Insert => "</w:ins>",
            RichRevisionKind::Delete => "</w:del>",
        });
    }
}

fn close_comment(out: &mut String, open_comment: &mut Option<u32>) {
    if let Some(cid) = open_comment.take() {
        let _ = write!(
            out,
            "<w:commentRangeEnd w:id=\"{cid}\"/><w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:commentReference w:id=\"{cid}\"/></w:r>"
        );
    }
}

/// Emits runs of one DOCX paragraph, maintaining well-nested hyperlinks, comment ranges,
/// tracked-change `<w:ins>`/`<w:del>` wrappers, and `<w:footnoteReference>`/`<w:endnoteReference>`.
pub(crate) fn push_docx_paragraph_runs(out: &mut String, p: &RichParagraph, links: &mut DocxLinks) {
    let mut cur_comment: Option<u32> = None;
    let mut cur_rev: Option<&RichRevision> = None;
    let mut cur_link: Option<&str> = None;

    for run in p.runs.iter().filter(|r| {
        !r.text.is_empty() || r.field.is_some() || r.note_ref.is_some() || r.comment_id.is_some()
    }) {
        let want_comment = run.comment_id;
        let want_rev = run.revision.as_ref();
        let want_link = run.link.as_deref().and_then(safe_link_target);

        if cur_link.is_some()
            && (cur_link != want_link || cur_rev != want_rev || cur_comment != want_comment)
        {
            out.push_str("</w:hyperlink>");
            cur_link = None;
        }
        if cur_rev.is_some() && (cur_rev != want_rev || cur_comment != want_comment) {
            close_revision(out, &mut cur_rev);
        }
        if cur_comment != want_comment {
            close_comment(out, &mut cur_comment);
            if let Some(cid) = want_comment {
                let _ = write!(out, "<w:commentRangeStart w:id=\"{cid}\"/>");
                cur_comment = Some(cid);
            }
        }
        if cur_rev != want_rev {
            if let Some(rev) = want_rev {
                let tag = match rev.kind {
                    RichRevisionKind::Insert => "w:ins",
                    RichRevisionKind::Delete => "w:del",
                };
                let author =
                    XmlSanitizer::escape(&truncate_utf8_bytes(&rev.author, MAX_AUTHOR_BYTES));
                let date = XmlSanitizer::escape(&truncate_utf8_bytes(&rev.date, MAX_DATE_BYTES));
                let _ = write!(
                    out,
                    "<{tag} w:id=\"{}\" w:author=\"{author}\" w:date=\"{date}\">",
                    rev.id
                );
                cur_rev = Some(rev);
            }
        }
        if cur_link != want_link {
            if let Some(url) = want_link {
                match url.strip_prefix('#') {
                    Some(anchor) => {
                        let _ = write!(out, "<w:hyperlink w:anchor=\"{anchor}\" w:history=\"1\">");
                    }
                    None => {
                        let _ = write!(
                            out,
                            "<w:hyperlink r:id=\"{}\" w:history=\"1\">",
                            links.rel_id(url)
                        );
                    }
                }
                cur_link = Some(url);
            }
        }

        if let Some((kind, id)) = run.note_ref {
            let tag = match kind {
                RichNoteKind::Footnote => "w:footnoteReference",
                RichNoteKind::Endnote => "w:endnoteReference",
            };
            let _ = write!(
                out,
                "<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><{tag} w:id=\"{id}\"/></w:r>"
            );
            continue;
        }
        let properties = docx_run_properties(run);
        if let Some(field) = run.field {
            out.push_str(&docx_field_run(field, &properties));
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        out.push_str("<w:r>");
        out.push_str(&if cur_link.is_some() {
            docx_linked_run_properties(&properties)
        } else {
            properties
        });
        let text_tag = if is_deleted_run(run) {
            "w:delText"
        } else {
            "w:t"
        };
        push_docx_text_tagged(out, &run.text, text_tag);
        out.push_str("</w:r>");
    }

    if cur_link.is_some() {
        out.push_str("</w:hyperlink>");
    }
    close_revision(out, &mut cur_rev);
    close_comment(out, &mut cur_comment);
}

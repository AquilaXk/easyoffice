//! The Word-style parts of the DOCX walk (PD-395): bookmarks, section breaks and line numbers.

use super::*;

/// The properties of `docProps/core.xml` (author, subject, keywords, comments), cut to their bounds.
pub(super) fn read_properties(core: Option<&str>) -> RichProperties {
    let Some(xml) = core else {
        return RichProperties::default();
    };
    let first = |tag: &str| {
        crate::rtf_xml_helpers::extract_tag_bodies(xml, tag)
            .into_iter()
            .next()
            .map(XmlSanitizer::unescape)
            .unwrap_or_default()
    };
    let author = first("dc:creator");
    RichProperties {
        title: String::new(),
        author: if author == PLACEHOLDER_CREATOR {
            String::new()
        } else {
            author
        },
        subject: first("dc:subject"),
        keywords: first("cp:keywords"),
        comments: first("dc:description"),
    }
    .bounded()
}

impl DocxWalker {
    /// `w:bookmarkStart`: the name is kept when the Word program would accept it ([`safe_bookmark_name`]) and the document has
    /// room for it; the next run of text carries the start.
    pub(super) fn start_bookmark(&mut self, attrs: &Attrs<'_>) {
        let (Some(id), Some(name)) = (
            attr(attrs, "w:id"),
            attr(attrs, "w:name").and_then(|n| safe_bookmark_name(&n)),
        ) else {
            return;
        };
        if self.bookmark_total >= MAX_BOOKMARKS {
            return;
        }
        self.bookmark_total += 1;
        self.bookmark_names.insert(id.into_owned(), name.clone());
        self.pending_bookmark_start = Some(name);
    }

    /// `w:bookmarkEnd`: the bookmark ends after the last run read. A range with no text in it (the start is still waiting for
    /// its run) is dropped.
    pub(super) fn end_bookmark(&mut self, attrs: &Attrs<'_>) {
        let Some(name) = attr(attrs, "w:id").and_then(|id| self.bookmark_names.remove(id.as_ref()))
        else {
            return;
        };
        if self.pending_bookmark_start.as_deref() == Some(name.as_str()) {
            self.pending_bookmark_start = None;
            return;
        }
        let Some(top) = self.paras.last_mut() else {
            return;
        };
        match top.runs.last_mut() {
            Some(last) if last.bookmark_end.is_none() => last.bookmark_end = Some(name),
            _ => top.runs.push(RichRun {
                bookmark_end: Some(name),
                ..RichRun::default()
            }),
        }
    }

    /// `w:lnNumType` of a section: the first one in the file is the document's. `w:start` counts from 0 (ECMA-376 17.6.8), so
    /// the first number shown is `start + 1`.
    pub(super) fn read_line_numbers(&mut self, attrs: &Attrs<'_>) {
        if self.line_numbers.is_some() {
            return;
        }
        let number = |key: &str, default: u32| {
            attr(attrs, key)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(default)
        };
        self.line_numbers = RichLineNumbers {
            start: number("w:start", 0).saturating_add(1),
            count_by: number("w:countBy", 1),
            distance_twips: number("w:distance", 0),
            restart: attr(attrs, "w:restart")
                .as_deref()
                .and_then(RichLineNumberRestart::from_wire_name)
                .unwrap_or_default(),
        }
        .checked();
    }

    /// Puts the section breaks on the paragraphs that end a section: the break after a section starts the next section the way
    /// that section's `w:type` says (no type is a new page). A section that ended with something that is not a paragraph gets an
    /// empty paragraph for it.
    pub(super) fn apply_section_breaks(&mut self) {
        let marks = std::mem::take(&mut self.section_marks);
        for (len, ordinal) in marks.into_iter().rev() {
            let kind = self
                .section_types
                .get(ordinal + 1)
                .copied()
                .flatten()
                .unwrap_or(RichSectionBreak::NextPage);
            match len.checked_sub(1).and_then(|i| self.blocks.get_mut(i)) {
                Some(RichBlock::Paragraph(p)) if p.section_break.is_none() => {
                    p.section_break = Some(kind);
                }
                _ => self.blocks.insert(
                    len.min(self.blocks.len()),
                    RichBlock::Paragraph(RichParagraph {
                        section_break: Some(kind),
                        ..RichParagraph::default()
                    }),
                ),
            }
        }
    }
}

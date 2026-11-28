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
    RichProperties {
        title: String::new(),
        author: first("dc:creator"),
        subject: first("dc:subject"),
        keywords: first("cp:keywords"),
        comments: first("dc:description"),
    }
    .bounded()
}

/// The text of the watermark in a header part (the `string` of the `v:textpath` of a `PowerPlusWaterMarkObject` shape), and the
/// part without the paragraph that holds the shape, so the paragraph is not a blank header line and the shape is not reported
/// as an unsupported object. Anything else in the part stays as it was.
pub(super) fn split_watermark(xml: &str) -> (Option<String>, Cow<'_, str>) {
    const MARK: &str = "PowerPlusWaterMarkObject";
    // Only a part that names the shape needs a read; the shape itself is found by its element and attributes below, so the
    // name in a text run or in another attribute is not taken for it.
    if !xml.contains(MARK) {
        return (None, Cow::Borrowed(xml));
    }
    // Where a name that the reader gave us (a slice of `xml`) starts in `xml`.
    let offset = |part: &str| (part.as_ptr() as usize).wrapping_sub(xml.as_ptr() as usize);
    let mut reader = XmlReader::new(xml);
    // The outermost open paragraph (its depth and where its tag starts) and what was found in it.
    let (mut depth, mut para_start) = (0usize, 0usize);
    let (mut found, mut text) = (false, None);
    while let Ok(Some(event)) = reader.next_event() {
        match event {
            XmlEvent::Start { name, .. } if name == "w:p" => {
                if depth == 0 {
                    para_start = offset(name).saturating_sub(1);
                }
                depth += 1;
            }
            XmlEvent::Start {
                name: "v:shape",
                attrs,
            } if !found => {
                found = depth > 0 && attr(&attrs, "id").is_some_and(|id| id.starts_with(MARK));
            }
            XmlEvent::Start {
                name: "v:textpath",
                attrs,
            } if found && text.is_none() => {
                text = attr(&attrs, "string")
                    .and_then(|t| crate::rich_word_features::safe_watermark_text(&t));
            }
            XmlEvent::End(name) if name == "w:p" => {
                depth = depth.saturating_sub(1);
                if depth == 0 && found {
                    // The paragraph ends at the `>` after the name of its end tag.
                    let name_end = offset(name).saturating_add(name.len());
                    let end = xml
                        .get(name_end..)
                        .and_then(|rest| rest.find('>'))
                        .map(|i| name_end + i + 1);
                    return match (xml.get(..para_start), end.and_then(|e| xml.get(e..))) {
                        (Some(head), Some(tail)) => (text, Cow::Owned([head, tail].concat())),
                        _ => (text, Cow::Borrowed(xml)),
                    };
                }
            }
            _ => {}
        }
    }
    (None, Cow::Borrowed(xml))
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
        // A second bookmark that starts before any text: the first one gets an empty run of its own so that neither is lost.
        if let Some(earlier) = self.pending_bookmark_start.replace(name) {
            if let Some(top) = self.paras.last_mut() {
                top.runs.push(RichRun {
                    bookmark_start: Some(earlier),
                    ..RichRun::default()
                });
            }
        }
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
            start: number("w:start", 0)
                .saturating_add(1)
                .min(MAX_LINE_NUMBER_VALUE),
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
    ///
    /// One pass builds the new block list (the marks are in the order of their block positions), so a file with many sections
    /// costs time in proportion to its size.
    pub(super) fn apply_section_breaks(&mut self) {
        let marks = std::mem::take(&mut self.section_marks);
        if marks.is_empty() {
            return;
        }
        let blocks = std::mem::take(&mut self.blocks);
        let count = blocks.len();
        let types = &self.section_types;
        let mut marks = marks
            .into_iter()
            .map(|(len, ordinal)| {
                let kind = types
                    .get(ordinal + 1)
                    .copied()
                    .flatten()
                    .unwrap_or(RichSectionBreak::NextPage);
                (len.min(count), kind)
            })
            .peekable();
        let mut out = Vec::with_capacity(count + marks.len());
        let mut blocks = blocks.into_iter();
        for position in 0..=count {
            if position > 0 {
                out.extend(blocks.next());
            }
            let mut group = Vec::new();
            while let Some((_, kind)) = marks.next_if(|(len, _)| *len == position) {
                group.push(kind);
            }
            // The last break of the group goes on the paragraph before it when that paragraph has none; the others and the
            // breaks after something that is not a paragraph get an empty paragraph each.
            if let Some(RichBlock::Paragraph(p)) = out.last_mut().filter(|_| position > 0) {
                if p.section_break.is_none() {
                    p.section_break = group.pop();
                }
            }
            out.extend(group.into_iter().map(|kind| {
                RichBlock::Paragraph(RichParagraph {
                    section_break: Some(kind),
                    ..RichParagraph::default()
                })
            }));
        }
        self.blocks = out;
    }
}

#[cfg(test)]
mod tests {
    use super::split_watermark;

    const SHAPE: &str = "<w:p><w:r><w:pict><v:shape id=\"PowerPlusWaterMarkObject1\"><v:textpath string=\"초안 &amp; 검토\"/></v:shape></w:pict></w:r></w:p>";

    #[test]
    fn the_shape_is_found_by_its_element_and_its_paragraph_is_cut_out() {
        let xml = format!("<w:hdr><w:p><w:r><w:t>머리글</w:t></w:r></w:p>{SHAPE}<w:p><w:r><w:t>끝</w:t></w:r></w:p></w:hdr>");
        let (text, rest) = split_watermark(&xml);
        assert_eq!(text.as_deref(), Some("초안 & 검토"));
        assert_eq!(
            rest,
            "<w:hdr><w:p><w:r><w:t>머리글</w:t></w:r></w:p><w:p><w:r><w:t>끝</w:t></w:r></w:p></w:hdr>"
        );
    }

    #[test]
    fn the_name_in_text_or_in_another_element_is_not_a_watermark() {
        for xml in [
            "<w:hdr><w:p><w:r><w:t>PowerPlusWaterMarkObject</w:t></w:r></w:p></w:hdr>",
            "<w:hdr><w:p><w:r><w:pict><v:shape id=\"other\" alt=\"PowerPlusWaterMarkObject\"><v:textpath string=\"x\"/></v:shape></w:pict></w:r></w:p></w:hdr>",
            "<w:hdr><w:p w:rsidR=\"PowerPlusWaterMarkObject\"><w:r><w:t>본문</w:t></w:r></w:p></w:hdr>",
        ] {
            let (text, rest) = split_watermark(xml);
            assert_eq!(text, None, "{xml}");
            assert_eq!(rest, xml);
        }
    }

    #[test]
    fn a_paragraph_with_attributes_and_a_hostile_text_are_handled() {
        let xml = format!(
            "<w:hdr>{}</w:hdr>",
            SHAPE
                .replace("<w:p>", "<w:p w:rsidR=\"00A1\">")
                .replace("초안 &amp; 검토", &"가".repeat(500))
        );
        let (text, rest) = split_watermark(&xml);
        assert_eq!(
            text.map(|t| t.chars().count()),
            Some(crate::rich_word_features::MAX_WATERMARK_CHARS),
            "a watermark text past its bound is cut"
        );
        assert_eq!(rest, "<w:hdr></w:hdr>");
    }
}

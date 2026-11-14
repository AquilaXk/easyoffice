//! Text that lives outside the main text flow: shape/text-box text (placed where the shape is
//! anchored) and footnotes, headers and footers (appended after the body under a heading).

use super::{reference, Flow, Reader};
use crate::pages_proto::{ArchiveObject, Message};
use crate::pages_schema::{footnote, header_footer, shape, ty};
use crate::{EasyOfficeError, RichBlock, RichParagraph, RichRun};

/// Appendix headings (heading level 2 paragraphs).
const FOOTNOTE_HEADING: &str = "각주";
const ENDNOTE_HEADING: &str = "미주";
const COMMENT_HEADING: &str = "댓글";
const HEADER_HEADING: &str = "머리글";
const FOOTER_HEADING: &str = "바닥글";
const APPENDIX_HEADING_LEVEL: u8 = 2;

fn non_empty(block: &RichBlock) -> bool {
    match block {
        RichBlock::Paragraph(p) => !p.plain_text().trim().is_empty(),
        _ => true,
    }
}

fn heading(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        heading_level: APPENDIX_HEADING_LEVEL,
        ..RichParagraph::default()
    })
}

impl Reader<'_> {
    /// Text blocks of the storage referenced by `field.1` of `obj`'s payload (paragraphs only).
    pub(super) fn nested_text(
        &mut self,
        payload: &Message<'_>,
        field: u32,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let Some(storage_id) = reference(payload, field) else {
            return Ok(Vec::new());
        };
        let Some(st) = self.storage_message(storage_id) else {
            return Ok(Vec::new());
        };
        let mut blocks = self.convert_storage(&st, Flow::Nested)?;
        blocks.retain(non_empty);
        Ok(blocks)
    }

    /// Paragraphs of a shape or text box, to be placed where it is anchored.
    pub(super) fn shape_blocks(
        &mut self,
        obj: &ArchiveObject,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let payload = Message::parse(&obj.payload)?;
        self.nested_text(&payload, shape::TEXT_STORAGE)
    }

    /// Footnote text in mark order, each led by its superscript number (the one shown at the
    /// mark in the body).
    fn footnote_blocks(&mut self, body: &Message<'_>) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let mut out = Vec::new();
        let marks = self.footnote_marks(body)?;
        self.note_numbering_rule(!marks.is_empty());
        for mark in marks {
            let Some(obj) = self.objects.get(mark.id) else {
                continue;
            };
            let payload = Message::parse(&obj.payload)?;
            let mut blocks = self.nested_text(&payload, footnote::TEXT_STORAGE)?;
            let mut lead = vec![self.footnote_number_run(mark.number)?, RichRun::plain(" ")];
            match blocks.first_mut() {
                Some(RichBlock::Paragraph(p)) => {
                    lead.append(&mut p.runs);
                    p.runs = lead;
                }
                _ => blocks.insert(
                    0,
                    RichBlock::Paragraph(RichParagraph {
                        runs: lead,
                        ..RichParagraph::default()
                    }),
                ),
            }
            out.extend(blocks);
        }
        Ok(out)
    }

    /// Distinct non-empty paragraphs of every header (or footer) storage, in object order.
    fn header_footer_blocks(&mut self, field: u32) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let mut ids: Vec<u64> = self
            .objects
            .objects
            .iter()
            .filter(|(_, o)| o.type_id == ty::HEADER_FOOTER_LIST)
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        let mut out: Vec<RichBlock> = Vec::new();
        for id in ids {
            let Some(obj) = self.objects.get(id) else {
                continue;
            };
            let list = Message::parse(&obj.payload)?;
            for entry in list.subs(field)? {
                let Some(storage_id) = entry.varint(1) else {
                    continue;
                };
                let Some(st) = self.storage_message(storage_id) else {
                    continue;
                };
                // Page-number fields and other marks in headers and footers are not reported as
                // skipped; pictures always are.
                self.quiet = true;
                let blocks = self.convert_storage(&st, Flow::Nested);
                self.quiet = false;
                for block in blocks? {
                    if non_empty(&block) && !out.contains(&block) {
                        out.push(block);
                    } else if let RichBlock::Image(image) = &block {
                        // The same logo repeats in every header/footer variant: count it once.
                        self.images.release(image);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Appendix after the body: footnotes, then header and footer text, each under a heading.
    pub(super) fn appendix_blocks(
        &mut self,
        body: &Message<'_>,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let note_heading = if self.settings.notes.endnotes {
            ENDNOTE_HEADING
        } else {
            FOOTNOTE_HEADING
        };
        let comments: Vec<RichBlock> = self
            .comment_texts(body)?
            .into_iter()
            .map(|text| {
                RichBlock::Paragraph(RichParagraph {
                    runs: vec![RichRun::plain(text)],
                    ..RichParagraph::default()
                })
            })
            .collect();
        let sections = [
            (note_heading, self.footnote_blocks(body)?),
            (
                HEADER_HEADING,
                self.header_footer_blocks(header_footer::HEADERS)?,
            ),
            (
                FOOTER_HEADING,
                self.header_footer_blocks(header_footer::FOOTERS)?,
            ),
            (COMMENT_HEADING, comments),
        ];
        let mut out = Vec::new();
        for (title, blocks) in sections {
            if !blocks.is_empty() {
                out.push(heading(title));
                out.extend(blocks);
            }
        }
        Ok(out)
    }
}

//! Text that lives outside the main text flow: shape/text-box text (placed where the shape is
//! anchored) and footnotes, headers, footers, comments and tracked changes (appended after the
//! body under a heading).

use super::{reference, Flow, Reader, MAX_PARAGRAPHS, MAX_RUNS};
use crate::pages_proto::{ArchiveObject, Message};
use crate::pages_schema::{footnote, header_footer, shape, storage};
use crate::{EasyOfficeError, RichBlock, RichParagraph, RichRun};

/// Appendix headings (heading level 2 paragraphs).
const FOOTNOTE_HEADING: &str = "각주";
const ENDNOTE_HEADING: &str = "미주";
const COMMENT_HEADING: &str = "댓글";
const CHANGE_HEADING: &str = "변경 추적";
const HEADER_HEADING: &str = "머리글";
const FOOTER_HEADING: &str = "바닥글";
const APPENDIX_HEADING_LEVEL: u8 = 2;

fn non_empty(block: &RichBlock) -> bool {
    match block {
        RichBlock::Paragraph(p) => !p.plain_text().trim().is_empty(),
        _ => true,
    }
}

/// Marks the paragraphs of a header or footer appendix as copies with their exact source text.
fn frame_copies(mut blocks: Vec<RichBlock>) -> Vec<RichBlock> {
    for block in &mut blocks {
        if let RichBlock::Paragraph(p) = block {
            p.frame_copy = Some(p.plain_text());
        }
    }
    blocks
}

fn heading(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        heading_level: APPENDIX_HEADING_LEVEL,
        ..RichParagraph::default()
    })
}

/// A converted nested storage and what converting it cost. Footnote marks and anchored shapes name
/// their storage by id, and a document may name one storage tens of thousands of times: the first
/// reading is kept, and a repeat is charged the same counters instead of converting again (PD-262).
#[derive(Clone)]
pub(super) struct NestedRead {
    blocks: Vec<RichBlock>,
    paragraphs: usize,
    runs: usize,
    text_bytes: usize,
    skipped: Vec<(String, usize)>,
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
        if let Some(read) = self.nested_reads.get(&(storage_id, self.quiet)).cloned() {
            return self.replay_nested(read);
        }
        let Some(st) = self.storage_message(storage_id) else {
            return Ok(Vec::new());
        };
        let before = (
            self.paragraphs,
            self.runs,
            self.text_budget.remaining(),
            self.skipped.clone(),
        );
        let mut blocks = self.convert_storage(&st, Flow::Nested)?;
        blocks.retain(non_empty);
        // Pictures are charged by their own budgets, so only text-only readings are kept.
        if blocks.iter().all(|b| matches!(b, RichBlock::Paragraph(_))) {
            let skipped = self
                .skipped
                .iter()
                .filter_map(|(kind, &n)| {
                    let new = n - before.3.get(kind).copied().unwrap_or(0);
                    (new > 0).then(|| (kind.clone(), new))
                })
                .collect();
            let read = NestedRead {
                blocks: blocks.clone(),
                paragraphs: self.paragraphs - before.0,
                runs: self.runs - before.1,
                text_bytes: before.2 - self.text_budget.remaining(),
                skipped,
            };
            self.nested_reads.insert((storage_id, self.quiet), read);
        }
        Ok(blocks)
    }

    /// Hands out a kept reading, charging the counters the conversion would have charged.
    fn replay_nested(&mut self, read: NestedRead) -> Result<Vec<RichBlock>, EasyOfficeError> {
        self.paragraphs += read.paragraphs;
        if self.paragraphs > MAX_PARAGRAPHS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 문단 수 한도 초과".to_string(),
            ));
        }
        self.runs += read.runs;
        if self.runs > MAX_RUNS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 글자 구간 수 한도 초과".to_string(),
            ));
        }
        self.text_budget
            .charge(read.text_bytes, "Pages 반복 참조 텍스트")?;
        for (kind, n) in read.skipped {
            *self.skipped.entry(kind).or_default() += n;
        }
        Ok(read.blocks)
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
        let body_text = body.text(storage::TEXT)?;
        let deleted = self.deleted_ranges(body, &body_text)?;
        let marks = self.footnote_marks(body, &deleted)?;
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

    /// Appendix after the body: footnotes, active headers/footers, comments and tracked changes.
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
        let changes = self.tracked_change_blocks(body)?;
        let sections = [
            (note_heading, self.footnote_blocks(body)?),
            (
                HEADER_HEADING,
                frame_copies(self.appendix_header_footer_blocks(body, header_footer::HEADERS)?),
            ),
            (
                FOOTER_HEADING,
                frame_copies(self.appendix_header_footer_blocks(body, header_footer::FOOTERS)?),
            ),
            (COMMENT_HEADING, comments),
            (CHANGE_HEADING, changes),
        ];
        let mut out = Vec::new();
        for (title, blocks) in sections {
            if blocks.is_empty() {
                continue;
            }
            let mut head = heading(title);
            // The heading of a header or footer section belongs to its copy, which starts with the paragraph that follows it.
            if let (RichBlock::Paragraph(h), Some(RichBlock::Paragraph(first))) =
                (&mut head, blocks.first())
            {
                if first.frame_copy.is_some() {
                    h.frame_copy = Some(h.plain_text());
                }
            }
            out.push(head);
            out.extend(blocks);
        }
        Ok(out)
    }
}

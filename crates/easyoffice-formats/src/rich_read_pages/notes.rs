//! Text that lives outside the main text flow: shape/text-box text (placed where the shape is
//! anchored) and footnotes, headers and footers (appended after the body under a heading).

use std::collections::HashSet;

use super::{object_kind, reference, AttrTable, Reader};
use crate::pages_proto::{ArchiveObject, Message};
use crate::pages_schema::{floating, footnote, header_footer, shape, storage, ty};
use crate::{EasyOfficeError, RichBlock, RichParagraph, RichRun};

/// Appendix headings (heading level 2 paragraphs).
const FOOTNOTE_HEADING: &str = "각주";
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
    fn nested_text(
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
        let mut blocks = self.convert_storage(&st, false)?;
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

    /// Reports drawables floating on pages (page-layout documents), which are not imported.
    /// Word-processing documents keep their objects anchored in the text and leave this list
    /// empty. Each drawable is counted once; an unreadable list is ignored.
    pub(super) fn note_floating_drawables(&mut self) {
        let mut seen = HashSet::new();
        for list in self.objects.objects.values() {
            if list.type_id != ty::FLOATING_DRAWABLES {
                continue;
            }
            let Ok(list) = Message::parse(&list.payload) else {
                continue;
            };
            for group in list.subs(floating::PAGE_GROUPS).unwrap_or_default() {
                for entry in group.subs(floating::GROUP_DRAWABLES).unwrap_or_default() {
                    let Some(id) = reference(&entry, 1) else {
                        continue;
                    };
                    if !seen.insert(id) {
                        continue;
                    }
                    match self.objects.get(id) {
                        Some(o) => self.skip(format!(
                            "쪽 위의 개체: {}(형식 {})",
                            object_kind(o.type_id),
                            o.type_id
                        )),
                        None => self.skip("쪽 위의 끊어진 개체 참조"),
                    }
                }
            }
        }
    }

    fn footnote_blocks(&mut self, body: &Message<'_>) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let table = AttrTable::parse(body, storage::FOOTNOTE_TABLE)?;
        let mut out = Vec::new();
        for id in table.entries.iter().filter_map(|(_, v)| *v) {
            let Some(obj) = self.object(id, ty::FOOTNOTE) else {
                continue;
            };
            let payload = Message::parse(&obj.payload)?;
            out.extend(self.nested_text(&payload, footnote::TEXT_STORAGE)?);
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
        // Page-number fields and other marks in headers and footers are not reported as skipped.
        let skipped_before = self.skipped.clone();
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
                for block in self.convert_storage(&st, false)? {
                    if non_empty(&block) && !out.contains(&block) {
                        out.push(block);
                    }
                }
            }
        }
        self.skipped = skipped_before;
        Ok(out)
    }

    /// Appendix after the body: footnotes, then header and footer text, each under a heading.
    pub(super) fn appendix_blocks(
        &mut self,
        body: &Message<'_>,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let sections = [
            (FOOTNOTE_HEADING, self.footnote_blocks(body)?),
            (
                HEADER_HEADING,
                self.header_footer_blocks(header_footer::HEADERS)?,
            ),
            (
                FOOTER_HEADING,
                self.header_footer_blocks(header_footer::FOOTERS)?,
            ),
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

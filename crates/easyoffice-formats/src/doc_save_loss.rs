//! What a document save leaves out for the formats that hold no review marks (PD-263): comments and tracked changes in HWPX,
//! RTF, Markdown, text and PDF, and the first-page header and footer in HWPX (its header control has no first-page variant).
//! The lines follow `pages_write_loss`: "<format> 저장에서 빠진 <what> <count>".

use crate::{DocumentFormat, RichBlock, RichDocument};

/// What a save to one format leaves out of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocSaveLoss {
    pub comments: usize,
    pub revisions: usize,
    pub first_page_frames: usize,
}

impl DocSaveLoss {
    /// The warnings of a save to `label`: one line per kind of loss, none when nothing is lost.
    pub fn warnings(&self, label: &str) -> Vec<String> {
        let mut out = Vec::new();
        if self.comments > 0 {
            out.push(format!("{label} 저장에서 빠진 메모 {}개", self.comments));
        }
        if self.revisions > 0 {
            out.push(format!(
                "{label} 저장에서 빠진 변경 추적 {}곳",
                self.revisions
            ));
        }
        if self.first_page_frames > 0 {
            out.push(format!(
                "{label} 저장에서 빠진 첫 쪽 머리글·바닥글 {}개",
                self.first_page_frames
            ));
        }
        out
    }
}

/// Counts what saving `doc` as `format` does not write; only the formats that cannot hold it lose anything.
pub fn doc_save_loss(doc: &RichDocument, format: DocumentFormat) -> DocSaveLoss {
    let marks_lost = matches!(
        format,
        DocumentFormat::Hwpx
            | DocumentFormat::Hwtx
            | DocumentFormat::Rtf
            | DocumentFormat::Markdown
            | DocumentFormat::Txt
            | DocumentFormat::Pdf
    );
    if !marks_lost {
        return DocSaveLoss::default();
    }
    let revisions = doc
        .blocks
        .iter()
        .filter_map(|block| match block {
            RichBlock::Paragraph(p) => Some(p.runs.iter().filter(|r| r.revision.is_some()).count()),
            _ => None,
        })
        .sum();
    let frame = &doc.page_frame;
    let first_page_frames = if matches!(format, DocumentFormat::Hwpx | DocumentFormat::Hwtx) {
        frame.first_header.len() + frame.first_footer.len()
    } else {
        0
    };
    DocSaveLoss {
        comments: doc.validated_comments().len(),
        revisions,
        first_page_frames,
    }
}

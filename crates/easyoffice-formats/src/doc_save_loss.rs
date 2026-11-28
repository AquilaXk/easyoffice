//! What a document save leaves out for the formats that hold no review marks (PD-263, PD-328):
//! comments and tracked changes in HWPX, DOC, RTF, Markdown, text and PDF, and the first-page
//! header and footer in HWPX and DOC. The lines follow `pages_write_loss`: "<format> 저장에서 빠진 <what> <count>".

use crate::{DocumentFormat, RichBlock, RichDocument};

/// What a save to one format leaves out of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocSaveLoss {
    pub comments: usize,
    pub revisions: usize,
    pub first_page_frames: usize,
    /// The Word-style items of PD-395 that only DOCX writes: the fields (date, time, cross reference, caption number), the
    /// bookmarks, the section breaks, the line numbers, the watermark, the repeated table header rows and the wrap, crop or
    /// rotation of a picture.
    pub word_features: usize,
    /// The paragraph and block features of PD-401 that only DOCX writes: tab stops, paragraph borders and shading, drop
    /// caps, column separators and breaks, text boxes, equations (their text stays) and the styles of the user.
    pub word_blocks: usize,
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
        if self.word_features > 0 {
            out.push(format!(
                "{label} 저장에서 빠진 필드·책갈피·구역 나누기·줄 번호·워터마크·표 머리글 행·그림 배치 {}곳",
                self.word_features
            ));
        }
        if self.word_blocks > 0 {
            out.push(format!(
                "{label} 저장에서 빠진 탭 정지·문단 테두리와 음영·첫 글자 장식·단 구분선과 단 나누기·텍스트 상자·수식·사용자 스타일 {}곳",
                self.word_blocks
            ));
        }
        out
    }
}

/// How many of the paragraph and block items of PD-401 a document holds.
fn word_blocks(doc: &RichDocument) -> usize {
    let in_text: usize = doc
        .blocks
        .iter()
        .map(|block| match block {
            RichBlock::Paragraph(p) => {
                let w = &p.word;
                usize::from(!w.tab_stops.is_empty())
                    + usize::from(w.borders.is_some() || w.shading.is_some())
                    + usize::from(w.drop_cap.is_some())
                    + usize::from(w.column_break_before || w.column_look.is_some())
                    + w.text_boxes.len()
                    + p.runs.iter().filter(|r| r.math.is_some()).count()
            }
            _ => 0,
        })
        .sum();
    in_text + doc.styles.len()
}

/// How many of the Word-style items of PD-395 a document holds.
fn word_features(doc: &RichDocument) -> usize {
    let in_text: usize = doc
        .blocks
        .iter()
        .map(|block| match block {
            RichBlock::Paragraph(p) => {
                usize::from(p.section_break.is_some())
                    + p.runs
                        .iter()
                        .filter(|r| r.field_code.is_some() || r.bookmark_start.is_some())
                        .count()
            }
            RichBlock::Table(t) => usize::from(t.header_rows > 0),
            RichBlock::Image(image) => usize::from(!image.layout().is_default()),
        })
        .sum();
    in_text
        + usize::from(doc.page_frame.line_numbers.is_some())
        + usize::from(doc.page_frame.watermark.is_some())
}

/// Counts what saving `doc` as `format` does not write; only the formats that cannot hold it lose anything.
pub fn doc_save_loss(doc: &RichDocument, format: DocumentFormat) -> DocSaveLoss {
    let marks_lost = matches!(
        format,
        DocumentFormat::Hwpx
            | DocumentFormat::Hwtx
            | DocumentFormat::Doc
            | DocumentFormat::Rtf
            | DocumentFormat::Markdown
            | DocumentFormat::Txt
            | DocumentFormat::Pdf
    );
    // DOCX and the formats that are not documents keep or never had them; every other document format writes none of them.
    let (word_features, word_blocks) = if matches!(
        format,
        DocumentFormat::Docx | DocumentFormat::Xlsx | DocumentFormat::Pptx
    ) {
        (0, 0)
    } else {
        (word_features(doc), word_blocks(doc))
    };
    if !marks_lost {
        return DocSaveLoss {
            word_features,
            word_blocks,
            ..DocSaveLoss::default()
        };
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
    let first_page_frames = if matches!(
        format,
        DocumentFormat::Hwpx | DocumentFormat::Hwtx | DocumentFormat::Doc
    ) {
        frame.first_header.len() + frame.first_footer.len()
    } else {
        0
    };
    DocSaveLoss {
        comments: doc.validated_comments().len(),
        revisions,
        first_page_frames,
        word_features,
        word_blocks,
    }
}

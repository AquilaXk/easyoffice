//! Two structured-save details of `convert_document_format` that sit beside the block parser:
//! the PDF Studio's page settings (`"pdf"` next to `blocks`) and the document form of a slide deck.

use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, PdfHighlight, PdfHighlightKind, PdfWriteOptions, RichBlock,
    RichDocument, RichListKind, RichParagraph, RichRun, RichSlideDeck, RichTable, SlideElement,
    SlidePlaceholderRole,
};

use crate::convert_structured::{field, invalid, opt_bounded, opt_str, JsonObject, SlideTriple};

/// Longest watermark or stamp text, in characters.
const MAX_PDF_LABEL_CHARS: usize = 200;
const MAX_ROTATION_DEGREES: i64 = 270;
const ROTATION_STEP_DEGREES: i64 = 90;
/// Heading level of a slide title in a document (the document title itself is the level-1 title).
const SLIDE_TITLE_HEADING_LEVEL: u8 = 2;

/// `{"rotationDegrees":0|90|180|270,"watermark":"…","stamp":"…","highlight":{"text":"…",
/// "kind":"highlight"|"underline","color":"#RRGGBB"}}`; every key is optional and an absent `pdf`
/// object writes no settings. Only a PDF target uses them.
pub(crate) fn parse_pdf_options(entries: &JsonObject) -> Result<PdfWriteOptions, EasyOfficeError> {
    let Some(value) = field(entries, "pdf") else {
        return Ok(PdfWriteOptions::default());
    };
    let obj = value
        .as_object()
        .ok_or_else(|| invalid("구조화 저장: 'pdf'는 객체여야 합니다."))?;
    let rotation = opt_bounded(obj, "rotationDegrees", 0, MAX_ROTATION_DEGREES)?.unwrap_or(0);
    if rotation % ROTATION_STEP_DEGREES != 0 {
        return Err(invalid(
            "구조화 저장: 'rotationDegrees'는 0, 90, 180, 270 중 하나여야 합니다.",
        ));
    }
    Ok(PdfWriteOptions {
        rotation_degrees: rotation as u16,
        watermark: pdf_label(obj, "watermark")?,
        approval_stamp: pdf_label(obj, "stamp")?,
        highlight: field(obj, "highlight")
            .map(|v| {
                parse_highlight(
                    v.as_object().ok_or_else(|| {
                        invalid("구조화 저장: 'pdf.highlight'는 객체여야 합니다.")
                    })?,
                )
            })
            .transpose()?,
    })
}

/// `{"text","kind","color"}`: a non-empty one-line phrase, `highlight` or `underline`, `#RRGGBB`.
fn parse_highlight(obj: &JsonObject) -> Result<PdfHighlight, EasyOfficeError> {
    let text = pdf_label(obj, "text")?
        .ok_or_else(|| invalid("구조화 저장: 'pdf.highlight.text'가 필요합니다."))?;
    let kind = match opt_str(obj, "kind")? {
        Some("highlight") => PdfHighlightKind::Highlight,
        Some("underline") => PdfHighlightKind::Underline,
        _ => {
            return Err(invalid(
                "구조화 저장: 'pdf.highlight.kind'는 highlight 또는 underline이어야 합니다.",
            ))
        }
    };
    let color = opt_str(obj, "color")?
        .and_then(|c| c.strip_prefix('#'))
        .filter(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8])
        .ok_or_else(|| invalid("구조화 저장: 'pdf.highlight.color'는 #RRGGBB여야 합니다."))?;
    Ok(PdfHighlight { text, kind, color })
}

/// A one-line label: at most [`MAX_PDF_LABEL_CHARS`] characters, no control characters.
fn pdf_label(obj: &JsonObject, key: &str) -> Result<Option<String>, EasyOfficeError> {
    match opt_str(obj, key)?.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(text)
            if text.chars().count() <= MAX_PDF_LABEL_CHARS
                && !text.chars().any(char::is_control) =>
        {
            Ok(Some(text.to_string()))
        }
        Some(_) => Err(invalid(format!(
            "구조화 저장: 'pdf.{key}'는 제어 문자 없는 {MAX_PDF_LABEL_CHARS}자 이하여야 합니다."
        ))),
    }
}

/// Targets that hold a slide deck as a document (the rest keep the deck or a table form).
pub(crate) fn is_document_format(format: DocumentFormat) -> bool {
    matches!(
        format,
        DocumentFormat::Docx
            | DocumentFormat::Doc
            | DocumentFormat::Odt
            | DocumentFormat::Rtf
            | DocumentFormat::Markdown
            | DocumentFormat::Txt
            | DocumentFormat::Hwp
            | DocumentFormat::Hwt
            | DocumentFormat::Hwpx
            | DocumentFormat::Hwtx
            | DocumentFormat::Pdf
    )
}

/// A deck as a document: each slide title a heading, each bullet a bullet item, and the speaker
/// notes an italic paragraph.
pub(crate) fn slides_to_document(slides: &[SlideTriple], title: &str) -> RichDocument {
    let mut blocks = Vec::new();
    let mut push = |text: &str, heading_level: u8, list: Option<RichListKind>, italic: bool| {
        if text.trim().is_empty() {
            return;
        }
        blocks.push(RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun {
                italic,
                ..RichRun::plain(text)
            }],
            heading_level,
            list,
            ..RichParagraph::default()
        }));
    };
    for (slide_title, bullets, notes) in slides {
        push(slide_title, SLIDE_TITLE_HEADING_LEVEL, None, false);
        for bullet in bullets {
            push(bullet, 0, Some(RichListKind::Bullet), false);
        }
        push(notes, 0, None, true);
    }
    RichDocument {
        page_frame: Default::default(),
        title: title.to_string(),
        blocks,
        official_agency: None,
        ..RichDocument::default()
    }
}

pub(crate) fn slide_deck_to_document(deck: &RichSlideDeck, title: &str) -> RichDocument {
    let mut blocks = Vec::new();
    let push_line = |blocks: &mut Vec<RichBlock>,
                     text: &str,
                     heading_level: u8,
                     list: Option<RichListKind>,
                     italic: bool| {
        if text.trim().is_empty() {
            return;
        }
        blocks.push(RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun {
                italic,
                ..RichRun::plain(text)
            }],
            heading_level,
            list,
            ..RichParagraph::default()
        }));
    };
    for slide in &deck.slides {
        push_line(
            &mut blocks,
            &slide.title,
            SLIDE_TITLE_HEADING_LEVEL,
            None,
            false,
        );
        for bullet in &slide.bullets {
            push_line(&mut blocks, bullet, 0, Some(RichListKind::Bullet), false);
        }
        for el in &slide.elements {
            match el {
                SlideElement::TextBox {
                    text,
                    bold,
                    italic,
                    underline,
                    color,
                    role,
                    ..
                } if !text.trim().is_empty() => {
                    let hl = if slide.title.is_empty() && *role == Some(SlidePlaceholderRole::Title)
                    {
                        SLIDE_TITLE_HEADING_LEVEL
                    } else {
                        0
                    };
                    blocks.push(RichBlock::Paragraph(RichParagraph {
                        runs: vec![RichRun {
                            bold: *bold,
                            italic: *italic,
                            underline: *underline,
                            color: color.clone(),
                            ..RichRun::plain(text)
                        }],
                        heading_level: hl,
                        ..RichParagraph::default()
                    }));
                }
                SlideElement::Shape {
                    text,
                    bold,
                    italic,
                    underline,
                    color,
                    ..
                } if !text.trim().is_empty() => {
                    blocks.push(RichBlock::Paragraph(RichParagraph {
                        runs: vec![RichRun {
                            bold: *bold,
                            italic: *italic,
                            underline: *underline,
                            color: color.clone(),
                            ..RichRun::plain(text)
                        }],
                        ..RichParagraph::default()
                    }));
                }
                SlideElement::Image { image, .. } => {
                    blocks.push(RichBlock::Image(image.clone()));
                }
                SlideElement::Table { rows, .. } => {
                    blocks.push(RichBlock::Table(RichTable::from(rows.clone())));
                }
                SlideElement::Chart {
                    title: ct,
                    categories,
                    series,
                    ..
                } => {
                    if !ct.trim().is_empty() {
                        push_line(&mut blocks, ct, 0, None, false);
                    }
                    let mut grid = Vec::new();
                    let mut hdr = vec!["항목".to_string()];
                    hdr.extend(series.iter().map(|s| s.name.clone()));
                    grid.push(hdr);
                    let len = categories
                        .len()
                        .max(series.iter().map(|s| s.values.len()).max().unwrap_or(0));
                    for i in 0..len {
                        let mut r = vec![categories.get(i).cloned().unwrap_or_default()];
                        for s in series {
                            r.push(s.values.get(i).map(|v| format!("{v}")).unwrap_or_default());
                        }
                        grid.push(r);
                    }
                    blocks.push(RichBlock::Table(RichTable::from(grid)));
                }
                _ => {}
            }
        }
        push_line(&mut blocks, &slide.notes, 0, None, true);
    }
    RichDocument {
        page_frame: Default::default(),
        title: title.to_string(),
        blocks,
        official_agency: None,
        ..RichDocument::default()
    }
}

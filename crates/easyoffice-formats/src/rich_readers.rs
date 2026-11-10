//! Structured decode of real files (PD-075): the same shape the WebUI saves (`blocks`, `sheets`,
//! `slides`), so open -> edit -> save round-trips.
//!
//! Every ZIP-based path goes through [`crate::ZipArchiveCodec::unpack`] with the default
//! [`crate::ZipSecurityPolicy`] (entry count, size and ratio ceilings, path traversal) and
//! [`crate::XmlSanitizer::validate_safe_xml`] (DOCTYPE/entity/XXE) before any content is read.

use crate::rich_read_hwpx::read_hwpx;
use crate::rich_read_odt::read_odt;
use crate::rich_read_ooxml::read_docx;
use crate::rich_read_pages::read_pages;
use crate::rich_read_rtf::read_rtf;
use crate::rich_read_text::{read_markdown, read_plain_text};
use crate::{
    ContainerFamily, CsvCodec, DocumentFormat, EasyOfficeError, FormatDetector, LegacyOle2Codec,
    OdfCodec, OdsUnpacked, OoxmlCodec, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter,
};

/// Slide as `(title, bullets, notes)`.
pub type DecodedSlide = (String, Vec<String>, String);

/// Decoded editor content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedContent {
    /// Ordered paragraphs and tables (DOC/HWP/PDF modules).
    Document(RichDocument),
    /// Every sheet with its cells as zero-based `(row, col, raw)`; formulas start with `=`.
    Workbook(Vec<OdsUnpacked>),
    /// Slides (SLIDE module).
    Slides(Vec<DecodedSlide>),
}

/// Result of [`UniversalFormatConverter::decode_rich`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedDocument {
    pub format: DocumentFormat,
    pub title: String,
    pub content: DecodedContent,
    /// What the structured model cannot carry from this file (shown to the user).
    pub warnings: Vec<String>,
}

impl DocumentFormat {
    /// Workspace module that edits this format: `DOC`, `HWP`, `SHEET`, `SLIDE`, or `PDF`.
    pub fn editor_module(&self) -> &'static str {
        match self {
            Self::Docx
            | Self::Doc
            | Self::Odt
            | Self::Rtf
            | Self::Markdown
            | Self::Txt
            | Self::Pages => "DOC",
            Self::Hwp | Self::Hwpx | Self::Hwt | Self::Hwtx => "HWP",
            Self::Xlsx | Self::Xls | Self::Ods | Self::Csv | Self::Tsv => "SHEET",
            Self::Pptx | Self::Ppt | Self::Ppsx | Self::Odp => "SLIDE",
            Self::Pdf => "PDF",
        }
    }
}

fn extension_of(hint: &str) -> Option<&str> {
    let name = hint.rsplit(['/', '\\']).next().unwrap_or(hint);
    match name.rsplit_once('.') {
        Some((_, ext)) => Some(ext),
        None if DocumentFormat::from_extension(name).is_some() => Some(name),
        None => None,
    }
}

/// Magic bytes decide for binary containers; the extension only chooses among text formats.
fn detect_for_open(bytes: &[u8], hint: Option<&str>) -> Result<DocumentFormat, EasyOfficeError> {
    let ext = hint
        .and_then(extension_of)
        .map(str::trim)
        .filter(|e| !e.is_empty());
    let by_magic = FormatDetector::detect(bytes, None)?;
    let Some(ext) = ext else {
        return Ok(by_magic);
    };
    let Some(by_ext) = DocumentFormat::from_extension(ext) else {
        return Err(EasyOfficeError::UnsupportedExtension(ext.to_string()));
    };
    if by_magic.container_family() != ContainerFamily::PlainTextUtf8 {
        return Ok(by_magic);
    }
    if by_magic == DocumentFormat::Rtf {
        return Ok(DocumentFormat::Rtf);
    }
    // Plain bytes: a binary extension must fail with the detector's signature error.
    FormatDetector::detect(bytes, Some(ext)).map(|_| by_ext)
}

fn flat_document(
    title: String,
    paragraphs: Vec<String>,
    tables: Vec<Vec<Vec<String>>>,
) -> RichDocument {
    let mut blocks: Vec<RichBlock> = paragraphs
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(p)],
                ..RichParagraph::default()
            })
        })
        .collect();
    blocks.extend(
        tables
            .into_iter()
            .filter(|t| !t.is_empty())
            .map(|rows| RichBlock::Table(rows.into())),
    );
    RichDocument {
        title,
        blocks,
        official_agency: None,
    }
}

fn normalize_sheets(mut sheets: Vec<OdsUnpacked>) -> Vec<OdsUnpacked> {
    for (name, cells) in &mut sheets {
        if name.trim().is_empty() {
            *name = "Sheet1".to_string();
        }
        cells.retain(|(_, _, raw)| !raw.is_empty());
        let is_strictly_sorted = cells
            .windows(2)
            .all(|w| (w[0].0, w[0].1) < (w[1].0, w[1].1));
        if !is_strictly_sorted {
            cells.sort_by_key(|(r, c, _)| (*r, *c));
            cells.dedup_by_key(|(r, c, _)| (*r, *c));
        }
    }
    sheets
}

fn read_delimited(bytes: &[u8], delim: char) -> Result<Vec<OdsUnpacked>, EasyOfficeError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| EasyOfficeError::InvalidUtf8("CSV/TSV UTF-8 디코딩 오류".to_string()))?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut cells = Vec::new();
    CsvCodec::visit_fields_swar(text, delim, |r, c, val| {
        if !val.is_empty() {
            cells.push((r as u32, c as u32, val.into_owned()));
        }
    })?;
    Ok(vec![("Sheet1".to_string(), cells)])
}

const NOTE_FLAT: &str =
    "서식(굵게·정렬 등)과 표 위치는 가져오지 않고 본문 텍스트 뒤에 표를 순서대로 붙입니다.";
const NOTE_SHEET: &str =
    "셀 서식·열 너비·차트·그림은 가져오지 않습니다. 수식 결과는 다시 계산하며 날짜는 일련번호로 표시됩니다.";
const NOTE_SHEET_STYLED: &str =
    "굵게·기울임·밑줄·취소선·글자 크기와 열 너비는 가져오지만 글꼴 이름·색·채우기·테두리·정렬·숫자 서식·차트·그림은 가져오지 않습니다. 수식 결과는 다시 계산하며 날짜는 일련번호로 표시됩니다.";
const NOTE_SLIDE: &str =
    "슬라이드 제목·글머리·발표자 노트만 가져오며 도형·이미지·서식은 제외됩니다.";

impl UniversalFormatConverter {
    /// Decodes `bytes` into the structured editor model. `file_name_hint` (a file name or bare
    /// extension) only disambiguates text formats; binary containers are identified by magic.
    pub fn decode_rich(
        bytes: &[u8],
        file_name_hint: Option<&str>,
    ) -> Result<DecodedDocument, EasyOfficeError> {
        Ok(Self::decode_rich_inner(bytes, file_name_hint, false)?.0)
    }

    /// Decodes `bytes` into the structured editor model together with per-sheet styling in a
    /// single archive/XML pass for XLSX and ODS containers.
    #[allow(clippy::type_complexity)]
    pub fn decode_rich_with_styling(
        bytes: &[u8],
        file_name_hint: Option<&str>,
    ) -> Result<(DecodedDocument, Vec<(String, crate::SheetStyling)>), EasyOfficeError> {
        Self::decode_rich_inner(bytes, file_name_hint, true)
    }

    #[allow(clippy::type_complexity)]
    fn decode_rich_inner(
        bytes: &[u8],
        file_name_hint: Option<&str>,
        with_styling: bool,
    ) -> Result<(DecodedDocument, Vec<(String, crate::SheetStyling)>), EasyOfficeError> {
        let format = detect_for_open(bytes, file_name_hint)?;
        let mut warnings: Vec<String> = Vec::new();
        let mut styling: Vec<(String, crate::SheetStyling)> = Vec::new();
        let content = match format {
            DocumentFormat::Docx => {
                let (doc, w) = read_docx(bytes)?;
                warnings = w;
                DecodedContent::Document(doc)
            }
            DocumentFormat::Hwpx | DocumentFormat::Hwtx => {
                let (doc, w) = read_hwpx(bytes)?;
                warnings = w;
                DecodedContent::Document(doc)
            }
            DocumentFormat::Pages => {
                let (doc, w) = read_pages(bytes)?;
                warnings = w;
                DecodedContent::Document(doc)
            }
            DocumentFormat::Odt => {
                let (doc, w) = read_odt(bytes)?;
                warnings = w;
                DecodedContent::Document(doc)
            }
            DocumentFormat::Markdown | DocumentFormat::Txt => {
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("텍스트 UTF-8 디코딩 오류".to_string())
                })?;
                let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
                let blocks = if format == DocumentFormat::Markdown {
                    read_markdown(text)?
                } else {
                    read_plain_text(text)?
                };
                DecodedContent::Document(RichDocument {
                    title: String::new(),
                    blocks,
                    official_agency: None,
                })
            }
            DocumentFormat::Rtf => {
                let (doc, w) = read_rtf(bytes)?;
                warnings = w;
                DecodedContent::Document(doc)
            }
            DocumentFormat::Doc
            | DocumentFormat::Hwp
            | DocumentFormat::Hwt
            | DocumentFormat::Pdf => {
                let (_, m) = Self::decode(bytes, Some(format.extension()))?;
                let (title, paragraphs, tables, agency) =
                    (m.title, m.paragraphs, m.tables, m.official_agency);
                warnings.push(if format == DocumentFormat::Pdf {
                    "PDF는 텍스트만 추출합니다(레이아웃·그림·서식 제외).".to_string()
                } else {
                    NOTE_FLAT.to_string()
                });
                let mut doc = flat_document(title, paragraphs, tables);
                doc.official_agency = agency;
                DecodedContent::Document(doc)
            }
            DocumentFormat::Xlsx => {
                warnings.push(NOTE_SHEET_STYLED.to_string());
                let sheets = if with_styling {
                    let (s, st) = OoxmlCodec::unpack_xlsx_with_styling(bytes)?;
                    styling = st;
                    s
                } else {
                    OoxmlCodec::unpack_xlsx_multi(bytes)?
                };
                DecodedContent::Workbook(normalize_sheets(sheets))
            }
            DocumentFormat::Xls => {
                warnings.push(NOTE_SHEET.to_string());
                let sheets = LegacyOle2Codec::unpack_xls_multi(bytes)?
                    .into_iter()
                    .map(|s| (s.sheet_name, s.cells))
                    .collect();
                DecodedContent::Workbook(normalize_sheets(sheets))
            }
            DocumentFormat::Ods => {
                warnings.push(NOTE_SHEET_STYLED.to_string());
                let sheets = if with_styling {
                    let (s, st) = OdfCodec::unpack_ods_with_styling(bytes)?;
                    styling = st;
                    s
                } else {
                    OdfCodec::unpack_ods_multi(bytes)?
                };
                DecodedContent::Workbook(normalize_sheets(sheets))
            }
            DocumentFormat::Csv | DocumentFormat::Tsv => {
                let delim = if format == DocumentFormat::Csv {
                    ','
                } else {
                    '\t'
                };
                DecodedContent::Workbook(normalize_sheets(read_delimited(bytes, delim)?))
            }
            DocumentFormat::Pptx | DocumentFormat::Ppsx => {
                warnings.push(NOTE_SLIDE.to_string());
                DecodedContent::Slides(OoxmlCodec::unpack_pptx(bytes)?.slides)
            }
            DocumentFormat::Ppt => {
                warnings.push(NOTE_SLIDE.to_string());
                let p = LegacyOle2Codec::unpack_ppt(bytes)?;
                DecodedContent::Slides(
                    p.slides
                        .into_iter()
                        .map(|s| (s.title, s.bullets, s.notes))
                        .collect(),
                )
            }
            DocumentFormat::Odp => {
                warnings.push(NOTE_SLIDE.to_string());
                DecodedContent::Slides(OdfCodec::unpack_odp(bytes)?.1)
            }
        };
        let title = match &content {
            DecodedContent::Document(d) => d.title.clone(),
            DecodedContent::Workbook(s) => s.first().map(|(n, _)| n.clone()).unwrap_or_default(),
            DecodedContent::Slides(s) => s.first().map(|(t, _, _)| t.clone()).unwrap_or_default(),
        };
        Ok((
            DecodedDocument {
                format,
                title,
                content,
                warnings,
            },
            styling,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_delimited_strips_utf8_bom_and_skips_empty_cells_via_swar() {
        let csv = "\u{FEFF}이름,,점수\n홍길동,,95\n";
        let sheets = read_delimited(csv.as_bytes(), ',').unwrap();
        assert_eq!(sheets.len(), 1);
        assert_eq!(
            sheets[0].1,
            vec![
                (0, 0, "이름".to_string()),
                (0, 2, "점수".to_string()),
                (1, 0, "홍길동".to_string()),
                (1, 2, "95".to_string()),
            ]
        );
        let unsorted = vec![(
            "  ".to_string(),
            vec![
                (1, 0, "B".to_string()),
                (0, 0, "A".to_string()),
                (0, 0, "DUP".to_string()),
                (0, 1, String::new()),
            ],
        )];
        let norm = normalize_sheets(unsorted);
        assert_eq!(norm[0].0, "Sheet1");
        assert_eq!(
            norm[0].1,
            vec![(0, 0, "A".to_string()), (1, 0, "B".to_string())]
        );
    }
}

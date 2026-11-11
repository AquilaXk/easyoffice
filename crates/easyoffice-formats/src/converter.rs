use crate::text_decode::decode_text;
use crate::{
    CommonMarkCodec, CsvCodec, DocumentFormat, EasyOfficeError, FormatDetector, Hwp5Codec,
    Hwp5DocumentPayload, Hwp5ParagraphRecord, HwpxCodec, HwpxPayload, LegacyDocPayload,
    LegacyOle2Codec, LegacyPptPayload, LegacyPptSlide, LegacyXlsPayload, MarkdownBlock, OdfCodec,
    OoxmlCodec, OoxmlDocxPayload, OoxmlPptxPayload, PdfDocumentModel, PdfPageModel, PdfSerializer,
    RtfCodec, RtfPayload, XlsxFormulaCache,
};

/// Canonical intermediate representation bridging all 20 supported document formats.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UniversalDocumentModel {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
    pub sheet_cells: Vec<(u32, u32, String)>,
    pub slides: Vec<(String, Vec<String>, String)>,
    pub official_agency: Option<String>,
}

impl UniversalDocumentModel {
    fn effective_paragraphs(&self) -> Vec<String> {
        if !self.paragraphs.is_empty() {
            return self.paragraphs.clone();
        }
        if !self.slides.is_empty() {
            let mut out = Vec::new();
            for (s_title, bullets, notes) in &self.slides {
                if !s_title.is_empty() {
                    out.push(s_title.clone());
                }
                out.extend(bullets.iter().cloned());
                if !notes.is_empty() {
                    out.push(notes.clone());
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        if !self.sheet_cells.is_empty() {
            return self.sheet_cells.iter().map(|(_, _, v)| v.clone()).collect();
        }
        vec![self.title.clone()]
    }

    /// Sheet cells that `encode` writes for spreadsheet targets (derived from paragraphs/tables
    /// when the model has no explicit cells).
    pub fn effective_sheet_cells(&self) -> Vec<(u32, u32, String)> {
        if !self.sheet_cells.is_empty() {
            return self.sheet_cells.clone();
        }
        let eff_paragraphs = if !self.paragraphs.is_empty() {
            self.paragraphs.clone()
        } else if !self.tables.is_empty() {
            Vec::new()
        } else {
            self.effective_paragraphs()
        };
        doc_to_sheet_cells(&eff_paragraphs, &self.tables)
    }

    fn effective_slides(&self) -> Vec<(String, Vec<String>, String)> {
        if !self.slides.is_empty() {
            return self.slides.clone();
        }
        vec![(
            self.title.clone(),
            self.effective_paragraphs(),
            String::new(),
        )]
    }
}

/// Lossless cross-format encoder, decoder, and converter for all 20 `DocumentFormat::ALL` formats.
pub struct UniversalFormatConverter;

/// Targets whose rich writer keeps block order and character runs (see `encode_rich`).
const RICH_ORDER_TARGETS: [DocumentFormat; 9] = [
    DocumentFormat::Docx,
    DocumentFormat::Hwpx,
    DocumentFormat::Hwtx,
    DocumentFormat::Odt,
    DocumentFormat::Rtf,
    DocumentFormat::Markdown,
    DocumentFormat::Txt,
    DocumentFormat::Pdf,
    DocumentFormat::Pages,
];

impl UniversalFormatConverter {
    pub fn encode(
        model: &UniversalDocumentModel,
        format: DocumentFormat,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::encode_with_formula_cache(model, format, None)
    }

    /// Like [`Self::encode`], but XLSX output stores `formula_cache` (computed results keyed by
    /// zero-based `(row, col)` of [`UniversalDocumentModel::effective_sheet_cells`]) as the cached
    /// value of each formula cell.
    pub fn encode_with_formula_cache(
        model: &UniversalDocumentModel,
        format: DocumentFormat,
        formula_cache: Option<&XlsxFormulaCache>,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let title = if model.title.trim().is_empty() {
            "문서".to_string()
        } else {
            model.title.clone()
        };
        let paragraphs = model.effective_paragraphs();
        let sheet_cells = model.effective_sheet_cells();
        let slides = model.effective_slides();

        match format {
            DocumentFormat::Pages => Err(EasyOfficeError::InvalidArgument(
                "Pages(.pages) 저장은 본문·제목·기본 서식을 가진 문서 내용(블록)만 지원합니다. 시트·슬라이드·단순 본문은 쓸 수 없습니다.".to_string(),
            )),
            DocumentFormat::Docx => OoxmlCodec::pack_docx(&OoxmlDocxPayload {
                title,
                paragraphs,
                tables: model.tables.clone(),
            }),
            DocumentFormat::Doc => LegacyOle2Codec::pack_doc(&LegacyDocPayload {
                title,
                paragraphs,
                tables: model.tables.clone(),
            }),
            DocumentFormat::Odt => OdfCodec::pack_odt(&title, &paragraphs, &model.tables),
            DocumentFormat::Rtf => Ok(RtfCodec::encode(&RtfPayload {
                title,
                paragraphs,
                tables: model.tables.clone(),
            })),
            DocumentFormat::Markdown => {
                let mut blocks = vec![MarkdownBlock::Heading {
                    level: 1,
                    text: title,
                }];
                for p in paragraphs {
                    blocks.push(MarkdownBlock::Paragraph(p));
                }
                for table in &model.tables {
                    if !table.is_empty() {
                        blocks.push(MarkdownBlock::Table(table.clone()));
                    }
                }
                Ok(CommonMarkCodec::render(&blocks).into_bytes())
            }
            DocumentFormat::Txt => {
                let mut lines = vec![title];
                lines.extend(paragraphs);
                Ok(lines.join("\n").into_bytes())
            }
            DocumentFormat::Hwp | DocumentFormat::Hwt => {
                let extra_tables = if model.tables.len() > 1 {
                    &model.tables[1..]
                } else {
                    &[]
                };
                Hwp5Codec::pack_multi_tables(
                    &Hwp5DocumentPayload {
                        version: (5, 0, 3, 0),
                        is_template: format == DocumentFormat::Hwt,
                        agency_name: model
                            .official_agency
                            .clone()
                            .unwrap_or_else(|| title.clone()),
                        document_number: title,
                        paragraphs: paragraphs
                            .into_iter()
                            .enumerate()
                            .map(|(idx, text)| Hwp5ParagraphRecord {
                                text,
                                style_name: if idx == 0 {
                                    "표제".to_string()
                                } else {
                                    "본문".to_string()
                                },
                                is_official_header: idx == 0 && model.official_agency.is_some(),
                            })
                            .collect(),
                        table_cells: model.tables.first().cloned().unwrap_or_default(),
                    },
                    extra_tables,
                )
            }
            DocumentFormat::Hwpx | DocumentFormat::Hwtx => HwpxCodec::pack(&HwpxPayload {
                title,
                is_template: format == DocumentFormat::Hwtx,
                official_agency: model.official_agency.clone(),
                approval_lines: vec!["기안".to_string(), "검토".to_string(), "결재".to_string()],
                paragraphs,
                tables: model.tables.clone(),
            }),
            DocumentFormat::Xlsx => OoxmlCodec::pack_xlsx_multi_with_formula_cache(
                &[(title, sheet_cells)],
                formula_cache.map(std::slice::from_ref).unwrap_or_default(),
            ),
            DocumentFormat::Xls => {
                let payload = LegacyXlsPayload {
                    sheet_name: title,
                    cells: sheet_cells,
                };
                // Cells derived from document text are never formulas, even when they start
                // with '='; only a model that carries explicit sheet cells keeps formulas.
                if model.sheet_cells.is_empty() {
                    LegacyOle2Codec::pack_xls_multi_values(std::slice::from_ref(&payload))
                } else {
                    LegacyOle2Codec::pack_xls(&payload)
                }
            }
            DocumentFormat::Ods => OdfCodec::pack_ods_multi_styled_with_cache(
                &[(title, sheet_cells)],
                formula_cache.map(std::slice::from_ref).unwrap_or_default(),
                &[],
            ),
            DocumentFormat::Csv | DocumentFormat::Tsv => {
                let delim = if format == DocumentFormat::Csv {
                    ','
                } else {
                    '\t'
                };
                let rows = cells_to_grid(&sheet_cells, &title)?;
                Ok(CsvCodec::serialize_with_bom(&rows, delim))
            }
            DocumentFormat::Pptx | DocumentFormat::Ppsx => {
                OoxmlCodec::pack_pptx(&OoxmlPptxPayload {
                    title,
                    is_slideshow: format == DocumentFormat::Ppsx,
                    slides,
                })
            }
            DocumentFormat::Ppt => LegacyOle2Codec::pack_ppt(&LegacyPptPayload {
                title,
                slides: slides
                    .into_iter()
                    .map(|(s_title, bullets, notes)| LegacyPptSlide {
                        title: s_title,
                        bullets,
                        notes,
                        theme: "SignatureLavender".to_string(),
                        transition: "Fade".to_string(),
                    })
                    .collect(),
            }),
            DocumentFormat::Odp => OdfCodec::pack_odp(&title, &slides),
            DocumentFormat::Pdf => {
                let mut body_lines = paragraphs;
                for table in &model.tables {
                    for row in table {
                        if !row.is_empty() {
                            body_lines.push(row.join(" | "));
                        }
                    }
                }
                Ok(PdfSerializer::serialize(&PdfDocumentModel {
                    title,
                    pages: vec![PdfPageModel {
                        page_number: 1,
                        rotation_degrees: 0,
                        body_text: body_lines.join("\n"),
                        approval_stamp: model.official_agency.clone(),
                    }],
                    watermark_text: None,
                }))
            }
        }
    }

    pub fn decode(
        bytes: &[u8],
        format_hint: Option<&str>,
    ) -> Result<(DocumentFormat, UniversalDocumentModel), EasyOfficeError> {
        let format = FormatDetector::detect(bytes, format_hint)?;
        let model = match format {
            DocumentFormat::Pages => Self::decode_pages(bytes)?,
            DocumentFormat::Docx => {
                let p = OoxmlCodec::unpack_docx(bytes)?;
                Self::from_doc(p.title, p.paragraphs, p.tables, None)
            }
            DocumentFormat::Doc => {
                let p = LegacyOle2Codec::unpack_doc(bytes)?;
                Self::from_doc(p.title, p.paragraphs, p.tables, None)
            }
            DocumentFormat::Odt => {
                let (title, paragraphs, tables) = OdfCodec::unpack_odt(bytes)?;
                Self::from_doc(title, paragraphs, tables, None)
            }
            DocumentFormat::Rtf => {
                let p = RtfCodec::decode(bytes)?;
                Self::from_doc(p.title, p.paragraphs, p.tables, None)
            }
            DocumentFormat::Markdown => Self::decode_markdown(bytes)?,
            DocumentFormat::Txt => Self::decode_txt(bytes)?,
            DocumentFormat::Hwp | DocumentFormat::Hwt => {
                let (p, tables) = Hwp5Codec::unpack_multi_tables(bytes)?;
                let paragraphs = p.paragraphs.into_iter().map(|r| r.text).collect();
                let agency = if p.agency_name.is_empty() {
                    None
                } else {
                    Some(p.agency_name)
                };
                Self::from_doc(p.document_number, paragraphs, tables, agency)
            }
            DocumentFormat::Hwpx | DocumentFormat::Hwtx => {
                let p = HwpxCodec::unpack(bytes)?;
                Self::from_doc(p.title, p.paragraphs, p.tables, p.official_agency)
            }
            DocumentFormat::Xlsx => {
                let sheets = OoxmlCodec::unpack_xlsx_multi(bytes)?;
                Self::from_sheets(sheets)
            }
            DocumentFormat::Xls => {
                let sheets = LegacyOle2Codec::unpack_xls_multi(bytes)?
                    .into_iter()
                    .map(|s| (s.sheet_name, s.cells))
                    .collect();
                Self::from_sheets(sheets)
            }
            DocumentFormat::Ods => {
                let sheets = OdfCodec::unpack_ods_multi(bytes)?;
                Self::from_sheets(sheets)
            }
            DocumentFormat::Csv | DocumentFormat::Tsv => Self::decode_delimited(bytes, format)?,
            DocumentFormat::Pptx | DocumentFormat::Ppsx => {
                let p = OoxmlCodec::unpack_pptx(bytes)?;
                Self::from_slides(p.title, p.slides)
            }
            DocumentFormat::Ppt => {
                let p = LegacyOle2Codec::unpack_ppt(bytes)?;
                let slides = p
                    .slides
                    .into_iter()
                    .map(|s| (s.title, s.bullets, s.notes))
                    .collect();
                Self::from_slides(p.title, slides)
            }
            DocumentFormat::Odp => {
                let (title, slides) = OdfCodec::unpack_odp(bytes)?;
                Self::from_slides(title, slides)
            }
            DocumentFormat::Pdf => Self::decode_pdf(bytes)?,
        };
        Ok((format, model))
    }

    fn decode_pages(bytes: &[u8]) -> Result<UniversalDocumentModel, EasyOfficeError> {
        let doc = Self::decode_rich(bytes, Some("pages"))?;
        match doc.content {
            crate::DecodedContent::Document(rich) => Ok(rich.to_universal_model()),
            _ => Err(EasyOfficeError::CorruptedContainer(
                "Pages 문서 본문을 읽지 못했습니다.".to_string(),
            )),
        }
    }

    fn decode_markdown(bytes: &[u8]) -> Result<UniversalDocumentModel, EasyOfficeError> {
        let text = decode_text(bytes, "Markdown")?;
        let blocks = CommonMarkCodec::parse(&text)?;
        let mut title = "Markdown 문서".to_string();
        let mut paragraphs = Vec::new();
        let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
        for block in blocks {
            match block {
                MarkdownBlock::Heading { level: 1, text }
                    if paragraphs.is_empty() && tables.is_empty() =>
                {
                    title = text;
                }
                MarkdownBlock::Heading { text, .. }
                | MarkdownBlock::Paragraph(text)
                | MarkdownBlock::BlockQuote(text) => paragraphs.push(text),
                MarkdownBlock::BulletList(items) | MarkdownBlock::NumberedList(items) => {
                    paragraphs.extend(items);
                }
                MarkdownBlock::Table(rows) if !rows.is_empty() => tables.push(rows),
                MarkdownBlock::Table(_) => {}
            }
        }
        Ok(Self::from_doc(title, paragraphs, tables, None))
    }

    fn decode_txt(bytes: &[u8]) -> Result<UniversalDocumentModel, EasyOfficeError> {
        let text = decode_text(bytes, "TXT")?;
        let mut lines: Vec<String> = text.lines().map(ToString::to_string).collect();
        let title = if lines.is_empty() {
            "텍스트 문서".to_string()
        } else {
            lines.remove(0)
        };
        if lines.is_empty() {
            lines.push(title.clone());
        }
        Ok(Self::from_doc(title, lines, Vec::new(), None))
    }

    fn decode_delimited(
        bytes: &[u8],
        format: DocumentFormat,
    ) -> Result<UniversalDocumentModel, EasyOfficeError> {
        let delim = if format == DocumentFormat::Csv {
            ','
        } else {
            '\t'
        };
        let text = decode_text(bytes, "CSV/TSV")?;
        let rows = CsvCodec::parse(&text, delim)?;
        let mut cells = Vec::new();
        for (r_idx, row) in rows.iter().enumerate() {
            for (c_idx, val) in row.iter().enumerate() {
                if !val.is_empty() {
                    cells.push((r_idx as u32, c_idx as u32, val.clone()));
                }
            }
        }
        let title = rows
            .first()
            .and_then(|r| r.first())
            .cloned()
            .unwrap_or_else(|| "시트".to_string());
        Ok(Self::from_sheet(title, cells))
    }

    fn decode_pdf(bytes: &[u8]) -> Result<UniversalDocumentModel, EasyOfficeError> {
        let doc = PdfSerializer::parse(bytes)?;
        let mut paragraphs = Vec::new();
        let mut agency = None;
        for page in doc.pages {
            paragraphs.extend(
                page.body_text
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(ToString::to_string),
            );
            if agency.is_none() {
                agency = page.approval_stamp;
            }
        }
        Ok(Self::from_doc(doc.title, paragraphs, Vec::new(), agency))
    }

    /// Converts between formats. A document source converted to a document target goes through
    /// the rich model, so block order (tables between their paragraphs) and character formatting
    /// survive (#167); everything else (sheets, slides, or a document into a sheet or deck) uses
    /// the flat model as before.
    pub fn convert(
        bytes: &[u8],
        source_hint: Option<&str>,
        target: DocumentFormat,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        if RICH_ORDER_TARGETS.contains(&target) {
            if let Ok(decoded) = Self::decode_rich(bytes, source_hint) {
                if let crate::DecodedContent::Document(doc) = decoded.content {
                    return Self::encode_rich(&doc, target, None);
                }
            }
        }
        let (_, model) = Self::decode(bytes, source_hint)?;
        Self::encode(&model, target)
    }

    fn from_doc(
        title: String,
        paragraphs: Vec<String>,
        tables: Vec<Vec<Vec<String>>>,
        official_agency: Option<String>,
    ) -> UniversalDocumentModel {
        let sheet_cells = doc_to_sheet_cells(&paragraphs, &tables);
        let slides = vec![(title.clone(), paragraphs.clone(), String::new())];
        UniversalDocumentModel {
            title,
            paragraphs,
            tables,
            sheet_cells,
            slides,
            official_agency,
        }
    }

    fn from_sheet(
        sheet_name: String,
        sheet_cells: Vec<(u32, u32, String)>,
    ) -> UniversalDocumentModel {
        Self::from_sheets(vec![(sheet_name, sheet_cells)])
    }

    fn from_sheets(sheets: Vec<crate::OdsUnpacked>) -> UniversalDocumentModel {
        let mut iter = sheets.into_iter();
        let (sheet_name, sheet_cells) = iter
            .next()
            .unwrap_or_else(|| ("시트1".to_string(), Vec::new()));
        let mut paragraphs: Vec<String> = sheet_cells.iter().map(|(_, _, v)| v.clone()).collect();
        let mut tables = Vec::new();
        let build_grid = |cells: &[(u32, u32, String)]| -> Option<Vec<Vec<String>>> {
            if cells.is_empty() {
                return None;
            }
            let max_r = cells.iter().map(|(r, _, _)| *r).max().unwrap_or(0).min(512) as usize;
            let max_c = cells.iter().map(|(_, c, _)| *c).max().unwrap_or(0).min(64) as usize;
            let mut grid = vec![vec![String::new(); max_c + 1]; max_r + 1];
            for (r, c, val) in cells {
                let ri = *r as usize;
                let ci = *c as usize;
                if ri <= max_r && ci <= max_c {
                    grid[ri][ci] = val.clone();
                }
            }
            Some(grid)
        };
        if let Some(g) = build_grid(&sheet_cells) {
            tables.push(g);
        }
        for (_, extra_cells) in iter {
            paragraphs.extend(extra_cells.iter().map(|(_, _, v)| v.clone()));
            if let Some(g) = build_grid(&extra_cells) {
                tables.push(g);
            }
        }
        let slides = vec![(sheet_name.clone(), paragraphs.clone(), String::new())];
        UniversalDocumentModel {
            title: sheet_name,
            paragraphs,
            tables,
            sheet_cells,
            slides,
            official_agency: None,
        }
    }

    fn from_slides(
        title: String,
        slides: Vec<(String, Vec<String>, String)>,
    ) -> UniversalDocumentModel {
        let mut paragraphs = Vec::new();
        for (s_title, bullets, notes) in &slides {
            if !s_title.is_empty() {
                paragraphs.push(s_title.clone());
            }
            for b in bullets {
                if !b.is_empty() && paragraphs.last() != Some(b) {
                    paragraphs.push(b.clone());
                }
            }
            if !notes.is_empty() && paragraphs.last() != Some(notes) {
                paragraphs.push(notes.clone());
            }
        }
        let sheet_cells: Vec<(u32, u32, String)> = paragraphs
            .iter()
            .enumerate()
            .map(|(idx, p)| (idx as u32, 0, p.clone()))
            .collect();
        UniversalDocumentModel {
            title,
            paragraphs,
            tables: Vec::new(),
            sheet_cells,
            slides,
            official_agency: None,
        }
    }
}

fn doc_to_sheet_cells(
    paragraphs: &[String],
    tables: &[Vec<Vec<String>>],
) -> Vec<(u32, u32, String)> {
    let mut cells: Vec<(u32, u32, String)> = Vec::new();
    let mut next_row: u32 = 0;
    for p in paragraphs {
        cells.push((next_row, 0, p.clone()));
        next_row += 1;
    }
    for table in tables {
        for row in table {
            for (c_idx, val) in row.iter().enumerate() {
                cells.push((next_row, c_idx as u32, val.clone()));
            }
            next_row += 1;
        }
    }
    cells
}

/// Rectangular text grid of a sheet for CSV/TSV. Sized to the cells' real extent; a sheet the CSV
/// reader would refuse (beyond its row or column limit) or whose grid is too large to build is
/// refused here instead of being cut short.
fn cells_to_grid(
    cells: &[(u32, u32, String)],
    fallback_title: &str,
) -> Result<Vec<Vec<String>>, EasyOfficeError> {
    if cells.is_empty() {
        return Ok(vec![vec![fallback_title.to_string(), String::new()]]);
    }
    let max_r = cells.iter().map(|(r, _, _)| *r).max().unwrap_or(0) as usize;
    let max_c = cells.iter().map(|(_, c, _)| *c).max().unwrap_or(0).max(1) as usize;
    let (rows, cols) = (max_r + 1, max_c + 1);
    if rows > CsvCodec::MAX_ROWS || cols > CsvCodec::MAX_COLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "CSV/TSV는 {}행 x {}열까지 저장할 수 있어 이 시트({rows}행 x {cols}열)는 저장할 수 없습니다.",
            CsvCodec::MAX_ROWS,
            CsvCodec::MAX_COLS
        )));
    }
    if rows.saturating_mul(cols) > CsvCodec::MAX_GRID_CELLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "시트 값 표가 너무 커서 CSV/TSV로 저장할 수 없습니다.".to_string(),
        ));
    }
    let mut grid = vec![vec![String::new(); cols]; rows];
    for (r, c, val) in cells {
        grid[*r as usize][*c as usize].clone_from(val);
    }
    Ok(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_twenty_formats_encode_detect_decode_and_convert() {
        let model = UniversalDocumentModel {
            title: "통합포맷검증보고서".to_string(),
            paragraphs: vec![
                "첫번째 핵심 문단입니다.".to_string(),
                "두번째 보안 무결성 검증 문단입니다.".to_string(),
            ],
            tables: vec![vec![
                vec!["구분".to_string(), "결과".to_string()],
                vec!["정합성".to_string(), "통과".to_string()],
            ]],
            sheet_cells: vec![
                (0, 0, "첫번째 핵심 문단입니다.".to_string()),
                (1, 0, "두번째 보안 무결성 검증 문단입니다.".to_string()),
            ],
            slides: vec![(
                "통합포맷검증보고서".to_string(),
                vec![
                    "첫번째 핵심 문단입니다.".to_string(),
                    "두번째 보안 무결성 검증 문단입니다.".to_string(),
                ],
                "발표자 노트".to_string(),
            )],
            official_agency: Some("디지털표준원".to_string()),
        };

        for format in DocumentFormat::ALL {
            let encoded = UniversalFormatConverter::encode(&model, format).unwrap();
            assert!(!encoded.is_empty());

            // Must be recognized by FormatDetector::detect with extension hint
            let detected = FormatDetector::detect(&encoded, Some(format.extension())).unwrap();
            assert_eq!(detected, format);

            // Decode and verify semantic recovery
            let (decoded_fmt, decoded_model) =
                UniversalFormatConverter::decode(&encoded, Some(format.extension())).unwrap();
            assert_eq!(decoded_fmt, format);
            assert!(decoded_model
                .paragraphs
                .iter()
                .any(|p| p.contains("첫번째 핵심 문단입니다.")));

            // Cross-format convert to DOCX and PDF
            let converted_pdf = UniversalFormatConverter::convert(
                &encoded,
                Some(format.extension()),
                DocumentFormat::Pdf,
            )
            .unwrap();
            assert_eq!(
                FormatDetector::detect(&converted_pdf, Some("pdf")).unwrap(),
                DocumentFormat::Pdf
            );
        }
    }

    #[test]
    fn test_markdown_and_pdf_preserve_document_tables() {
        let model = UniversalDocumentModel {
            title: "표보존검증".to_string(),
            paragraphs: vec!["본문요약".to_string()],
            tables: vec![vec![
                vec!["항목".to_string(), "값".to_string()],
                vec!["매출".to_string(), "1200".to_string()],
            ]],
            sheet_cells: Vec::new(),
            slides: Vec::new(),
            official_agency: None,
        };

        let md_bytes = UniversalFormatConverter::encode(&model, DocumentFormat::Markdown).unwrap();
        let md_str = String::from_utf8(md_bytes.clone()).unwrap();
        assert!(md_str.contains("| 항목 | 값 |"));
        assert!(md_str.contains("| --- | --- |"));
        assert!(md_str.contains("| 매출 | 1200 |"));
        let (_, md_decoded) = UniversalFormatConverter::decode(&md_bytes, Some("md")).unwrap();
        assert_eq!(md_decoded.tables, model.tables);

        let pdf_bytes = UniversalFormatConverter::encode(&model, DocumentFormat::Pdf).unwrap();
        let pdf_raw = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_raw.contains("/Subtype /Type0"));
        assert!(pdf_raw.contains("/Encoding /UniKS-UCS2-H"));
        assert!(pdf_raw.contains("/DescendantFonts"));
        assert!(pdf_raw.contains("/Subtype /CIDFontType0"));
        assert!(pdf_raw.contains("begincmap"));
        let (_, pdf_decoded) = UniversalFormatConverter::decode(&pdf_bytes, Some("pdf")).unwrap();
        assert!(pdf_decoded
            .paragraphs
            .iter()
            .any(|p| p.contains("매출 | 1200")));
    }

    #[test]
    fn test_cross_family_tables_hwp5_multi_table_slides_and_xml_quote_gt() {
        let doc_model = UniversalDocumentModel {
            title: "복합표문서".to_string(),
            paragraphs: vec!["머리말문단".to_string()],
            tables: vec![
                vec![
                    vec!["부서".to_string(), "예산".to_string()],
                    vec!["플랫폼팀".to_string(), "5000".to_string()],
                ],
                vec![
                    vec!["분기".to_string(), "상태".to_string()],
                    vec!["4분기".to_string(), "완료".to_string()],
                ],
            ],
            sheet_cells: Vec::new(),
            slides: Vec::new(),
            official_agency: Some("행정안전부".to_string()),
        };

        let hwp_bytes = UniversalFormatConverter::encode(&doc_model, DocumentFormat::Hwp).unwrap();
        let (_, hwp_decoded) = UniversalFormatConverter::decode(&hwp_bytes, Some("hwp")).unwrap();
        assert_eq!(hwp_decoded.tables.len(), 2);
        assert_eq!(hwp_decoded.tables, doc_model.tables);

        let xlsx_from_hwp =
            UniversalFormatConverter::encode(&hwp_decoded, DocumentFormat::Xlsx).unwrap();
        let (_, sheet_decoded) =
            UniversalFormatConverter::decode(&xlsx_from_hwp, Some("xlsx")).unwrap();
        assert!(sheet_decoded
            .sheet_cells
            .iter()
            .any(|(_, _, v)| v == "머리말문단"));
        assert!(sheet_decoded
            .sheet_cells
            .iter()
            .any(|(_, _, v)| v == "플랫폼팀"));
        assert!(sheet_decoded
            .sheet_cells
            .iter()
            .any(|(_, _, v)| v == "4분기"));
        assert!(!sheet_decoded.tables.is_empty());
        assert_eq!(sheet_decoded.tables[0][1][0], "부서");
        assert_eq!(sheet_decoded.tables[0][1][1], "예산");

        let pptx_bytes = UniversalFormatConverter::encode(
            &UniversalDocumentModel {
                title: "발표덱".to_string(),
                paragraphs: Vec::new(),
                tables: Vec::new(),
                sheet_cells: Vec::new(),
                slides: vec![(
                    "슬라이드제목1".to_string(),
                    vec!["핵심불릿1".to_string()],
                    "발표자노트1".to_string(),
                )],
                official_agency: None,
            },
            DocumentFormat::Pptx,
        )
        .unwrap();
        let (_, slide_decoded) =
            UniversalFormatConverter::decode(&pptx_bytes, Some("pptx")).unwrap();
        assert!(slide_decoded
            .paragraphs
            .iter()
            .any(|p| p == "슬라이드제목1"));
        assert!(slide_decoded.paragraphs.iter().any(|p| p == "핵심불릿1"));
        assert!(slide_decoded.paragraphs.iter().any(|p| p == "발표자노트1"));

        assert_eq!(
            DocumentFormat::from_extension("docm"),
            Some(DocumentFormat::Docx)
        );
        assert_eq!(
            DocumentFormat::from_extension("xlsm"),
            Some(DocumentFormat::Xlsx)
        );
        assert_eq!(
            DocumentFormat::from_extension("pptm"),
            Some(DocumentFormat::Pptx)
        );
        assert_eq!(DocumentFormat::from_extension("html"), None);
        assert_eq!(DocumentFormat::from_extension("htm"), None);

        let xml_with_gt_attr =
            "<table:table table:name=\"A > B\"><table:table-row><table:table-cell><text:p>셀값</text:p></table:table-cell></table:table-row></table:table>";
        assert_eq!(
            crate::rtf_xml_helpers::extract_attr(xml_with_gt_attr, "table:table", "table:name"),
            Some("A > B")
        );
        let blocks = crate::rtf_xml_helpers::extract_tag_bodies(xml_with_gt_attr, "table:table");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("셀값"));
    }
}

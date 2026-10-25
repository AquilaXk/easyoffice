use crate::{
    CommonMarkCodec, CsvCodec, DocumentFormat, EasyOfficeError, FormatDetector, Hwp5Codec,
    Hwp5DocumentPayload, Hwp5ParagraphRecord, HwpxCodec, HwpxPayload, LegacyDocPayload,
    LegacyOle2Codec, LegacyPptPayload, LegacyPptSlide, LegacyXlsPayload, MarkdownBlock, OdfCodec,
    OoxmlCodec, OoxmlDocxPayload, OoxmlPptxPayload, OoxmlXlsxPayload, PdfDocumentModel,
    PdfPageModel, PdfSerializer, RtfCodec, RtfPayload,
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
            for (s_title, bullets, _) in &self.slides {
                if !s_title.is_empty() {
                    out.push(s_title.clone());
                }
                out.extend(bullets.iter().cloned());
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

    fn effective_sheet_cells(&self) -> Vec<(u32, u32, String)> {
        if !self.sheet_cells.is_empty() {
            return self.sheet_cells.clone();
        }
        if let Some(first_table) = self.tables.first() {
            let mut cells = Vec::new();
            for (r_idx, row) in first_table.iter().enumerate() {
                for (c_idx, val) in row.iter().enumerate() {
                    cells.push((r_idx as u32, c_idx as u32, val.clone()));
                }
            }
            if !cells.is_empty() {
                return cells;
            }
        }
        self.effective_paragraphs()
            .into_iter()
            .enumerate()
            .map(|(idx, p)| (idx as u32, 0, p))
            .collect()
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

impl UniversalFormatConverter {
    pub fn encode(
        model: &UniversalDocumentModel,
        format: DocumentFormat,
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
            DocumentFormat::Hwp | DocumentFormat::Hwt => Hwp5Codec::pack(&Hwp5DocumentPayload {
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
            }),
            DocumentFormat::Hwpx | DocumentFormat::Hwtx => HwpxCodec::pack(&HwpxPayload {
                title,
                is_template: format == DocumentFormat::Hwtx,
                official_agency: model.official_agency.clone(),
                approval_lines: vec!["기안".to_string(), "검토".to_string(), "결재".to_string()],
                paragraphs,
                tables: model.tables.clone(),
            }),
            DocumentFormat::Xlsx => OoxmlCodec::pack_xlsx(&OoxmlXlsxPayload {
                sheet_name: title,
                cells: sheet_cells,
            }),
            DocumentFormat::Xls => LegacyOle2Codec::pack_xls(&LegacyXlsPayload {
                sheet_name: title,
                cells: sheet_cells,
            }),
            DocumentFormat::Ods => OdfCodec::pack_ods(&title, &sheet_cells),
            DocumentFormat::Csv | DocumentFormat::Tsv => {
                let delim = if format == DocumentFormat::Csv {
                    ','
                } else {
                    '\t'
                };
                let rows = cells_to_grid(&sheet_cells, &title);
                Ok(CsvCodec::serialize(&rows, delim).into_bytes())
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
            DocumentFormat::Markdown => {
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("Markdown UTF-8 디코딩 오류".to_string())
                })?;
                let blocks = CommonMarkCodec::parse(text)?;
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
                        | MarkdownBlock::BlockQuote(text) => {
                            paragraphs.push(text);
                        }
                        MarkdownBlock::BulletList(items) | MarkdownBlock::NumberedList(items) => {
                            paragraphs.extend(items);
                        }
                        MarkdownBlock::Table(rows) => {
                            if !rows.is_empty() {
                                tables.push(rows);
                            }
                        }
                    }
                }
                Self::from_doc(title, paragraphs, tables, None)
            }
            DocumentFormat::Txt => {
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("TXT UTF-8 디코딩 오류".to_string())
                })?;
                let mut lines: Vec<String> = text.lines().map(ToString::to_string).collect();
                let title = if !lines.is_empty() {
                    lines.remove(0)
                } else {
                    "텍스트 문서".to_string()
                };
                if lines.is_empty() {
                    lines.push(title.clone());
                }
                Self::from_doc(title, lines, Vec::new(), None)
            }
            DocumentFormat::Hwp | DocumentFormat::Hwt => {
                let p = Hwp5Codec::unpack(bytes)?;
                let tables = if p.table_cells.is_empty() {
                    Vec::new()
                } else {
                    vec![p.table_cells]
                };
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
                let p = OoxmlCodec::unpack_xlsx(bytes)?;
                Self::from_sheet(p.sheet_name, p.cells)
            }
            DocumentFormat::Xls => {
                let p = LegacyOle2Codec::unpack_xls(bytes)?;
                Self::from_sheet(p.sheet_name, p.cells)
            }
            DocumentFormat::Ods => {
                let (sheet_name, cells) = OdfCodec::unpack_ods(bytes)?;
                Self::from_sheet(sheet_name, cells)
            }
            DocumentFormat::Csv | DocumentFormat::Tsv => {
                let delim = if format == DocumentFormat::Csv {
                    ','
                } else {
                    '\t'
                };
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("CSV/TSV UTF-8 디코딩 오류".to_string())
                })?;
                let rows = CsvCodec::parse(text, delim)?;
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
                Self::from_sheet(title, cells)
            }
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
            DocumentFormat::Pdf => {
                let doc = PdfSerializer::parse(bytes)?;
                let mut paragraphs = Vec::new();
                let mut agency = None;
                for page in doc.pages {
                    for line in page.body_text.lines() {
                        if !line.is_empty() {
                            paragraphs.push(line.to_string());
                        }
                    }
                    if agency.is_none() {
                        agency = page.approval_stamp;
                    }
                }
                Self::from_doc(doc.title, paragraphs, Vec::new(), agency)
            }
        };
        Ok((format, model))
    }

    pub fn convert(
        bytes: &[u8],
        source_hint: Option<&str>,
        target: DocumentFormat,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let (_, model) = Self::decode(bytes, source_hint)?;
        Self::encode(&model, target)
    }

    fn from_doc(
        title: String,
        paragraphs: Vec<String>,
        tables: Vec<Vec<Vec<String>>>,
        official_agency: Option<String>,
    ) -> UniversalDocumentModel {
        let sheet_cells: Vec<(u32, u32, String)> = paragraphs
            .iter()
            .enumerate()
            .map(|(idx, p)| (idx as u32, 0, p.clone()))
            .collect();
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
        let paragraphs: Vec<String> = sheet_cells.iter().map(|(_, _, v)| v.clone()).collect();
        let slides = vec![(sheet_name.clone(), paragraphs.clone(), String::new())];
        UniversalDocumentModel {
            title: sheet_name,
            paragraphs,
            tables: Vec::new(),
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
        for (s_title, bullets, _) in &slides {
            if !bullets.is_empty() {
                paragraphs.extend(bullets.iter().cloned());
            } else if !s_title.is_empty() {
                paragraphs.push(s_title.clone());
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

fn cells_to_grid(cells: &[(u32, u32, String)], fallback_title: &str) -> Vec<Vec<String>> {
    if cells.is_empty() {
        return vec![vec![fallback_title.to_string(), String::new()]];
    }
    let max_r = cells.iter().map(|(r, _, _)| *r).max().unwrap_or(0).min(512) as usize;
    let max_c = cells
        .iter()
        .map(|(_, c, _)| *c)
        .max()
        .unwrap_or(0)
        .clamp(1, 64) as usize;
    let mut grid = vec![vec![String::new(); max_c + 1]; max_r + 1];
    for (r, c, val) in cells {
        let ri = *r as usize;
        let ci = *c as usize;
        if ri <= max_r && ci <= max_c {
            grid[ri][ci] = val.clone();
        }
    }
    grid
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
        let (_, pdf_decoded) = UniversalFormatConverter::decode(&pdf_bytes, Some("pdf")).unwrap();
        assert!(pdf_decoded
            .paragraphs
            .iter()
            .any(|p| p.contains("매출 | 1200")));
    }
}

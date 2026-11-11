use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, UniversalDocumentModel, UniversalFormatConverter,
    XlsxCachedValue, XlsxFormulaCache,
};
use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, SheetEngine};

use crate::convert_structured::{encode_structured, parse_structured_content, StructuredContent};
use crate::EasyOfficeIpcRouter;

/// Evaluates every formula cell XLSX output will contain with the sheet engine, so the written
/// workbook carries real cached values (`<v>`) instead of placeholders. Error results are left
/// uncached; the workbook's `fullCalcOnLoad` makes consumers recompute them.
pub(crate) fn compute_xlsx_formula_cache(model: &UniversalDocumentModel) -> XlsxFormulaCache {
    let mut cache = XlsxFormulaCache::new();
    let cells = model.effective_sheet_cells();
    if !cells.iter().any(|(_, _, raw)| raw.starts_with('=')) {
        return cache;
    }
    let sheet_name = if model.title.trim().is_empty() {
        "문서"
    } else {
        model.title.as_str()
    };
    let mut engine = SheetEngine::new(sheet_name);
    for (row, col, raw) in &cells {
        if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
            let _ = engine.set_cell(&coord.to_a1(), raw.clone());
        }
    }
    let results = engine.recalculate_all();
    for (row, col, raw) in &cells {
        if !raw.starts_with('=') {
            continue;
        }
        let Ok(coord) = PackedCellCoordinate::new(*row, *col) else {
            continue;
        };
        let cached = match results.get(&coord) {
            Some(EvaluatedCellValue::Number(n)) if n.is_finite() => XlsxCachedValue::Number(*n),
            Some(EvaluatedCellValue::Text(s)) => XlsxCachedValue::Text(s.clone()),
            Some(EvaluatedCellValue::Boolean(b)) => XlsxCachedValue::Boolean(*b),
            _ => continue,
        };
        cache.insert((*row, *col), cached);
    }
    cache
}

/// Collects the line-oriented legacy markup into model parts.
#[derive(Default)]
struct LegacyBodyAccumulator {
    paragraphs: Vec<String>,
    tables: Vec<Vec<Vec<String>>>,
    current_table: Vec<Vec<String>>,
    sheet_cells: Vec<(u32, u32, String)>,
    slides: Vec<(String, Vec<String>, String)>,
    official_agency: Option<String>,
}

impl LegacyBodyAccumulator {
    fn apply_line(&mut self, line: &str, title: &str) {
        if let Some(agency) = line.strip_prefix("[[AGENCY]]\t") {
            if !agency.trim().is_empty() {
                self.official_agency = Some(agency.trim().to_string());
            }
        } else if line == "[[TABLE_NEW]]" {
            if !self.current_table.is_empty() {
                self.tables.push(std::mem::take(&mut self.current_table));
            }
        } else if let Some(row_str) = line.strip_prefix("[[TABLE_ROW]]\t") {
            self.current_table
                .push(row_str.split('\t').map(|c| c.trim().to_string()).collect());
        } else if let Some(cell_str) = line.strip_prefix("[[SHEET_CELL]]\t") {
            self.push_indexed_cell(cell_str);
        } else if let Some(a1_str) = line.strip_prefix("[[SHEET_A1]]\t") {
            self.push_a1_cell(a1_str);
        } else if let Some(slide_str) = line.strip_prefix("[[SLIDE]]\t") {
            self.slides.push(Self::parse_slide(slide_str, title));
        } else {
            self.paragraphs.push(line.to_string());
        }
    }

    fn push_indexed_cell(&mut self, cell_str: &str) {
        let mut cols = cell_str.splitn(3, '\t');
        if let (Some(r_s), Some(c_s), Some(v_s)) = (cols.next(), cols.next(), cols.next()) {
            if let (Ok(r), Ok(c)) = (r_s.trim().parse::<u32>(), c_s.trim().parse::<u32>()) {
                self.sheet_cells.push((r, c, v_s.to_string()));
            }
        }
    }

    fn push_a1_cell(&mut self, a1_str: &str) {
        let Some((a1, v_s)) = a1_str.split_once('\t') else {
            return;
        };
        if let Ok(coord) = PackedCellCoordinate::parse_a1(a1.trim()) {
            self.sheet_cells
                .push((coord.row(), coord.col(), v_s.to_string()));
        }
    }

    fn parse_slide(slide_str: &str, title: &str) -> (String, Vec<String>, String) {
        let mut cols = slide_str.splitn(3, '\t');
        let s_title = cols.next().unwrap_or(title).trim().to_string();
        let bullets: Vec<String> = cols
            .next()
            .unwrap_or("")
            .split(";;")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        let notes = cols.next().unwrap_or("").trim().to_string();
        (s_title, bullets, notes)
    }

    fn into_model(mut self, title: &str) -> UniversalDocumentModel {
        if !self.current_table.is_empty() {
            self.tables.push(std::mem::take(&mut self.current_table));
        }
        if self.paragraphs.is_empty() && !self.sheet_cells.is_empty() {
            self.paragraphs = self.sheet_cells.iter().map(|(_, _, v)| v.clone()).collect();
        }
        if self.paragraphs.is_empty() {
            for (s_t, bullets, notes) in &self.slides {
                if !s_t.is_empty() {
                    self.paragraphs.push(s_t.clone());
                }
                self.paragraphs.extend(bullets.iter().cloned());
                if !notes.is_empty() {
                    self.paragraphs.push(notes.clone());
                }
            }
        }
        if self.paragraphs.is_empty() {
            self.paragraphs.push(title.to_string());
        }
        UniversalDocumentModel {
            title: title.to_string(),
            paragraphs: self.paragraphs,
            tables: self.tables,
            sheet_cells: self.sheet_cells,
            slides: self.slides,
            official_agency: self.official_agency,
        }
    }
}

impl EasyOfficeIpcRouter {
    pub(crate) fn handle_convert_document_format(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let mut structured: Option<StructuredContent> = None;
        let mut warnings: Vec<String> = Vec::new();
        let (src_owned, dst_owned, title_owned, body_owned) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
            if let Some(b64) = Self::optional_json_str(&entries, "sourceBase64")? {
                return Self::handle_source_file_request(&entries, b64);
            }
            if Self::optional_json_str(&entries, "action")? == Some("decode") {
                return Err(EasyOfficeError::InvalidArgument(
                    "decode 요청에는 sourceBase64가 필요합니다.".to_string(),
                ));
            }
            if let Some((content, parse_warnings)) = parse_structured_content(&entries)? {
                structured = Some(content);
                warnings = parse_warnings;
            }
            let src = match Self::optional_json_str(&entries, "sourceFormat")? {
                Some(s) => s,
                None => Self::optional_json_str(&entries, "source")?.unwrap_or("docx"),
            }
            .trim()
            .to_string();
            let dst = match Self::optional_json_str(&entries, "targetFormat")? {
                Some(d) => d,
                None => Self::optional_json_str(&entries, "target")?.unwrap_or("hwpx"),
            }
            .trim()
            .to_string();
            let t = Self::optional_json_str(&entries, "title")?
                .unwrap_or("")
                .trim()
                .to_string();
            let b = Self::optional_json_str(&entries, "body")?
                .unwrap_or("본문 텍스트")
                .trim()
                .to_string();
            (src, dst, t, b)
        } else if let Some((first, rest)) = payload.split_once('|') {
            if let Some((s, d)) = first.split_once("->") {
                let (t, b) = rest.split_once('|').unwrap_or((rest, ""));
                (
                    s.trim().to_string(),
                    d.trim().to_string(),
                    t.trim().to_string(),
                    b.trim().to_string(),
                )
            } else if let Some((second, rest2)) = rest.split_once('|') {
                let (t, b) = rest2.split_once('|').unwrap_or((rest2, ""));
                (
                    first.trim().to_string(),
                    second.trim().to_string(),
                    t.trim().to_string(),
                    b.trim().to_string(),
                )
            } else {
                (
                    first.trim().to_string(),
                    rest.trim().to_string(),
                    "변환_문서".to_string(),
                    "본문 텍스트".to_string(),
                )
            }
        } else if let Some((s, d)) = payload.split_once("->") {
            (
                s.trim().to_string(),
                d.trim().to_string(),
                "변환_문서".to_string(),
                "본문 텍스트".to_string(),
            )
        } else {
            (
                "docx".to_string(),
                "hwpx".to_string(),
                "변환_문서".to_string(),
                "본문 텍스트".to_string(),
            )
        };

        let src_ext = src_owned.as_str();
        let dst_ext = dst_owned.as_str();
        let body = body_owned.as_str();
        let title = if title_owned.is_empty() {
            "변환_문서"
        } else {
            title_owned.as_str()
        };

        let src_fmt = DocumentFormat::from_extension(src_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 원본 포맷: {src_ext}"))
        })?;
        let dst_fmt = DocumentFormat::from_extension(dst_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 대상 포맷: {dst_ext}"))
        })?;

        // A source format that is not written in full (.pages) without sourceBase64 uses DOCX as the
        // size baseline.
        let size_fmt = if src_fmt.is_experimental_write() {
            DocumentFormat::Docx
        } else {
            src_fmt
        };
        let (src_bytes, dst_bytes) = if let Some(content) = &structured {
            // Only what the target loses is reported; the source-side encode is just for its size.
            (
                encode_structured(content, size_fmt, title, &mut Vec::new())?,
                encode_structured(content, dst_fmt, title, &mut warnings)?,
            )
        } else {
            let model = Self::legacy_body_model(body, title);
            let formula_cache = if matches!(size_fmt, DocumentFormat::Xlsx | DocumentFormat::Ods)
                || matches!(dst_fmt, DocumentFormat::Xlsx | DocumentFormat::Ods)
            {
                Some(compute_xlsx_formula_cache(&model))
            } else {
                None
            };
            (
                UniversalFormatConverter::encode_with_formula_cache(
                    &model,
                    size_fmt,
                    formula_cache.as_ref(),
                )?,
                UniversalFormatConverter::encode_with_formula_cache(
                    &model,
                    dst_fmt,
                    formula_cache.as_ref(),
                )?,
            )
        };
        Self::conversion_response(src_fmt, dst_fmt, &src_bytes, &dst_bytes, &warnings)
    }

    /// Legacy text-markup `body` (`[[TABLE_ROW]]`, `[[SHEET_A1]]`, `[[SLIDE]]`, `[[AGENCY]]` lines).
    /// Kept for backward compatibility; the WebUI sends structured `blocks`/`sheets`/`slides`.
    fn legacy_body_model(body: &str, title: &str) -> UniversalDocumentModel {
        let mut acc = LegacyBodyAccumulator::default();
        for raw_line in body.lines() {
            let line = raw_line.trim();
            if !line.is_empty() {
                acc.apply_line(line, title);
            }
        }
        acc.into_model(title)
    }

    pub(crate) fn conversion_response(
        src_fmt: DocumentFormat,
        dst_fmt: DocumentFormat,
        src_bytes: &[u8],
        dst_bytes: &[u8],
        warnings: &[String],
    ) -> Result<String, EasyOfficeError> {
        let warnings_json = warnings
            .iter()
            .map(|w| format!("\"{}\"", Self::escape_json_str(w)))
            .collect::<Vec<_>>()
            .join(",");
        let (detected_dst, decoded) =
            UniversalFormatConverter::decode(dst_bytes, Some(dst_fmt.extension()))?;
        let b64 = Self::encode_base64(dst_bytes);
        let compatibility = dst_fmt.output_compatibility();
        let compatibility_warning = compatibility.warning().map_or_else(
            || "null".to_string(),
            |w| format!("\"{}\"", Self::escape_json_str(w)),
        );

        Ok(format!(
            concat!(
                "{{",
                "\"sourceFormat\":\"{}\",",
                "\"targetFormat\":\"{}\",",
                "\"sourceBytes\":{},",
                "\"outputBytes\":{},",
                "\"targetBytes\":{},",
                "\"detectedTarget\":\"{}\",",
                "\"containerFamily\":\"{}\",",
                "\"specification\":\"{}\",",
                "\"compatibility\":\"{}\",",
                "\"compatibilityWarning\":{},",
                "\"paragraphCount\":{},",
                "\"tableCount\":{},",
                "\"title\":\"{}\",",
                "\"convertedBase64\":\"{}\",",
                "\"warnings\":[{}],",
                "\"status\":\"OK\"",
                "}}"
            ),
            src_fmt.extension(),
            dst_fmt.extension(),
            src_bytes.len(),
            dst_bytes.len(),
            dst_bytes.len(),
            detected_dst.extension(),
            dst_fmt.container_family().as_str(),
            dst_fmt.output_specification(),
            compatibility.as_str(),
            compatibility_warning,
            decoded.paragraphs.len(),
            decoded.tables.len(),
            Self::escape_json_str(&decoded.title),
            b64,
            warnings_json
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{ZipArchiveCodec, ZipSecurityPolicy};

    fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
        let marker = format!("\"{key}\":\"");
        let start = json.find(&marker).unwrap() + marker.len();
        let end = json[start..].find('"').unwrap() + start;
        &json[start..end]
    }

    #[test]
    fn xlsx_conversion_caches_sheet_engine_results_for_formula_cells() {
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"캐시검증","body":"[[SHEET_A1]]\tA1\t10\n[[SHEET_A1]]\tA2\t32\n[[SHEET_A1]]\tA3\t=SUM(A1:A2)\n[[SHEET_A1]]\tB1\t=IF(A3>40,\"합격\",\"불합격\")\n[[SHEET_A1]]\tB2\t=A3/0"}"#,
        )
        .unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
        let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
        let sheet = entries
            .iter()
            .find(|e| e.path == "xl/worksheets/sheet1.xml")
            .map(|e| String::from_utf8(e.data.clone()).unwrap())
            .unwrap();

        let mut engine = SheetEngine::new("캐시검증");
        engine.set_cell("A1", "10").unwrap();
        engine.set_cell("A2", "32").unwrap();
        engine.set_cell("A3", "=SUM(A1:A2)").unwrap();
        let expected = engine.evaluate_cell("A3").to_display_string();
        assert_eq!(expected, "42");
        assert!(
            sheet.contains(&format!(
                "<c r=\"A3\"><f>SUM(A1:A2)</f><v>{expected}</v></c>"
            )),
            "{sheet}"
        );
        assert!(sheet.contains("<v>합격</v>"), "{sheet}");
        // Error results are not cached; fullCalcOnLoad recomputes them.
        assert!(sheet.contains("<c r=\"B2\"><f>A3/0</f></c>"), "{sheet}");
        assert!(json.contains("\"compatibility\":\"standard\""));
        assert!(json.contains("\"compatibilityWarning\":null"));
    }

    #[test]
    fn conversion_response_carries_base64_exactly_once_under_camel_case_key() {
        let json = EasyOfficeIpcRouter::dispatch("convert_document_format", "docx->hwpx|제목|본문")
            .unwrap();
        assert!(!json.contains("converted_base64"), "{json}");
        assert_eq!(json.matches("\"convertedBase64\":\"").count(), 1);
        let b64 = json_str_field(&json, "convertedBase64");
        assert!(
            json.len() < b64.len() + 1_000,
            "response duplicates payload"
        );
        let bytes = EasyOfficeIpcRouter::decode_base64(b64).unwrap();
        assert_eq!(
            bytes.len().to_string(),
            json_num_field(&json, "outputBytes")
        );
    }

    fn json_num_field(json: &str, key: &str) -> String {
        let marker = format!("\"{key}\":");
        let start = json.find(&marker).unwrap() + marker.len();
        json[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect()
    }

    #[test]
    fn legacy_doc_and_ppt_conversion_responses_report_standard_compatibility() {
        for (dst, label) in [("doc", "Word Binary"), ("ppt", "Presentation Binary")] {
            let json = EasyOfficeIpcRouter::dispatch(
                "convert_document_format",
                &format!("docx->{dst}|호환성|본문"),
            )
            .unwrap();
            assert!(json.contains("\"compatibility\":\"standard\""), "{json}");
            assert!(json.contains("\"compatibilityWarning\":null"), "{json}");
            let spec = json_str_field(&json, "specification");
            assert!(
                spec.contains(label) && !spec.contains("호환성 제한"),
                "{spec}"
            );
        }
    }

    #[test]
    fn pages_source_without_binary_payload_encodes_writable_target_and_writes_pages_with_warnings()
    {
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            r#"{"sourceFormat":"pages","targetFormat":"docx","title":"Pages변환","blocks":[{"type":"paragraph","runs":[{"text":"본문"}]}]}"#,
        )
        .unwrap();
        assert_eq!(json_str_field(&json, "sourceFormat"), "pages");
        assert_eq!(json_str_field(&json, "targetFormat"), "docx");
        assert_eq!(json_str_field(&json, "detectedTarget"), "docx");
        let pages = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            r#"{"sourceFormat":"docx","targetFormat":"pages","title":"쓰기","blocks":[{"type":"paragraph","runs":[{"text":"본문"}]},{"type":"table","rows":[[{"text":"a"}]]}]}"#,
        )
        .unwrap();
        assert_eq!(json_str_field(&pages, "detectedTarget"), "pages");
        assert!(pages.contains("Pages 저장에서 빠진 요소 1개"), "{pages}");
        // Sheets and slides are not documents: they are refused, not silently emptied.
        assert!(EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            r#"{"sourceFormat":"xlsx","targetFormat":"pages","title":"시트","sheets":[{"name":"S","cells":[{"ref":"A1","raw":"1"}]}]}"#,
        )
        .is_err());
    }
}

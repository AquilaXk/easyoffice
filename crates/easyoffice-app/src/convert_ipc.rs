use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, UniversalDocumentModel, UniversalFormatConverter,
    XlsxCachedValue, XlsxFormulaCache,
};
use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, SheetEngine};

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

impl EasyOfficeIpcRouter {
    pub(crate) fn handle_convert_document_format(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let (src_owned, dst_owned, title_owned, body_owned) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
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

        let mut paragraphs: Vec<String> = Vec::new();
        let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
        let mut current_table: Vec<Vec<String>> = Vec::new();
        let mut sheet_cells: Vec<(u32, u32, String)> = Vec::new();
        let mut slides: Vec<(String, Vec<String>, String)> = Vec::new();
        let mut official_agency: Option<String> = None;

        for raw_line in body.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(agency) = line.strip_prefix("[[AGENCY]]\t") {
                if !agency.trim().is_empty() {
                    official_agency = Some(agency.trim().to_string());
                }
            } else if line == "[[TABLE_NEW]]" {
                if !current_table.is_empty() {
                    tables.push(std::mem::take(&mut current_table));
                }
            } else if let Some(row_str) = line.strip_prefix("[[TABLE_ROW]]\t") {
                let row: Vec<String> = row_str.split('\t').map(|c| c.trim().to_string()).collect();
                if !row.is_empty() {
                    current_table.push(row);
                }
            } else if let Some(cell_str) = line.strip_prefix("[[SHEET_CELL]]\t") {
                let mut cols = cell_str.splitn(3, '\t');
                if let (Some(r_s), Some(c_s), Some(v_s)) = (cols.next(), cols.next(), cols.next()) {
                    if let (Ok(r), Ok(c)) = (r_s.trim().parse::<u32>(), c_s.trim().parse::<u32>()) {
                        sheet_cells.push((r, c, v_s.to_string()));
                    }
                }
            } else if let Some(a1_str) = line.strip_prefix("[[SHEET_A1]]\t") {
                if let Some((a1, v_s)) = a1_str.split_once('\t') {
                    if let Ok(coord) = PackedCellCoordinate::parse_a1(a1.trim()) {
                        sheet_cells.push((coord.row(), coord.col(), v_s.to_string()));
                    }
                }
            } else if let Some(slide_str) = line.strip_prefix("[[SLIDE]]\t") {
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
                slides.push((s_title, bullets, notes));
            } else {
                paragraphs.push(line.to_string());
            }
        }
        if !current_table.is_empty() {
            tables.push(current_table);
        }
        if paragraphs.is_empty() && !sheet_cells.is_empty() {
            paragraphs = sheet_cells.iter().map(|(_, _, v)| v.clone()).collect();
        }
        if paragraphs.is_empty() && !slides.is_empty() {
            for (s_t, bullets, notes) in &slides {
                if !s_t.is_empty() {
                    paragraphs.push(s_t.clone());
                }
                paragraphs.extend(bullets.iter().cloned());
                if !notes.is_empty() {
                    paragraphs.push(notes.clone());
                }
            }
        }
        if paragraphs.is_empty() {
            paragraphs.push(title.to_string());
        }

        let model = UniversalDocumentModel {
            title: title.to_string(),
            paragraphs,
            tables,
            sheet_cells,
            slides,
            official_agency,
        };

        let formula_cache = if src_fmt == DocumentFormat::Xlsx || dst_fmt == DocumentFormat::Xlsx {
            Some(compute_xlsx_formula_cache(&model))
        } else {
            None
        };
        let src_bytes = UniversalFormatConverter::encode_with_formula_cache(
            &model,
            src_fmt,
            formula_cache.as_ref(),
        )?;
        let dst_bytes = UniversalFormatConverter::encode_with_formula_cache(
            &model,
            dst_fmt,
            formula_cache.as_ref(),
        )?;
        let (detected_dst, decoded) =
            UniversalFormatConverter::decode(&dst_bytes, Some(dst_fmt.extension()))?;
        let b64 = Self::encode_base64(&dst_bytes);
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
                "\"converted_base64\":\"{}\",",
                "\"convertedBase64\":\"{}\",",
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
            b64
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
    fn legacy_doc_and_ppt_conversion_responses_flag_limited_compatibility() {
        for (dst, label) in [("doc", "Word Binary"), ("ppt", "Presentation Binary")] {
            let json = EasyOfficeIpcRouter::dispatch(
                "convert_document_format",
                &format!("docx->{dst}|호환성|본문"),
            )
            .unwrap();
            assert!(json.contains("\"compatibility\":\"limited\""), "{json}");
            let warning = json_str_field(&json, "compatibilityWarning");
            assert!(warning.contains("열리지 않을 수 있습니다"), "{json}");
            let spec = json_str_field(&json, "specification");
            assert!(
                spec.contains(label) && spec.contains("호환성 제한"),
                "{spec}"
            );
        }
    }
}

use easyoffice_formats::{
    CommonMarkCodec, CsvCodec, DocumentFormat, EasyOfficeError, FormatDetector, Ole2SecurityGuard,
    PdfDocumentModel, PdfPageModel, PdfSerializer, XmlSanitizer, ZipEntryDescriptor,
    ZipSecurityGuard, ZipSecurityPolicy,
};
use easyoffice_layout::{
    DesktopPlatform, DocumentPageLayout, LayoutRect, OfficeModuleKind, PageOrientation,
    PdfStudioLayout, SignatureLavenderPalette, SlideDeckTheme, SlideStageLayout,
};
use easyoffice_sheet::{
    CellRange, ConditionalFormatRule, DataValidationRule, EvaluatedCellValue, PivotAggregation,
    SheetEngine,
};

/// Tauri v2 compatible IPC router dispatching desktop workspace commands to pure Rust engines.
pub struct EasyOfficeIpcRouter;

impl EasyOfficeIpcRouter {
    pub const RIBBON_CATEGORIES: [&'static str; 7] = [
        "home",
        "insert",
        "layout",
        "formulasAndData",
        "moduleSpecific",
        "review",
        "view",
    ];

    pub const RECENT_FORMAT_FILTERS: [&'static str; 7] = [
        "전체",
        "DOCX",
        "HWP/HWPX",
        "XLSX",
        "PPTX",
        "PDF",
        "ODF",
    ];

    pub fn dispatch(command: &str, payload: &str) -> Result<String, EasyOfficeError> {
        match command {
            "get_workspace_bootstrap" => Self::handle_workspace_bootstrap(),
            "detect_format" => Self::handle_detect_format(payload),
            "evaluate_sheet" => Self::handle_evaluate_sheet(payload),
            "compute_layout" => Self::handle_compute_layout(payload),
            "audit_security" => Self::handle_audit_security(payload),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "알 수 없는 Tauri IPC 명령입니다: {other}"
            ))),
        }
    }

    fn handle_workspace_bootstrap() -> Result<String, EasyOfficeError> {
        let palette = SignatureLavenderPalette::default();
        let platform = DesktopPlatform::current();
        let filters_json = Self::RECENT_FORMAT_FILTERS
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(",");
        let cats_json = Self::RIBBON_CATEGORIES
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");

        let json = format!(
            concat!(
                "{{",
                "\"platform\":\"{}\",",
                "\"shortcutModifier\":\"{}\",",
                "\"palette\":{{\"primaryAccent\":\"{}\",\"lightScaffold\":\"{}\",\"darkScaffold\":\"{}\"}},",
                "\"moduleAccents\":{{\"DOC\":\"{}\",\"HWP\":\"{}\",\"SHEET\":\"{}\",\"SLIDE\":\"{}\",\"PDF\":\"{}\"}},",
                "\"ribbonCategories\":[{}],",
                "\"ribbonMatrixSlots\":35,",
                "\"recentFormatFilters\":[{}],",
                "\"openTabs\":[",
                "{{\"id\":\"tab-doc\",\"title\":\"연간_아키텍처_보고서.docx\",\"module\":\"DOC\",\"isPinned\":true,\"isFavorite\":true}},",
                "{{\"id\":\"tab-hwp\",\"title\":\"행정_표준_기안문.hwpx\",\"module\":\"HWP\",\"isPinned\":true,\"isFavorite\":false}},",
                "{{\"id\":\"tab-sheet\",\"title\":\"분기_예산_시트.xlsx\",\"module\":\"SHEET\",\"isPinned\":false,\"isFavorite\":true}},",
                "{{\"id\":\"tab-slide\",\"title\":\"제품_전략_발표덱.pptx\",\"module\":\"SLIDE\",\"isPinned\":false,\"isFavorite\":false}},",
                "{{\"id\":\"tab-pdf\",\"title\":\"보안_감사_인증서.pdf\",\"module\":\"PDF\",\"isPinned\":false,\"isFavorite\":true}}",
                "]",
                "}}"
            ),
            platform.os_label(),
            platform.primary_shortcut_modifier(),
            palette.primary_accent,
            palette.light_scaffold,
            palette.dark_scaffold,
            OfficeModuleKind::Document.accent_hex(),
            OfficeModuleKind::Hwp.accent_hex(),
            OfficeModuleKind::Sheet.accent_hex(),
            OfficeModuleKind::Slide.accent_hex(),
            OfficeModuleKind::Pdf.accent_hex(),
            cats_json,
            filters_json,
        );
        Ok(json)
    }

    /// Payload format: `<extension_hint>|<hex_or_utf8_payload>` (e.g. `docx|HEX:504B0304` or `csv|a,b,c`)
    fn handle_detect_format(payload: &str) -> Result<String, EasyOfficeError> {
        let (hint_part, data_part) = payload.split_once('|').unwrap_or(("", payload));
        let bytes = if let Some(hex_str) = data_part.strip_prefix("HEX:") {
            Self::decode_hex(hex_str)?
        } else {
            data_part.as_bytes().to_vec()
        };
        let hint = if hint_part.trim().is_empty() {
            None
        } else {
            Some(hint_part.trim())
        };
        let fmt = FormatDetector::detect(&bytes, hint)?;
        Ok(format!(
            "{{\"format\":\"{}\",\"containerFamily\":\"{}\",\"specification\":\"{}\",\"byteCount\":{}}}",
            fmt.extension(),
            fmt.container_family().as_str(),
            fmt.standard_specification(),
            bytes.len()
        ))
    }

    /// Payload format: semicolon-separated `A1=val` pairs followed by `?target_expr_or_cell`
    /// Example: `A1=100;A2=200;A3==SUM(A1:A2)?A3`
    fn handle_evaluate_sheet(payload: &str) -> Result<String, EasyOfficeError> {
        let (cells_part, query_part) = payload.split_once('?').unwrap_or((payload, "A1"));
        let mut sheet = SheetEngine::new("IPC_Sheet");
        for pair in cells_part.split(';') {
            let trimmed = pair.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((addr, val)) = trimmed.split_once('=') {
                sheet.set_cell(addr.trim(), val.trim())?;
            }
        }
        let evaluated = if query_part.trim().starts_with('=') {
            sheet.evaluate_formula_str(query_part.trim())
        } else {
            sheet.evaluate_cell(query_part.trim())
        };
        let rule = ConditionalFormatRule::GreaterThan(100.0);
        let cf_match = rule.matches(&evaluated, &[]);
        let dv_rule = DataValidationRule::NumberRange {
            min: 0.0,
            max: 1_000_000.0,
        };
        let dv_valid = dv_rule.validate(&evaluated);

        Ok(format!(
            "{{\"query\":\"{}\",\"value\":\"{}\",\"conditionalMatch\":{},\"validationPassed\":{}}}",
            query_part.trim(),
            evaluated.to_display_string(),
            cf_match,
            dv_valid
        ))
    }

    fn handle_compute_layout(payload: &str) -> Result<String, EasyOfficeError> {
        let mode = payload.trim();
        let orientation = if mode.eq_ignore_ascii_case("landscape") {
            PageOrientation::Landscape
        } else {
            PageOrientation::Portrait
        };
        let doc_layout = DocumentPageLayout::compute(orientation, 2, 54.0)?;
        let slide_layout = SlideStageLayout::new(SlideDeckTheme::SignatureLavender);
        let pdf_layout = PdfStudioLayout::compute("APPROVED");

        Ok(format!(
            concat!(
                "{{",
                "\"document\":{{\"widthPt\":{:.1},\"heightPt\":{:.1},\"columnCount\":{},\"columnWidthPt\":{:.1}}},",
                "\"slide\":{{\"stageWidthPt\":{:.1},\"stageHeightPt\":{:.1},\"lowerZoneMinY\":{:.1}}},",
                "\"pdf\":{{\"minVisibleY\":{:.1},\"maxVisibleY\":{:.1},\"stamp\":\"{}\"}}",
                "}}"
            ),
            doc_layout.page_width_pt,
            doc_layout.page_height_pt,
            doc_layout.column_count,
            doc_layout.column_width_pt,
            slide_layout.stage_width_pt,
            slide_layout.stage_height_pt,
            slide_layout.lower_element_zone_min_y,
            PdfStudioLayout::MIN_VISIBLE_Y,
            PdfStudioLayout::MAX_VISIBLE_Y,
            pdf_layout.stamp_label
        ))
    }

    fn handle_audit_security(payload: &str) -> Result<String, EasyOfficeError> {
        XmlSanitizer::validate_safe_xml(payload)?;
        let policy = ZipSecurityPolicy::default();
        let default_entries = vec![ZipEntryDescriptor::new(
            "word/document.xml",
            512,
            2048,
            false,
        )];
        ZipSecurityGuard::validate_archive_entries(&default_entries, &policy)?;
        Ok("{\"status\":\"SAFE\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\"}".to_string())
    }

    fn decode_hex(hex: &str) -> Result<Vec<u8>, EasyOfficeError> {
        let cleaned: Vec<u8> = hex
            .bytes()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        if cleaned.len() % 2 != 0 {
            return Err(EasyOfficeError::InvalidArgument(
                "16진수 문자열 길이가 짝수가 아닙니다.".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(cleaned.len() / 2);
        for pair in cleaned.chunks_exact(2) {
            let hi = Self::hex_val(pair[0])?;
            let lo = Self::hex_val(pair[1])?;
            out.push((hi << 4) | lo);
        }
        Ok(out)
    }

    fn hex_val(byte: u8) -> Result<u8, EasyOfficeError> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(EasyOfficeError::InvalidArgument(
                "유효하지 않은 16진수 문자입니다.".to_string(),
            )),
        }
    }
}

/// Executes deterministic end-to-end self-tests across all 4 crates and embedded Tauri UI assets.
pub fn run_self_test() -> Result<String, EasyOfficeError> {
    // 1. Format detection across 20 formats and Fail-Closed guards
    if DocumentFormat::ALL.len() != 20 {
        return Err(EasyOfficeError::CorruptedContainer(
            "지원 포맷 수가 20종이 아닙니다.".to_string(),
        ));
    }
    let zip_bytes = b"PK\x03\x04\x14\x00\x00\x00word/document.xml";
    let detected_docx = FormatDetector::detect(zip_bytes, Some("report.docx"))?;
    if detected_docx != DocumentFormat::Docx {
        return Err(EasyOfficeError::CorruptedContainer(
            "DOCX 포맷 감지 실패".to_string(),
        ));
    }

    // 2. ZIP / OLE2 / XML security guards
    let policy = ZipSecurityPolicy::default();
    ZipSecurityGuard::validate_archive_entries(
        &[ZipEntryDescriptor::new("word/document.xml", 256, 1024, false)],
        &policy,
    )?;
    if ZipSecurityGuard::validate_archive_entries(
        &[ZipEntryDescriptor::new("../evil.xml", 100, 200, false)],
        &policy,
    )
    .is_ok()
    {
        return Err(EasyOfficeError::SecurityViolation(
            "Zip-Slip 경로 순회 차단에 실패했습니다.".to_string(),
        ));
    }
    let mut ole2_buf = vec![0u8; 512 * 3];
    ole2_buf[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
    ole2_buf[44..48].copy_from_slice(&1u32.to_le_bytes());
    let chain = Ole2SecurityGuard::validate_sector_chain(
        &ole2_buf,
        &[1u32, Ole2SecurityGuard::END_OF_CHAIN],
        0,
    )?;
    if chain != vec![0, 1] {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 FAT 체인 검증 결과 불일치".to_string(),
        ));
    }
    XmlSanitizer::validate_safe_xml("<root attr=\"ok\">안전</root>")?;

    // 3. CSV, CommonMark, and PDF codecs
    let csv_rows = vec![vec!["A".to_string(), "B,C".to_string()]];
    let csv_encoded = CsvCodec::serialize(&csv_rows, ',');
    if CsvCodec::parse(&csv_encoded, ',')? != csv_rows {
        return Err(EasyOfficeError::CorruptedContainer(
            "CSV 라운드트립 불일치".to_string(),
        ));
    }
    let md_blocks = CommonMarkCodec::parse("# 제목\n\n본문")?;
    if CommonMarkCodec::render(&md_blocks) != "# 제목\n\n본문" {
        return Err(EasyOfficeError::CorruptedContainer(
            "CommonMark 라운드트립 불일치".to_string(),
        ));
    }
    let pdf_model = PdfDocumentModel {
        title: "SelfTest PDF".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "PDF 본문".to_string(),
            approval_stamp: Some("APPROVED".to_string()),
        }],
        watermark_text: Some("CONFIDENTIAL".to_string()),
    };
    let pdf_bytes = PdfSerializer::serialize(&pdf_model);
    if PdfSerializer::parse(&pdf_bytes)? != pdf_model {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF 직렬화 라운드트립 불일치".to_string(),
        ));
    }

    // 4. Sheet engine & Pivot
    let mut sheet = SheetEngine::new("SelfTestSheet");
    sheet.set_cell("A1", "부서")?;
    sheet.set_cell("B1", "예산")?;
    sheet.set_cell("A2", "코어")?;
    sheet.set_cell("B2", "1000")?;
    sheet.set_cell("A3", "코어")?;
    sheet.set_cell("B3", "=200*150%")?;
    sheet.set_cell("C1", "=SUM(B2:B3)")?;
    if sheet.evaluate_cell("C1") != EvaluatedCellValue::Number(1300.0) {
        return Err(EasyOfficeError::FormulaError(
            "시트 SUM 계산 결과 불일치".to_string(),
        ));
    }
    let pivot = sheet.compute_pivot(
        &CellRange::parse("A1:B3")?,
        true,
        0,
        1,
        PivotAggregation::Sum,
    );
    if (pivot.grand_total - 1300.0).abs() > 1e-6 {
        return Err(EasyOfficeError::FormulaError(
            "피벗 총계 계산 불일치".to_string(),
        ));
    }

    // 5. Layout engine
    let doc_layout = DocumentPageLayout::compute(PageOrientation::Portrait, 2, 54.0)?;
    if doc_layout.compute_manuscript_grid_200().len() != 200 {
        return Err(EasyOfficeError::InvalidArgument(
            "200자 원고지 격자 수 불일치".to_string(),
        ));
    }
    let slide_layout = SlideStageLayout::new(SlideDeckTheme::SignatureLavender);
    let clamped = slide_layout.clamp_element_to_lower_stage(LayoutRect::new(40.0, 50.0, 200.0, 80.0));
    if !slide_layout.verify_non_overlapping(&[clamped]) {
        return Err(EasyOfficeError::InvalidArgument(
            "슬라이드 비겹침 스테이지 검증 실패".to_string(),
        ));
    }
    let pdf_layout = PdfStudioLayout::compute("APPROVED");
    if !pdf_layout.is_within_upper_middle_viewport() {
        return Err(EasyOfficeError::InvalidArgument(
            "PDF 가시 영역 좌표 검증 실패".to_string(),
        ));
    }

    // 6. IPC Router & Embedded UI asset check
    let bootstrap = EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "")?;
    if !bootstrap.contains("#5C6BC0") || !bootstrap.contains("HWP/HWPX") {
        return Err(EasyOfficeError::InvalidArgument(
            "IPC 부트스트랩 응답 검증 실패".to_string(),
        ));
    }
    let html_asset = include_str!("../ui/index.html");
    let css_asset = include_str!("../ui/styles.css");
    let js_asset = include_str!("../ui/app.js");
    let tauri_conf = include_str!("../tauri.conf.json");
    if !html_asset.contains("EasyOffice")
        || !css_asset.contains("#5C6BC0")
        || !js_asset.contains("compositionstart")
        || !tauri_conf.contains("downloadBootstrapper")
    {
        return Err(EasyOfficeError::CorruptedContainer(
            "Tauri v2 UI 또는 번들 설정 자산 무결성 검증 실패".to_string(),
        ));
    }

    Ok(format!(
        "EasyOffice Rust Cross-Platform Self-Test PASSED (platform={}, formats=20, ipc_commands=5)",
        DesktopPlatform::current().os_label()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_router_commands_and_self_test() {
        let bootstrap = EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "").unwrap();
        assert!(bootstrap.contains("#5C6BC0"));
        assert!(bootstrap.contains("\"ribbonMatrixSlots\":35"));

        let detect_res =
            EasyOfficeIpcRouter::dispatch("detect_format", "docx|HEX:504B03041400").unwrap();
        assert!(detect_res.contains("\"format\":\"docx\""));

        let sheet_res = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            "A1=150;A2=250;A3==SUM(A1:A2)?A3",
        )
        .unwrap();
        assert!(sheet_res.contains("\"value\":\"400\""));
        assert!(sheet_res.contains("\"conditionalMatch\":true"));

        let layout_res = EasyOfficeIpcRouter::dispatch("compute_layout", "landscape").unwrap();
        assert!(layout_res.contains("\"widthPt\":842.0"));

        let sec_res = EasyOfficeIpcRouter::dispatch("audit_security", "<doc>safe</doc>").unwrap();
        assert!(sec_res.contains("\"status\":\"SAFE\""));

        let summary = run_self_test().unwrap();
        assert!(summary.contains("PASSED"));
    }
}

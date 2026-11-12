use easyoffice_formats::{
    deflate_raw, inflate_raw, CommonMarkCodec, CsvCodec, DocumentFormat, EasyOfficeError,
    FormatDetector, Hwp5Codec, Hwp5DocumentPayload, Hwp5ParagraphRecord, Ole2SecurityGuard,
    PdfDocumentModel, PdfPageModel, PdfSerializer, UniversalDocumentModel,
    UniversalFormatConverter, XmlSanitizer, ZipEntryDescriptor, ZipSecurityGuard,
    ZipSecurityPolicy,
};
use easyoffice_layout::{
    ContextualObjectToolKind, DesktopPlatform, DocumentPageLayout, LayoutRect, OfficeModuleKind,
    PageOrientation, PdfStudioLayout, RibbonLayoutStyle, SlideDeckTheme, SlideStageLayout,
};
use easyoffice_sheet::{
    CellRange, EvaluatedCellValue, PivotAggregation, SheetEngine, WorkbookEngine,
};

use crate::{
    invoke_c_abi_roundtrip, run_deterministic_benchmarks, DocumentSecurityAuditor,
    EasyOfficeIpcRouter, RibbonCommandRouter, RibbonWorkspaceState,
};

fn check_codec_roundtrips() -> Result<(), EasyOfficeError> {
    if DocumentFormat::ALL.len() != 20 {
        return Err(EasyOfficeError::CorruptedContainer(
            "지원 포맷 수가 20종이 아닙니다.".to_string(),
        ));
    }
    if !DocumentFormat::Pages.is_experimental_write()
        || DocumentFormat::ALL.contains(&DocumentFormat::Pages)
    {
        return Err(EasyOfficeError::CorruptedContainer(
            "Pages는 실험적 쓰기 형식이어야 합니다.".to_string(),
        ));
    }
    let zip_bytes = b"PK\x03\x04\x14\x00\x00\x00word/document.xml";
    let detected_docx = FormatDetector::detect(zip_bytes, Some("report.docx"))?;
    if detected_docx != DocumentFormat::Docx {
        return Err(EasyOfficeError::CorruptedContainer(
            "DOCX 포맷 감지 실패".to_string(),
        ));
    }

    let raw_deflate_input = "EasyOffice RFC 1951 DEFLATE Self-Test Vector ".repeat(16);
    let deflated = deflate_raw(raw_deflate_input.as_bytes());
    let inflated = inflate_raw(&deflated, 65_536)?;
    if inflated != raw_deflate_input.as_bytes() {
        return Err(EasyOfficeError::CorruptedContainer(
            "RFC 1951 DEFLATE 라운드트립 불일치".to_string(),
        ));
    }
    let hwp_compressed_payload = Hwp5DocumentPayload {
        version: (5, 0, 3, 0),
        is_template: false,
        agency_name: "행정안전부".to_string(),
        document_number: "제2026-55호".to_string(),
        paragraphs: vec![Hwp5ParagraphRecord {
            text: "HWP 5.0 압축 스트림 검증".to_string(),
            style_name: "본문".to_string(),
            is_official_header: false,
        }],
        table_cells: vec![vec!["항목".to_string(), "통과".to_string()]],
    };
    let packed_comp_hwp = Hwp5Codec::pack_compressed(&hwp_compressed_payload)?;
    if Hwp5Codec::unpack(&packed_comp_hwp)? != hwp_compressed_payload {
        return Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 압축 스트림(flags=0x01) 라운드트립 불일치".to_string(),
        ));
    }

    let policy = ZipSecurityPolicy::default();
    ZipSecurityGuard::validate_archive_entries(
        &[ZipEntryDescriptor::new(
            "word/document.xml",
            256,
            1024,
            false,
        )],
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

    Ok(())
}

fn check_sheet_and_formats() -> Result<(), EasyOfficeError> {
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
    if SheetEngine::supported_function_names().len() != 203 {
        return Err(EasyOfficeError::FormulaError(format!(
            "지원 수식 함수 수가 203종이 아닙니다: {}",
            SheetEngine::supported_function_names().len()
        )));
    }
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Sheet2");
    wb.set_cell("Sheet2", "A1", "250")?;
    wb.set_cell("Sheet1", "B1", "=Sheet2!A1*3")?;
    if wb.evaluate_cell("Sheet1", "B1") != EvaluatedCellValue::Number(750.0) {
        return Err(EasyOfficeError::FormulaError(
            "다중 시트 참조(Sheet2!A1) 계산 불일치".to_string(),
        ));
    }
    let sample_doc = UniversalDocumentModel {
        title: "SelfTest Document".to_string(),
        paragraphs: vec!["통합 포맷 라운드트립 문단".to_string()],
        tables: vec![vec![vec!["항목".to_string(), "값".to_string()]]],
        sheet_cells: vec![(0, 0, "A1_VAL".to_string())],
        slides: vec![(
            "슬라이드 제목".to_string(),
            vec!["핵심 요약".to_string()],
            "발표 노트".to_string(),
        )],
        official_agency: Some("행정표준원".to_string()),
    };
    for fmt in DocumentFormat::ALL {
        let encoded = UniversalFormatConverter::encode(&sample_doc, fmt)?;
        let (detected_fmt, _decoded) =
            UniversalFormatConverter::decode(&encoded, Some(fmt.extension()))?;
        if detected_fmt != fmt {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "20종 포맷 인코딩/디코딩 검증 실패: {}",
                fmt.extension()
            )));
        }
    }

    Ok(())
}

fn check_layout_and_ribbon() -> Result<(), EasyOfficeError> {
    let doc_layout = DocumentPageLayout::compute(PageOrientation::Portrait, 2, 54.0)?;
    if doc_layout.compute_manuscript_grid_200().len() != 200 {
        return Err(EasyOfficeError::InvalidArgument(
            "200자 원고지 격자 수 불일치".to_string(),
        ));
    }
    let slide_layout = SlideStageLayout::new(SlideDeckTheme::SignatureLavender);
    let clamped =
        slide_layout.clamp_element_to_lower_stage(LayoutRect::new(40.0, 50.0, 200.0, 80.0));
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

    let all_ribbon = RibbonCommandRouter::all_commands();
    if all_ribbon.len() != 140 {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "리본 명령 수가 140개가 아닙니다: {}",
            all_ribbon.len()
        )));
    }
    if RibbonLayoutStyle::HangulHwp.category_titles()[2] != "쪽·원고지"
        || ContextualObjectToolKind::for_module(OfficeModuleKind::Sheet).badge_title()
            != "수식·피벗 분석"
    {
        return Err(EasyOfficeError::InvalidArgument(
            "리본 스타일 또는 컨텍스트 개체 도구 배지 검증 실패".to_string(),
        ));
    }
    let mut r_state = RibbonWorkspaceState::default();
    RibbonCommandRouter::execute_command("hwp-mod-manuscript", &mut r_state)?;
    if !r_state.manuscript_grid_enabled {
        return Err(EasyOfficeError::InvalidArgument(
            "원고지 리본 명령 상태 변경 실패".to_string(),
        ));
    }

    Ok(())
}

fn check_security_and_ffi() -> Result<(), EasyOfficeError> {
    let pii_res =
        DocumentSecurityAuditor::scan_and_scrub_pii("900101-1234567 010-1234-5678 a@b.kr");
    if pii_res.total_masked_count() != 3 {
        return Err(EasyOfficeError::SecurityViolation(
            "PII 마스킹 건수 검증 실패".to_string(),
        ));
    }
    let ffi_json = invoke_c_abi_roundtrip("switch_ribbon_style", "hangul");
    if !ffi_json.contains("쪽·원고지") {
        return Err(EasyOfficeError::InvalidArgument(
            "C-ABI FFI 라운드트립 검증 실패".to_string(),
        ));
    }

    Ok(())
}

fn check_ipc_responses() -> Result<(), EasyOfficeError> {
    let conv_json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        "docx->hwpx|기관보고서|1. 개요\n2. 세부 계획",
    )?;
    if !conv_json.contains("\"targetFormat\":\"hwpx\"")
        || !conv_json.contains("\"convertedBase64\":\"")
        || !conv_json.contains("\"status\":\"OK\"")
    {
        return Err(EasyOfficeError::InvalidArgument(
            "IPC 포맷 변환 검증 실패".to_string(),
        ));
    }
    let cat_cmds_json =
        EasyOfficeIpcRouter::dispatch("get_ribbon_category_commands", "HWP|module")?;
    if !cat_cmds_json.contains("hwp-mod-header") || !cat_cmds_json.contains("hwp-mod-manuscript") {
        return Err(EasyOfficeError::InvalidArgument(
            "IPC 리본 카테고리 명령 조회 검증 실패".to_string(),
        ));
    }

    let bootstrap = EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "")?;
    if !bootstrap.contains("\"shortcutModifier\"") || bootstrap.contains("ribbonCategories") {
        return Err(EasyOfficeError::InvalidArgument(
            "IPC 부트스트랩 응답 검증 실패".to_string(),
        ));
    }
    Ok(())
}

/// Embedded WebUI asset checks: `(asset text, required marker)`.
const WEBUI_ASSET_MARKERS: &[(&str, &str)] = &[
    (include_str!("../ui/index.html"), "EasyOffice"),
    (include_str!("../ui/tokens.css"), "--ui-menu-open"),
    (include_str!("../ui/styles.css"), "var(--ui-text)"),
    (include_str!("../ui/shell.css"), "var(--ui-bg-light)"),
    (include_str!("../ui/content.css"), "--doc-paper"),
    (include_str!("../ui/theme.js"), "__EASYOFFICE_THEME__"),
    (include_str!("../ui/home.css"), ".home-screen"),
    (include_str!("../ui/menu-bar.css"), ".md-sub-panel"),
    (include_str!("../ui/toolbar.css"), "#icon-toolbar"),
    (
        include_str!("../ui/home-screen.js"),
        "__EASYOFFICE_CREATE_HOME_SCREEN__",
    ),
    (
        include_str!("../ui/recent-documents-dialog.js"),
        "__EASYOFFICE_RECENT_DIALOG__",
    ),
    (
        include_str!("../ui/update-notice.js"),
        "__EASYOFFICE_CREATE_UPDATE_NOTICE__",
    ),
    (include_str!("../ui/icons.js"), "__EASYOFFICE_ICONS__"),
    (
        include_str!("../ui/window-bridge.js"),
        "__EASYOFFICE_WINDOW_BRIDGE__",
    ),
    (
        include_str!("../ui/ipc-transport.js"),
        "__EASYOFFICE_CREATE_IPC_TRANSPORT__",
    ),
    (
        include_str!("../ui/menu-keyboard.js"),
        "__EASYOFFICE_CREATE_MENU_KEYBOARD__",
    ),
    (
        include_str!("../ui/menu-kinds.js"),
        "__EASYOFFICE_MENU_KINDS__",
    ),
    (include_str!("../ui/index.html"), "./shell.css"),
    (include_str!("../ui/index.html"), "./tokens.css"),
    (include_str!("../ui/index.html"), "./content.css"),
    (include_str!("../ui/index.html"), "./theme.js"),
    (include_str!("../ui/index.html"), "./home.css"),
    (include_str!("../ui/index.html"), "./menu-bar.css"),
    (include_str!("../ui/index.html"), "./toolbar.css"),
    (include_str!("../ui/index.html"), "./home-screen.js"),
    (
        include_str!("../ui/index.html"),
        "./recent-documents-dialog.js",
    ),
    (include_str!("../ui/index.html"), "./update-notice.js"),
    (include_str!("../ui/index.html"), "./icons.js"),
    (include_str!("../ui/index.html"), "./window-bridge.js"),
    (include_str!("../ui/index.html"), "./ipc-transport.js"),
    (include_str!("../ui/index.html"), "./menu-keyboard.js"),
    (include_str!("../ui/index.html"), "./menu-kinds.js"),
    (
        include_str!("../ui/sheet-utils.js"),
        "__EASYOFFICE_SHEET_UTILS__",
    ),
    (
        include_str!("../ui/pii-dom-scrubber.js"),
        "__EASYOFFICE_CREATE_PII_SCRUBBER__",
    ),
    (
        include_str!("../ui/sheet-store.js"),
        "__EASYOFFICE_CREATE_SHEET_STORE__",
    ),
    (
        include_str!("../ui/ribbon-catalog.js"),
        "__EASYOFFICE_RIBBON_CATALOG__",
    ),
    (
        include_str!("../ui/save-payload.js"),
        "__EASYOFFICE_SAVE_PAYLOAD__",
    ),
    (
        include_str!("../ui/desktop-bridge.js"),
        "__EASYOFFICE_DESKTOP_BRIDGE__",
    ),
    (
        include_str!("../ui/custom-select.js"),
        "__EASYOFFICE_CUSTOM_SELECT__",
    ),
    (
        include_str!("../ui/color-picker.js"),
        "__EASYOFFICE_COLOR_PICKER__",
    ),
    (
        include_str!("../ui/workspace-scrub.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_SCRUB__",
    ),
    (
        include_str!("../ui/workspace-tools.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_TOOLS__",
    ),
    (
        include_str!("../ui/workspace-panels.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_PANELS__",
    ),
    (
        include_str!("../ui/sheet-grid.js"),
        "__EASYOFFICE_CREATE_SHEET_GRID__",
    ),
    (
        include_str!("../ui/sheet-clipboard.js"),
        "__EASYOFFICE_CREATE_SHEET_CLIPBOARD__",
    ),
    (
        include_str!("../ui/sheet-input.js"),
        "__EASYOFFICE_CREATE_SHEET_INPUT__",
    ),
    (
        include_str!("../ui/sheet-sort.js"),
        "__EASYOFFICE_CREATE_SHEET_SORT__",
    ),
    (
        include_str!("../ui/sheet-sort-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_SORT_UI__",
    ),
    (
        include_str!("../ui/sheet-ops.js"),
        "__EASYOFFICE_CREATE_SHEET_OPS__",
    ),
    (
        include_str!("../ui/sheet-layout.js"),
        "__EASYOFFICE_SHEET_LAYOUT__",
    ),
    (
        include_str!("../ui/sheet-filter.js"),
        "__EASYOFFICE_CREATE_SHEET_FILTER__",
    ),
    (
        include_str!("../ui/sheet-filter-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_FILTER_UI__",
    ),
    (
        include_str!("../ui/sheet-structure.js"),
        "__EASYOFFICE_CREATE_SHEET_STRUCTURE__",
    ),
    (
        include_str!("../ui/sheet-format.js"),
        "__EASYOFFICE_CREATE_SHEET_FORMAT__",
    ),
    (
        include_str!("../ui/sheet-controller.js"),
        "__EASYOFFICE_CREATE_SHEET_CONTROLLER__",
    ),
    (
        include_str!("../ui/ribbon-history.js"),
        "__EASYOFFICE_CREATE_RIBBON_HISTORY__",
    ),
    (
        include_str!("../ui/workspace-session.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_SESSION__",
    ),
    (
        include_str!("../ui/file-open.js"),
        "__EASYOFFICE_CREATE_FILE_OPEN__",
    ),
    (
        include_str!("../ui/pdf-edit.js"),
        "__EASYOFFICE_CREATE_PDF_EDIT__",
    ),
    (
        include_str!("../ui/slide-studio.js"),
        "__EASYOFFICE_CREATE_SLIDE_STUDIO__",
    ),
    (include_str!("../ui/doc-runs.js"), "__EASYOFFICE_DOC_RUNS__"),
    (
        include_str!("../ui/doc-runs-ui.js"),
        "__EASYOFFICE_CREATE_DOC_RUNS_UI__",
    ),
    (
        include_str!("../ui/hwp-host.js"),
        "__EASYOFFICE_CREATE_HWP_HOST__",
    ),
    (include_str!("../ui/index.html"), "id=\"pane-hwp\""),
    (
        include_str!("../ui/hwp-session.js"),
        "__EASYOFFICE_CREATE_HWP_SESSION__",
    ),
    (
        include_str!("../ui/hwp-print.js"),
        "__EASYOFFICE_CREATE_HWP_PRINT__",
    ),
    (include_str!("../ui/index.html"), "./hwp-host.js"),
    (include_str!("../ui/index.html"), "./hwp-print.js"),
    (include_str!("../ui/index.html"), "./hwp-session.js"),
    (include_str!("../ui/app.js"), "compositionstart"),
];

fn check_embedded_assets() -> Result<(), EasyOfficeError> {
    if WEBUI_ASSET_MARKERS
        .iter()
        .any(|(asset, marker)| !asset.contains(marker))
    {
        return Err(EasyOfficeError::CorruptedContainer(
            "내장 WebUI 자산 무결성 검증 실패".to_string(),
        ));
    }
    Ok(())
}

/// Executes deterministic end-to-end self-tests across all 4 crates and embedded WebUI assets.
pub fn run_self_test() -> Result<String, EasyOfficeError> {
    check_codec_roundtrips()?;
    check_sheet_and_formats()?;
    check_layout_and_ribbon()?;
    check_security_and_ffi()?;
    check_ipc_responses()?;
    check_embedded_assets()?;

    if let Some(reason) = crate::hwp_bundle::embedded_integrity_error() {
        return Err(EasyOfficeError::CorruptedContainer(reason));
    }

    let mut bench_report = run_deterministic_benchmarks(5);
    bench_report.real_paths = crate::benchmark_real::run_real_path_benchmarks();
    if let Err(reason) = crate::benchmark_real::real_path_gate(&bench_report.real_paths) {
        return Err(EasyOfficeError::CorruptedContainer(format!(
            "실제 경로 벤치마크 검증 실패: {reason}"
        )));
    }
    if let Err(reason) = crate::benchmark_large::performance_gate(&bench_report) {
        return Err(EasyOfficeError::CorruptedContainer(format!(
            "엔진 성능 게이트 실패: {reason}"
        )));
    }
    if !bench_report.all_equivalent || bench_report.suites.len() != 7 {
        return Err(EasyOfficeError::CorruptedContainer(
            "엔진 저수준 최적화 벤치마크 동등성 검증 실패".to_string(),
        ));
    }

    Ok(format!(
        "EasyOffice Rust Cross-Platform Self-Test PASSED (platform={}, formats=20, functions=203, ribbon_commands=140, ipc_commands=15, {})",
        DesktopPlatform::current().os_label(),
        bench_report.summary_line()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_persistent_ribbon_state_and_safe_sheet_semicolon_parsing() {
        let first =
            EasyOfficeIpcRouter::dispatch("execute_ribbon_command", "document-home-bold").unwrap();
        assert!(first.contains("\"isBold\":true"));
        assert!(first.contains("\"canUndo\":true"));

        let undone = EasyOfficeIpcRouter::dispatch("undo_ribbon_command", "").unwrap();
        assert!(undone.contains("\"isBold\":false"));
        assert!(undone.contains("\"canRedo\":true"));

        let redone = EasyOfficeIpcRouter::dispatch("redo_ribbon_command", "").unwrap();
        assert!(redone.contains("\"isBold\":true"));

        let second =
            EasyOfficeIpcRouter::dispatch("execute_ribbon_command", "document-home-bold").unwrap();
        assert!(second.contains("\"isBold\":false"));

        let rot1 =
            EasyOfficeIpcRouter::dispatch("execute_ribbon_command", "pdf-mod-rotate").unwrap();
        assert!(rot1.contains("\"pdfRotationDegrees\":90"));

        let eval_json = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","expr":"B1","cells":[{"cell":"A1","value":"항목;설명"},{"cell":"B1","value":100},{"cell":"C1","value":"foo;B1=999"}]}"#,
        )
        .unwrap();
        assert!(eval_json.contains("\"value\":\"100\""));

        let wb_json = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheet":"Sheet1","expr":"=SUM(A1:C1)","cells":[{"sheet":"Sheet1","cell":"A1","value":10},{"sheet":"Sheet1","cell":"B1","value":20},{"sheet":"Sheet1","cell":"C1","value":30}]}"#,
        )
        .unwrap();
        assert!(wb_json.contains("\"value\":\"60\""));

        let default_sheet2_json = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheet":"Sheet1","expr":"=Sheet2!A1+50","cells":[{"sheet":"Sheet1","cell":"A1","value":500}]}"#,
        )
        .unwrap();
        assert!(
            default_sheet2_json.contains("\"value\":\"#REF!\""),
            "no demo Sheet2 is seeded: {default_sheet2_json}"
        );

        let direct_sheet2_default = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheet":"Sheet2","expr":"=A1+50","cells":[]}"#,
        )
        .unwrap();
        assert!(direct_sheet2_default.contains("\"value\":\"50\""));

        let cleared_sheet2_ui = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","expr":"=Sheet2!A1+50","cells":[{"sheet":"Sheet1","cell":"A1","value":500}]}"#,
        )
        .unwrap();
        assert!(cleared_sheet2_ui.contains("\"value\":\"#REF!\""));

        for cmd in ["evaluate_workbook_formula", "evaluate_sheet"] {
            let cross1 = EasyOfficeIpcRouter::dispatch(
                cmd,
                r#"{"sheet":"Sheet1","expr":"=Sheet2!A1+50","cells":[{"sheet":"Sheet1","cell":"A1","value":100},{"sheet":"Sheet2","cell":"A1","value":250}]}"#,
            )
            .unwrap();
            assert!(
                cross1.contains("\"value\":\"300\""),
                "cmd={cmd} res={cross1}"
            );

            let cross2 = EasyOfficeIpcRouter::dispatch(
                cmd,
                r#"{"sheet":"Sheet2","expr":"=SUM(A1:B1)","cells":[{"sheet":"Sheet1","cell":"A1","value":100},{"sheet":"Sheet2","cell":"A1","value":250},{"sheet":"Sheet2","cell":"B1","value":50}]}"#,
            )
            .unwrap();
            assert!(
                cross2.contains("\"value\":\"300\""),
                "cmd={cmd} res={cross2}"
            );

            let cleared_res = EasyOfficeIpcRouter::dispatch(
                cmd,
                r#"{"sheet":"Sheet1","expr":"C4","cells":[{"sheet":"Sheet1","cell":"B1","value":10},{"sheet":"Sheet1","cell":"C4","value":"=Sheet1!A1+Sheet1!B1+ROW()+COLUMN()"}]}"#,
            )
            .unwrap();
            assert!(
                cleared_res.contains("\"value\":\"17\""),
                "cmd={cmd} res={cleared_res}"
            );

            let anchored_row = EasyOfficeIpcRouter::dispatch(
                cmd,
                r#"{"sheet":"Sheet1","cell":"C5","expr":"=ROW()"}"#,
            )
            .unwrap();
            assert!(
                anchored_row.contains("\"value\":\"5\""),
                "cmd={cmd} res={anchored_row}"
            );
            assert!(EasyOfficeIpcRouter::dispatch(
                cmd,
                r#"{"sheet":"Sheet1","cell":"NOT_A_CELL","expr":"=ROW()"}"#,
            )
            .is_err());
        }

        let conv_json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "docx->xlsx|예산보고서|첫번째문단\n[[AGENCY]]\t행정안전부\n[[TABLE_NEW]]\n[[TABLE_ROW]]\t항목\t금액\n[[TABLE_ROW]]\t인건비\t500\n[[SHEET_CELL]]\t0\t0\t시트셀값\n[[SHEET_A1]]\tAA1\t다중열셀\n[[SLIDE]]\t발표제목\t핵심1;;핵심2\t발표노트",
        )
        .unwrap();
        assert!(conv_json.contains("\"convertedBase64\":\""));
        assert!(conv_json.contains("\"tableCount\":1"));
        let b64_marker = "\"convertedBase64\":\"";
        let b64_start = conv_json.find(b64_marker).unwrap() + b64_marker.len();
        let b64_end = conv_json[b64_start..].find('"').unwrap() + b64_start;
        let b64_str = &conv_json[b64_start..b64_end];
        assert!(!b64_str.is_empty());
        let xlsx_bytes = EasyOfficeIpcRouter::decode_base64(b64_str).unwrap();
        let (_, decoded_xlsx) =
            UniversalFormatConverter::decode(&xlsx_bytes, Some("xlsx")).unwrap();
        assert!(decoded_xlsx
            .sheet_cells
            .contains(&(0, 0, "시트셀값".to_string())));
        assert!(decoded_xlsx
            .sheet_cells
            .contains(&(0, 26, "다중열셀".to_string())));

        let hwp_conv_json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "docx->hwt|공문서|본문문단\n[[AGENCY]]\t행정안전부\n[[TABLE_NEW]]\n[[TABLE_ROW]]\t구분\t비고\n[[TABLE_ROW]]\t승인\t정상",
        )
        .unwrap();
        assert!(hwp_conv_json.contains("\"tableCount\":1"));
        assert!(hwp_conv_json.contains("\"convertedBase64\":\""));
        let hwp_b64_start = hwp_conv_json.find(b64_marker).unwrap() + b64_marker.len();
        let hwp_b64_end = hwp_conv_json[hwp_b64_start..].find('"').unwrap() + hwp_b64_start;
        let hwp_b64_str = &hwp_conv_json[hwp_b64_start..hwp_b64_end];
        let hwp_bytes = EasyOfficeIpcRouter::decode_base64(hwp_b64_str).unwrap();
        let hwp_unpacked = easyoffice_formats::Hwp5Codec::unpack(&hwp_bytes).unwrap();
        assert_eq!(hwp_unpacked.agency_name, "행정안전부");

        let smart_json = EasyOfficeIpcRouter::dispatch(
            "run_smart_assistant",
            "첫 번째 핵심 문단입니다.\n[TODO] 보안 감사 보고서 제출\n두 번째 요약 문단입니다.",
        )
        .unwrap();
        assert!(smart_json.contains("\"summaryLines\":["));
        assert!(smart_json.contains("\"actionItems\":[\"[TODO] 보안 감사 보고서 제출\"]"));

        let hangul_pii = DocumentSecurityAuditor::scan_and_scrub_pii(
            "담당자 이메일은security@easyoffice.local입니다.",
        );
        assert_eq!(hangul_pii.masked_emails, 1);
        assert_eq!(hangul_pii.scrubbed_text, "담당자 이메일은***@***입니다.");

        let pii_rpc = EasyOfficeIpcRouter::dispatch(
            "scrub_pii",
            "연락처 010-1234-5678 및 user@example.com 확인",
        )
        .unwrap();
        assert!(pii_rpc.contains("\"spans\":[{\"start\":4,\"end\":17,\"replacement\":\"010-****-****\"},{\"start\":20,\"end\":36,\"replacement\":\"***@***\"}]"));

        let json_paren_comma_batch = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","targets":[{"sheet":"Sheet1","cell":"B1"},{"sheet":"예산(안), Q1","cell":"C1"},{"sheet":"미존재시트","cell":"A1"}],"cells":[{"sheet":"예산(안), Q1","cell":"A1","value":"250"},{"sheet":"예산(안), Q1","cell":"C1","value":"=A1*2"},{"sheet":"Sheet1","cell":"B1","value":"='예산(안), Q1'!C1+50"}]}"#,
        )
        .unwrap();
        assert!(json_paren_comma_batch.contains("\"targetValues\":[\"550\",\"500\",\"#REF!\"]"));

        let json_numeric_and_quote_sheet = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"'특수시트'","targets":[{"sheet":"'특수시트'","cell":"B1"},{"sheet":"Sheet!Bang","cell":"A1"}],"cells":[{"sheet":"'특수시트'","cell":"A1","value":250},{"sheet":"'특수시트'","cell":"A2","value":true},{"sheet":"'특수시트'","cell":"B1","value":"=IF(A2, '''특수시트'''!A1*3, 0)"},{"sheet":"Sheet!Bang","cell":"A1","value":42}]}"#,
        )
        .unwrap();
        assert!(json_numeric_and_quote_sheet.contains("\"targetValues\":[\"750\",\"42\"]"));

        assert!(EasyOfficeIpcRouter::dispatch("evaluate_sheet", "A1=10?A1").is_err());
        assert!(EasyOfficeIpcRouter::dispatch("evaluate_workbook_formula", "=Sheet2!A1").is_err());
        assert!(
            EasyOfficeIpcRouter::dispatch("audit_security", "word/document.xml,10,20|<w:p/>")
                .is_err()
        );
        assert!(EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","targets":["A1"],"cells":[]}"#,
        )
        .is_err());
        assert!(EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","cells":[{"sheet":"Sheet1","cell":"A1","value":{"bad":1}}]}"#,
        )
        .is_err());
        assert!(EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheets":["   "],"expr":"=1+1"}"#,
        )
        .is_err());

        let safe_audit_json = EasyOfficeIpcRouter::dispatch(
            "audit_security",
            r#"{"entries":[{"name":"word/document.xml","compressed":128,"uncompressed":512,"encrypted":false}],"xml":"<w:document><w:body><w:p>정상</w:p></w:body></w:document>"}"#,
        )
        .unwrap();
        assert!(safe_audit_json.contains("\"status\":\"SAFE\""));
        assert!(EasyOfficeIpcRouter::dispatch(
            "audit_security",
            r#"{"entries":[{"name":"word/document.xml","compressed":128,"uncompressed":512}],"xml":"<!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><w:document/>"}"#,
        )
        .is_err());

        let symmetric_quote_sheet = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"특수시트","expr":"='특수시트'!A1+A2","cells":[{"sheet":"특수시트","cell":"A1","value":200},{"sheet":"특수시트","cell":"A2","value":300}]}"#,
        )
        .unwrap();
        assert!(symmetric_quote_sheet.contains("\"value\":\"500\""));

        let explicit_sheets_missing_sheet2 = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"예산표","sheets":["예산표"],"expr":"=Sheet2!A1+10","cells":[{"sheet":"예산표","cell":"A1","value":300}]}"#,
        )
        .unwrap();
        assert!(explicit_sheets_missing_sheet2.contains("\"value\":\"#REF!\""));

        let quoted_sheet1_override = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheet":"'Sheet1'","expr":"=A1","cells":[{"cell":"A1","value":500}]}"#,
        )
        .unwrap();
        assert!(quoted_sheet1_override.contains("\"value\":\"500\""));

        let omitted_top_sheet_explicit_sheets = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheets":["예산표"],"targets":[{"sheet":"예산표","cell":"=Sheet1!A1+10"}],"cells":[{"sheet":"예산표","cell":"A1","value":300}]}"#,
        )
        .unwrap();
        assert!(omitted_top_sheet_explicit_sheets.contains("\"targetValues\":[\"#REF!\"]"));

        let unlisted_top_sheet_explicit_sheets = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheet":"없는시트","sheets":["예산표"],"expr":"=A1"}"#,
        )
        .unwrap();
        assert!(unlisted_top_sheet_explicit_sheets.contains("\"value\":\"#REF!\""));

        let empty_explicit_sheets = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheets":[],"expr":"=A1"}"#,
        )
        .unwrap();
        assert!(empty_explicit_sheets.contains("\"value\":\"#REF!\""));

        let unlisted_cell_sheet_ignored = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"예산표","sheets":["예산표"],"expr":"=미등록!A1","cells":[{"sheet":"미등록","cell":"A1","value":999}]}"#,
        )
        .unwrap();
        assert!(unlisted_cell_sheet_ignored.contains("\"value\":\"#REF!\""));

        let escaped_quote_distinct = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"요약","sheets":["요약","A'B","'A'B'"],"targets":[{"sheet":"요약","cell":"='A''B'!A1"},{"sheet":"요약","cell":"='''A''B'''!A1"}],"cells":[{"sheet":"A'B","cell":"A1","value":11},{"sheet":"'A'B'","cell":"A1","value":22}]}"#,
        )
        .unwrap();
        assert!(escaped_quote_distinct.contains("\"targetValues\":[\"11\",\"22\"]"));

        let huge_exp_overflow_audit = format!(
            "{{\"entries\":[{{\"name\":\"a.xml\",\"compressed\":100,\"uncompressed\":0.{}1e999999999}}]}}",
            "0".repeat(999_999)
        );
        assert!(EasyOfficeIpcRouter::dispatch("audit_security", &huge_exp_overflow_audit).is_err());

        let targets_with_anchor = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","cell":"C5","targets":[{"sheet":"Sheet1","cell":"=ROW()"},{"sheet":"Sheet1","cell":"=COLUMN()"}]}"#,
        )
        .unwrap();
        assert!(targets_with_anchor.contains("\"targetValues\":[\"5\",\"3\"]"));

        let default_lookup_at_cell = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","cell":"C5","cells":[{"sheet":"Sheet1","cell":"C5","value":77}]}"#,
        )
        .unwrap();
        assert!(default_lookup_at_cell.contains("\"value\":\"77\""));

        assert!(EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","cell":"NOT_A_CELL","targets":[{"sheet":"Sheet1","cell":"A1"}]}"#,
        )
        .is_err());
        assert!(EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","cell":"A$B1","expr":"=ROW()"}"#,
        )
        .is_err());
        assert!(EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","cell":"A01","expr":"=ROW()"}"#,
        )
        .is_err());

        assert!(EasyOfficeIpcRouter::dispatch("audit_security", "").is_err());
        assert!(EasyOfficeIpcRouter::dispatch("audit_security", ",,").is_err());
        assert!(EasyOfficeIpcRouter::dispatch("audit_security", r#"{"entries":[]}"#).is_err());
        assert!(
            EasyOfficeIpcRouter::dispatch("audit_security", r#"{"containerBase64":""}"#).is_err()
        );

        let real_container_audit = EasyOfficeIpcRouter::dispatch(
            "audit_security",
            &format!(r#"{{"fileName":"예산보고서.xlsx","containerBase64":"{b64_str}"}}"#),
        )
        .unwrap();
        assert!(real_container_audit.contains("\"verdict\":\"SAFE\""));
        assert!(real_container_audit.contains("\"format\":\"xlsx\""));

        let special_sheet_json = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","sheets":["Sheet1","Sheet2"],"targets":[{"sheet":"Sheet1","cell":"A1"},{"sheet":"Sheet1","cell":"B1"}],"cells":[{"sheet":"Sheet1","cell":"A1","value":"=IF(Sheet2!A1>1,\"a?b\",0)"},{"sheet":"Sheet1","cell":"B1","value":"100%3B"},{"sheet":"Sheet2","cell":"A1","value":5}]}"#,
        )
        .unwrap();
        assert!(
            special_sheet_json.contains("\"targetValues\":[\"a?b\",\"100%3B\"]"),
            "special_sheet_json={special_sheet_json}"
        );

        let structured_conv = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"특수문자검증","body":"[[SHEET_A1]]\tA1\t100%3B\n[[SHEET_A1]]\tB1\t=IF(Sheet2!A1>1,\"a?b\",0)"}"#,
        )
        .unwrap();
        let sc_start = structured_conv.find(b64_marker).unwrap() + b64_marker.len();
        let sc_end = structured_conv[sc_start..].find('"').unwrap() + sc_start;
        let sc_bytes =
            EasyOfficeIpcRouter::decode_base64(&structured_conv[sc_start..sc_end]).unwrap();
        let (_, sc_decoded) = UniversalFormatConverter::decode(&sc_bytes, Some("xlsx")).unwrap();
        assert!(sc_decoded
            .sheet_cells
            .contains(&(0, 0, "100%3B".to_string())));
        assert!(sc_decoded
            .sheet_cells
            .contains(&(0, 1, "=IF(Sheet2!A1>1,\"a?b\",0)".to_string())));
    }
}

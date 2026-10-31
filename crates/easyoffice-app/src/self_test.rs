use easyoffice_formats::{
    deflate_raw, inflate_raw, CommonMarkCodec, CsvCodec, DocumentFormat, EasyOfficeError,
    FormatDetector, Hwp5Codec, Hwp5DocumentPayload, Hwp5ParagraphRecord, Ole2SecurityGuard,
    PdfDocumentModel, PdfPageModel, PdfSerializer, UniversalDocumentModel,
    UniversalFormatConverter, XmlSanitizer, ZipEntryDescriptor, ZipSecurityGuard,
    ZipSecurityPolicy,
};
use easyoffice_layout::{
    ContextualObjectToolKind, DesktopPlatform, DocumentDomainEngine, DocumentPageLayout,
    DocumentTableModel, LayoutRect, OfficeModuleKind, PageOrientation, PdfDomainEngine,
    PdfStudioLayout, RibbonLayoutStyle, SlideDeckTheme, SlideDomainEngine, SlideStageLayout,
};
use easyoffice_sheet::{
    CellRange, EvaluatedCellValue, PivotAggregation, SheetEngine, WorkbookEngine,
};

use crate::{
    invoke_c_abi_roundtrip, run_deterministic_benchmarks, DocumentSecurityAuditor,
    EasyOfficeIpcRouter, RibbonCommandRouter, RibbonWorkspaceState,
};

/// Executes deterministic end-to-end self-tests across all 4 crates and embedded Tauri UI assets.
pub fn run_self_test() -> Result<String, EasyOfficeError> {
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
    if SheetEngine::supported_function_names().len() != 122 {
        return Err(EasyOfficeError::FormulaError(format!(
            "지원 수식 함수 수가 122종이 아닙니다: {}",
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

    let tbl = DocumentTableModel::new(vec![
        vec!["100".to_string()],
        vec!["250".to_string()],
        vec!["=SUM(ABOVE)".to_string()],
    ]);
    if (tbl.evaluate_cell_formula(2, 0)? - 350.0).abs() > 1e-6 {
        return Err(EasyOfficeError::FormulaError(
            "표 =SUM(ABOVE) 수식 검증 실패".to_string(),
        ));
    }
    let mut doc_domain = DocumentDomainEngine::new();
    doc_domain.record_revision("r1", "검토자", "insert", "초안 추가");
    if !doc_domain.accept_revision("r1") {
        return Err(EasyOfficeError::InvalidArgument(
            "문서 변경 내용 추적 승인 실패".to_string(),
        ));
    }
    let mut slide_domain = SlideDomainEngine::default();
    slide_domain.presenter.move_laser_pointer(300.0, 200.0);
    slide_domain.presenter.append_ink_point(120.0, 180.0);
    if slide_domain.presenter.laser_pointer != Some((300.0, 200.0)) {
        return Err(EasyOfficeError::InvalidArgument(
            "발표자 레이저 포인터 검증 실패".to_string(),
        ));
    }
    let mut pdf_domain = PdfDomainEngine::new(pdf_model.clone());
    if pdf_domain.rotate_page(0, 90)? != 90 {
        return Err(EasyOfficeError::InvalidArgument(
            "PDF 페이지 회전 검증 실패".to_string(),
        ));
    }

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

    let conv_json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        "docx->hwpx|기관보고서|1. 개요\n2. 세부 계획",
    )?;
    if !conv_json.contains("\"targetFormat\":\"hwpx\"")
        || !conv_json.contains("\"converted_base64\":\"")
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
    if !bootstrap.contains("#5C6BC0") || !bootstrap.contains("HWP/HWPX") {
        return Err(EasyOfficeError::InvalidArgument(
            "IPC 부트스트랩 응답 검증 실패".to_string(),
        ));
    }
    let html_asset = include_str!("../ui/index.html");
    let css_asset = include_str!("../ui/styles.css");
    let offline_ast_asset = include_str!("../ui/offline-formula-ast.js");
    let offline_eval_asset = include_str!("../ui/offline-formula-eval.js");
    let offline_rpc_asset = include_str!("../ui/offline-rpc.js");
    let pii_scrubber_asset = include_str!("../ui/pii-dom-scrubber.js");
    let sheet_store_asset = include_str!("../ui/sheet-store.js");
    let ribbon_catalog_asset = include_str!("../ui/ribbon-catalog.js");
    let workspace_tools_asset = include_str!("../ui/workspace-tools.js");
    let workspace_panels_asset = include_str!("../ui/workspace-panels.js");
    let sheet_controller_asset = include_str!("../ui/sheet-controller.js");
    let ribbon_history_asset = include_str!("../ui/ribbon-history.js");
    let js_asset = include_str!("../ui/app.js");
    let tauri_conf = include_str!("../tauri.conf.json");
    if !html_asset.contains("EasyOffice")
        || !css_asset.contains("#5C6BC0")
        || !offline_ast_asset.contains("__EASYOFFICE_FORMULA_AST__")
        || !offline_eval_asset.contains("__EASYOFFICE_OFFLINE_FORMULA_EVAL__")
        || !offline_rpc_asset.contains("__EASYOFFICE_CREATE_OFFLINE_RPC__")
        || !pii_scrubber_asset.contains("__EASYOFFICE_CREATE_PII_SCRUBBER__")
        || !sheet_store_asset.contains("__EASYOFFICE_CREATE_SHEET_STORE__")
        || !ribbon_catalog_asset.contains("__EASYOFFICE_RIBBON_CATALOG__")
        || !workspace_tools_asset.contains("__EASYOFFICE_CREATE_WORKSPACE_TOOLS__")
        || !workspace_panels_asset.contains("__EASYOFFICE_CREATE_WORKSPACE_PANELS__")
        || !sheet_controller_asset.contains("__EASYOFFICE_CREATE_SHEET_CONTROLLER__")
        || !ribbon_history_asset.contains("__EASYOFFICE_CREATE_RIBBON_HISTORY__")
        || !js_asset.contains("compositionstart")
        || !tauri_conf.contains("downloadBootstrapper")
    {
        return Err(EasyOfficeError::CorruptedContainer(
            "Tauri v2 UI 또는 번들 설정 자산 무결성 검증 실패".to_string(),
        ));
    }

    let bench_report = run_deterministic_benchmarks(5);
    if !bench_report.all_equivalent || bench_report.suites.len() != 5 {
        return Err(EasyOfficeError::CorruptedContainer(
            "엔진 저수준 최적화 벤치마크 동등성 검증 실패".to_string(),
        ));
    }

    Ok(format!(
        "EasyOffice Rust Cross-Platform Self-Test PASSED (platform={}, formats=20, functions=122, ribbon_commands=140, ipc_commands=15, {})",
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
        assert!(default_sheet2_json.contains("\"value\":\"150\""));

        let direct_sheet2_default = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            r#"{"sheet":"Sheet2","expr":"=A1+50","cells":[]}"#,
        )
        .unwrap();
        assert!(direct_sheet2_default.contains("\"value\":\"150\""));

        let cleared_sheet2_ui = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            r#"{"sheet":"Sheet1","expr":"=Sheet2!A1+50","cells":[{"sheet":"Sheet1","cell":"A1","value":500}]}"#,
        )
        .unwrap();
        assert!(cleared_sheet2_ui.contains("\"value\":\"50\""));

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
            "docx->hwp|공문서|본문문단\n[[AGENCY]]\t행정안전부\n[[TABLE_NEW]]\n[[TABLE_ROW]]\t구분\t비고\n[[TABLE_ROW]]\t승인\t정상",
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
    }
}

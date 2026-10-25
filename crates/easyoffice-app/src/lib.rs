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
    PdfStudioLayout, RibbonLayoutStyle, SignatureLavenderPalette, SlideDeckTheme,
    SlideDomainEngine, SlideStageLayout,
};
use easyoffice_sheet::{
    CellRange, ConditionalFormatRule, DataValidationRule, EvaluatedCellValue, PivotAggregation,
    SheetEngine, WorkbookEngine,
};

pub mod benchmark;
pub mod ffi;
pub mod ribbon;
pub mod smart_tools;

pub use benchmark::*;
pub use ffi::*;
pub use ribbon::*;
pub use smart_tools::*;

static RIBBON_WORKSPACE_STATE: std::sync::OnceLock<std::sync::Mutex<RibbonWorkspaceState>> =
    std::sync::OnceLock::new();

fn shared_ribbon_state() -> &'static std::sync::Mutex<RibbonWorkspaceState> {
    RIBBON_WORKSPACE_STATE.get_or_init(|| std::sync::Mutex::new(RibbonWorkspaceState::default()))
}

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

    pub const RECENT_FORMAT_FILTERS: [&'static str; 7] =
        ["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"];

    pub fn dispatch(command: &str, payload: &str) -> Result<String, EasyOfficeError> {
        match command {
            "get_workspace_bootstrap" => Self::handle_workspace_bootstrap(),
            "detect_format" => Self::handle_detect_format(payload),
            "evaluate_sheet" | "evaluate_sheet_formula" => Self::handle_evaluate_sheet(payload),
            "evaluate_workbook_formula" => Self::handle_evaluate_workbook_formula(payload),
            "compute_layout" => Self::handle_compute_layout(payload),
            "audit_security" => Self::handle_audit_security(payload),
            "execute_ribbon_command" => Self::handle_execute_ribbon_command(payload),
            "switch_ribbon_style" => Self::handle_switch_ribbon_style(payload),
            "run_smart_assistant" => Self::handle_run_smart_assistant(payload),
            "scrub_pii" => Self::handle_scrub_pii(payload),
            "convert_document_format" => Self::handle_convert_document_format(payload),
            "get_ribbon_category_commands" => Self::handle_get_ribbon_category_commands(payload),
            "run_engine_benchmark" => Self::handle_run_engine_benchmark(payload),
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
                "{{\"id\":\"tab-doc\",\"title\":\"분기_경영실적_보고서.docx\",\"module\":\"DOC\",\"isPinned\":true,\"isFavorite\":true}},",
                "{{\"id\":\"tab-hwp\",\"title\":\"행정안전부_기안문.hwpx\",\"module\":\"HWP\",\"isPinned\":true,\"isFavorite\":false}},",
                "{{\"id\":\"tab-sheet\",\"title\":\"예산집행_총괄표.xlsx\",\"module\":\"SHEET\",\"isPinned\":false,\"isFavorite\":true}},",
                "{{\"id\":\"tab-slide\",\"title\":\"전략기획_발표자료.pptx\",\"module\":\"SLIDE\",\"isPinned\":false,\"isFavorite\":false}},",
                "{{\"id\":\"tab-pdf\",\"title\":\"최종계약서_검토본.pdf\",\"module\":\"PDF\",\"isPinned\":false,\"isFavorite\":true}}",
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
            Self::escape_json_str(fmt.extension()),
            Self::escape_json_str(fmt.container_family().as_str()),
            Self::escape_json_str(fmt.standard_specification()),
            bytes.len()
        ))
    }

    fn is_cell_key_prefix(s: &str) -> bool {
        let trimmed = s.trim_start();
        let Some((lhs, _)) = trimmed.split_once('=') else {
            return false;
        };
        let key = lhs.trim();
        let ref_part = key.split_once('!').map(|(_, r)| r.trim()).unwrap_or(key);
        let bytes = ref_part.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i == 0 || i > 3 || i == bytes.len() {
            return false;
        }
        bytes[i..].iter().all(|b| b.is_ascii_digit())
    }

    fn split_cell_assignments(cells_part: &str) -> Vec<(String, String)> {
        let mut items = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut paren_depth = 0usize;
        let mut chars = cells_part.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == ';' || next_ch == '?' || next_ch == '\\' {
                        current.push(chars.next().unwrap());
                        continue;
                    }
                }
                current.push(ch);
                continue;
            }
            if ch == '"' {
                in_quotes = !in_quotes;
                current.push(ch);
                continue;
            }
            if !in_quotes {
                if ch == '(' {
                    paren_depth += 1;
                } else if ch == ')' {
                    paren_depth = paren_depth.saturating_sub(1);
                } else if ch == ';' && paren_depth == 0 {
                    let rest: String = chars.clone().collect();
                    if rest.trim().is_empty() || Self::is_cell_key_prefix(&rest) {
                        let seg = std::mem::take(&mut current);
                        if let Some((k, v)) = seg.trim().split_once('=') {
                            items.push((
                                k.trim().to_string(),
                                v.trim().replace("%3B", ";").replace("%3F", "?"),
                            ));
                        }
                        continue;
                    }
                }
            }
            current.push(ch);
        }
        if let Some((k, v)) = current.trim().split_once('=') {
            items.push((
                k.trim().to_string(),
                v.trim().replace("%3B", ";").replace("%3F", "?"),
            ));
        }
        items
    }

    /// Payload format: semicolon-separated `A1=val` pairs followed by `?target_expr_or_cell`
    /// Example: `A1=100;A2=200;A3==SUM(A1:A2)?A3`
    fn handle_evaluate_sheet(payload: &str) -> Result<String, EasyOfficeError> {
        let (cells_part, query_part) = payload.rsplit_once('?').unwrap_or((payload, "A1"));
        let mut sheet = SheetEngine::new("IPC_Sheet");
        for (addr, val) in Self::split_cell_assignments(cells_part) {
            if !addr.is_empty() {
                sheet.set_cell(&addr, &val)?;
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
            Self::escape_json_str(query_part.trim()),
            Self::escape_json_str(&evaluated.to_display_string()),
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
        let pdf_layout = PdfStudioLayout::compute("승인필");

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
            Self::escape_json_str(&pdf_layout.stamp_label)
        ))
    }

    fn handle_audit_security(payload: &str) -> Result<String, EasyOfficeError> {
        let (entries_raw, xml_part) = match payload.split_once('|') {
            Some((left, right)) => (Some(left.trim()), right.trim()),
            None => (None, payload.trim()),
        };
        if !xml_part.is_empty() {
            XmlSanitizer::validate_safe_xml(xml_part)?;
        }
        let policy = ZipSecurityPolicy::default();
        let mut parsed_entries: Vec<ZipEntryDescriptor> = Vec::new();
        if let Some(entries_segment) = entries_raw {
            if !entries_segment.is_empty() {
                for item in entries_segment.split(';') {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    let cols: Vec<&str> = item.split(',').map(str::trim).collect();
                    if cols.len() < 3 {
                        return Err(EasyOfficeError::InvalidArgument(
                            "ZIP 엔트리 명세는 'path,compressed,uncompressed[,encrypted]' 형식이어야 합니다."
                                .to_string(),
                        ));
                    }
                    let compressed = cols[1].parse::<u64>().map_err(|_| {
                        EasyOfficeError::InvalidArgument(
                            "유효하지 않은 compressed_size".to_string(),
                        )
                    })?;
                    let uncompressed = cols[2].parse::<u64>().map_err(|_| {
                        EasyOfficeError::InvalidArgument(
                            "유효하지 않은 uncompressed_size".to_string(),
                        )
                    })?;
                    let encrypted = cols
                        .get(3)
                        .map(|v| v.eq_ignore_ascii_case("true") || *v == "1")
                        .unwrap_or(false);
                    parsed_entries.push(ZipEntryDescriptor::new(
                        cols[0],
                        compressed,
                        uncompressed,
                        encrypted,
                    ));
                }
            }
        }
        ZipSecurityGuard::validate_archive_entries(&parsed_entries, &policy)?;
        Ok(format!(
            "{{\"status\":\"SAFE\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\",\"checkedEntries\":{}}}",
            parsed_entries.len()
        ))
    }

    fn handle_execute_ribbon_command(payload: &str) -> Result<String, EasyOfficeError> {
        let mut guard = shared_ribbon_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let summary = RibbonCommandRouter::execute_command(payload.trim(), &mut guard)?;
        let badge = guard.active_contextual_tool().badge_title();
        Ok(format!(
            concat!(
                "{{",
                "\"commandId\":\"{}\",",
                "\"summary\":\"{}\",",
                "\"contextualBadge\":\"{}\",",
                "\"catalogCount\":140,",
                "\"isBold\":{},",
                "\"isItalic\":{},",
                "\"isUnderline\":{},",
                "\"columnCount\":{},",
                "\"isLandscape\":{},",
                "\"manuscriptGridEnabled\":{},",
                "\"approvalHeaderEnabled\":{},",
                "\"trackChangesEnabled\":{},",
                "\"splitViewEnabled\":{},",
                "\"ribbonCollapsed\":{},",
                "\"darkThemeEnabled\":{},",
                "\"zoomPercent\":{},",
                "\"pdfRotationDegrees\":{},",
                "\"executedCount\":{}",
                "}}"
            ),
            Self::escape_json_str(&guard.last_command_id),
            Self::escape_json_str(&summary),
            Self::escape_json_str(badge),
            guard.is_bold,
            guard.is_italic,
            guard.is_underline,
            guard.column_count,
            guard.is_landscape,
            guard.manuscript_grid_enabled,
            guard.approval_header_enabled,
            guard.track_changes_enabled,
            guard.split_view_enabled,
            guard.ribbon_collapsed,
            guard.dark_theme_enabled,
            guard.zoom_percent,
            guard.pdf_rotation_degrees,
            guard.executed_count
        ))
    }

    fn handle_switch_ribbon_style(payload: &str) -> Result<String, EasyOfficeError> {
        let style = match payload.trim().to_ascii_lowercase().as_str() {
            "hangul" | "hwp" | "hangul_hwp" | "한글" => RibbonLayoutStyle::HangulHwp,
            "standard" | "standard_office" | "표준" => RibbonLayoutStyle::StandardOffice,
            other => {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "지원하지 않는 리본 레이아웃 모드입니다: {other}"
                )))
            }
        };
        if let Ok(mut guard) = shared_ribbon_state().lock() {
            guard.ribbon_style = style;
        }
        let cats = style
            .category_titles()
            .iter()
            .map(|c| format!("\"{}\"", Self::escape_json_str(c)))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!(
            "{{\"style\":\"{}\",\"categories\":[{}]}}",
            Self::escape_json_str(style.display_name()),
            cats
        ))
    }

    fn handle_run_smart_assistant(payload: &str) -> Result<String, EasyOfficeError> {
        let paragraphs: Vec<String> = payload
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let summary = LocalSmartAssistant::summarize_document(&paragraphs, 2);
        let diag = LocalSmartAssistant::diagnose_writing(payload);
        let actions = LocalSmartAssistant::extract_action_items(&paragraphs);
        let summary_json = summary
            .iter()
            .map(|s| format!("\"{}\"", Self::escape_json_str(s)))
            .collect::<Vec<_>>()
            .join(",");
        let suggestions_json = diag
            .suggestions
            .iter()
            .map(|s| format!("\"{}\"", Self::escape_json_str(s)))
            .collect::<Vec<_>>()
            .join(",");
        let actions_json = actions
            .iter()
            .map(|a| format!("\"{}\"", Self::escape_json_str(a)))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!(
            concat!(
                "{{",
                "\"summaryCount\":{},",
                "\"charCount\":{},",
                "\"wordCount\":{},",
                "\"sentenceCount\":{},",
                "\"readingTimeSeconds\":{},",
                "\"actionItemCount\":{},",
                "\"suggestionCount\":{},",
                "\"summaryLines\":[{}],",
                "\"styleSuggestions\":[{}],",
                "\"actionItems\":[{}]",
                "}}"
            ),
            summary.len(),
            diag.char_count,
            diag.word_count,
            diag.sentence_count,
            diag.reading_time_seconds,
            actions.len(),
            diag.suggestions.len(),
            summary_json,
            suggestions_json,
            actions_json
        ))
    }

    fn handle_scrub_pii(payload: &str) -> Result<String, EasyOfficeError> {
        let audit = DocumentSecurityAuditor::scan_and_scrub_pii(payload);
        let escaped = Self::escape_json_str(&audit.scrubbed_text);
        Ok(format!(
            "{{\"scrubbedText\":\"{}\",\"maskedResidentIds\":{},\"maskedPhoneNumbers\":{},\"maskedEmails\":{},\"totalMasked\":{}}}",
            escaped,
            audit.masked_resident_ids,
            audit.masked_phone_numbers,
            audit.masked_emails,
            audit.total_masked_count()
        ))
    }

    fn handle_evaluate_workbook_formula(payload: &str) -> Result<String, EasyOfficeError> {
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("Sheet1");
        wb.add_sheet("Sheet2");
        wb.set_cell("Sheet2", "A1", "100")?;
        wb.set_cell("Sheet1", "A1", "1200")?;
        wb.set_cell("Sheet1", "B1", "850")?;
        let (cells_part, expr_part) = match payload.rsplit_once('?') {
            Some((c, e)) => (Some(c.trim()), e.trim()),
            None => (None, payload.trim()),
        };
        if let Some(cells_str) = cells_part {
            for (lhs, rhs) in Self::split_cell_assignments(cells_str) {
                let (sh, a1) = lhs.split_once('!').unwrap_or(("Sheet1", &lhs));
                wb.add_sheet(sh.trim());
                wb.set_cell(sh.trim(), a1.trim(), rhs.trim())?;
            }
        }
        let expr = if expr_part.is_empty() {
            "=Sheet2!A1+50"
        } else {
            expr_part
        };
        wb.set_cell("Sheet1", "C1", expr)?;
        let val = wb.evaluate_cell("Sheet1", "C1");
        Ok(format!(
            "{{\"sheet\":\"Sheet1\",\"formula\":\"{}\",\"value\":\"{}\"}}",
            Self::escape_json_str(expr),
            Self::escape_json_str(&val.to_display_string())
        ))
    }

    fn handle_run_engine_benchmark(payload: &str) -> Result<String, EasyOfficeError> {
        let iters = payload.trim().parse::<usize>().unwrap_or(5).clamp(3, 25);
        Ok(run_deterministic_benchmarks(iters).to_json())
    }

    fn handle_convert_document_format(payload: &str) -> Result<String, EasyOfficeError> {
        let parts: Vec<&str> = payload.split('|').collect();
        let (src_ext, dst_ext, title, body) = if let Some(first) = parts.first() {
            if let Some((s, d)) = first.split_once("->") {
                (
                    s.trim(),
                    d.trim(),
                    parts.get(1).copied().unwrap_or("변환_문서").trim(),
                    parts.get(2).copied().unwrap_or("본문 텍스트").trim(),
                )
            } else if parts.len() >= 2 {
                (
                    parts[0].trim(),
                    parts[1].trim(),
                    parts.get(2).copied().unwrap_or("변환_문서").trim(),
                    parts.get(3).copied().unwrap_or("본문 텍스트").trim(),
                )
            } else {
                ("docx", "hwpx", "변환_문서", "본문 텍스트")
            }
        } else {
            ("docx", "hwpx", "변환_문서", "본문 텍스트")
        };

        let src_fmt = DocumentFormat::from_extension(src_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 원본 포맷: {src_ext}"))
        })?;
        let dst_fmt = DocumentFormat::from_extension(dst_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 대상 포맷: {dst_ext}"))
        })?;

        let mut paragraphs: Vec<String> = body
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if paragraphs.is_empty() {
            paragraphs.push(title.to_string());
        }

        let model = UniversalDocumentModel {
            title: title.to_string(),
            paragraphs,
            tables: vec![vec![
                vec!["항목".to_string(), "값".to_string()],
                vec!["원본포맷".to_string(), src_fmt.extension().to_string()],
            ]],
            sheet_cells: vec![
                (0, 0, title.to_string()),
                (0, 1, "100".to_string()),
                (1, 1, "=B1*2".to_string()),
            ],
            slides: vec![(
                title.to_string(),
                vec![body.to_string()],
                "발표자 노트".to_string(),
            )],
            official_agency: Some("행정안전부".to_string()),
        };

        let src_bytes = UniversalFormatConverter::encode(&model, src_fmt)?;
        let dst_bytes =
            UniversalFormatConverter::convert(&src_bytes, Some(src_fmt.extension()), dst_fmt)?;
        let (detected_dst, decoded) =
            UniversalFormatConverter::decode(&dst_bytes, Some(dst_fmt.extension()))?;

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
                "\"paragraphCount\":{},",
                "\"title\":\"{}\",",
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
            dst_fmt.standard_specification(),
            decoded.paragraphs.len(),
            Self::escape_json_str(&decoded.title)
        ))
    }

    fn handle_get_ribbon_category_commands(payload: &str) -> Result<String, EasyOfficeError> {
        let (mod_str, cat_str) = payload.split_once('|').unwrap_or(("DOC", payload.trim()));
        let module = match mod_str.trim().to_ascii_uppercase().as_str() {
            "HWP" | "HWPX" => OfficeModuleKind::Hwp,
            "SHEET" | "XLSX" | "XLS" => OfficeModuleKind::Sheet,
            "SLIDE" | "PPTX" | "PPT" => OfficeModuleKind::Slide,
            "PDF" => OfficeModuleKind::Pdf,
            _ => OfficeModuleKind::Document,
        };
        let (category_key, category_label) = match cat_str.trim().to_ascii_lowercase().as_str() {
            "home" | "홈" => ("home", "홈"),
            "insert" | "삽입" => ("insert", "삽입"),
            "layout" | "레이아웃" | "쪽·원고지" => ("layout", "레이아웃"),
            "formulas" | "formulasanddata" | "수식·데이터" | "productivity" | "생산성·보안" => {
                ("formulasAndData", "수식·데이터")
            }
            "module" | "modulespecific" | "모듈 전용" => ("moduleSpecific", "모듈 전용"),
            "review" | "검토" => ("review", "검토"),
            "view" | "보기" => ("view", "보기"),
            _ => ("home", "홈"),
        };
        let items: Vec<String> = RibbonCommandRouter::all_commands()
            .into_iter()
            .filter(|c| c.module == module && c.category_key == category_key)
            .map(|c| {
                format!(
                    "{{\"id\":\"{}\",\"title\":\"{}\",\"shortcut\":\"{}\",\"category\":\"{}\"}}",
                    Self::escape_json_str(&c.id),
                    Self::escape_json_str(c.title),
                    Self::escape_json_str(c.shortcut),
                    Self::escape_json_str(category_label)
                )
            })
            .collect();
        Ok(format!(
            "{{\"module\":\"{:?}\",\"category\":\"{}\",\"commands\":[{}]}}",
            module,
            Self::escape_json_str(cat_str.trim()),
            items.join(",")
        ))
    }

    fn escape_json_str(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out
    }

    fn decode_hex(hex: &str) -> Result<Vec<u8>, EasyOfficeError> {
        let cleaned: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        if !cleaned.len().is_multiple_of(2) {
            return Err(EasyOfficeError::InvalidArgument(
                "16진수 문자열 길이가 짝수가 아닙니다.".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(cleaned.len() / 2);
        for &[hi_b, lo_b] in cleaned.as_chunks::<2>().0 {
            let hi = Self::hex_val(hi_b)?;
            let lo = Self::hex_val(lo_b)?;
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
    if !conv_json.contains("\"targetFormat\":\"hwpx\"") || !conv_json.contains("\"status\":\"OK\"")
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

    let bench_report = run_deterministic_benchmarks(5);
    if !bench_report.all_equivalent || bench_report.suites.len() != 5 {
        return Err(EasyOfficeError::CorruptedContainer(
            "엔진 저수준 최적화 벤치마크 동등성 검증 실패".to_string(),
        ));
    }

    Ok(format!(
        "EasyOffice Rust Cross-Platform Self-Test PASSED (platform={}, formats=20, functions=122, ribbon_commands=140, ipc_commands=13, {})",
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
        let second =
            EasyOfficeIpcRouter::dispatch("execute_ribbon_command", "document-home-bold").unwrap();
        assert!(second.contains("\"isBold\":false"));

        let rot1 =
            EasyOfficeIpcRouter::dispatch("execute_ribbon_command", "pdf-mod-rotate").unwrap();
        assert!(rot1.contains("\"pdfRotationDegrees\":90"));

        let eval_json = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            "A1=항목;설명;B1=100;C1=foo\\;B1=999?B1",
        )
        .unwrap();
        assert!(eval_json.contains("\"value\":\"100\""));

        let smart_json = EasyOfficeIpcRouter::dispatch(
            "run_smart_assistant",
            "첫 번째 핵심 문단입니다.\n[TODO] 보안 감사 보고서 제출\n두 번째 요약 문단입니다.",
        )
        .unwrap();
        assert!(smart_json.contains("\"summaryLines\":["));
        assert!(smart_json.contains("\"actionItems\":[\"[TODO] 보안 감사 보고서 제출\"]"));
    }
}

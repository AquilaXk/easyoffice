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
    CellRange, ConditionalFormatRule, DataValidationRule, EvaluatedCellValue, PackedCellCoordinate,
    PivotAggregation, SheetEngine, WorkbookEngine,
};

pub mod benchmark;
pub mod ffi;
pub mod ribbon;
pub mod smart_tools;

pub use benchmark::*;
pub use ffi::*;
pub use ribbon::*;
pub use smart_tools::*;

#[derive(Default)]
struct SharedRibbonSession {
    state: RibbonWorkspaceState,
    history: UndoRedoStack<RibbonWorkspaceState>,
}

static RIBBON_WORKSPACE_SESSION: std::sync::OnceLock<std::sync::Mutex<SharedRibbonSession>> =
    std::sync::OnceLock::new();

fn shared_ribbon_session() -> &'static std::sync::Mutex<SharedRibbonSession> {
    RIBBON_WORKSPACE_SESSION.get_or_init(|| std::sync::Mutex::new(SharedRibbonSession::default()))
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
            "undo_ribbon_command" => Self::handle_undo_ribbon_command(),
            "redo_ribbon_command" => Self::handle_redo_ribbon_command(),
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

    fn is_cell_key_prefix_at(s: &str) -> bool {
        let trimmed = s.trim_start();
        let mut max_len = trimmed.len().min(64);
        while max_len > 0 && !trimmed.is_char_boundary(max_len) {
            max_len -= 1;
        }
        let probe = &trimmed[..max_len];
        let Some((lhs, _)) = probe.split_once('=') else {
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

    fn split_unescaped_query(payload: &str) -> Option<(&str, &str)> {
        let bytes = payload.as_bytes();
        for i in (0..bytes.len()).rev() {
            if bytes[i] == b'?' {
                let mut backslashes = 0usize;
                let mut k = i;
                while k > 0 && bytes[k - 1] == b'\\' {
                    backslashes += 1;
                    k -= 1;
                }
                if backslashes % 2 == 0 {
                    return Some((&payload[..i], &payload[i + 1..]));
                }
            }
        }
        None
    }

    fn split_cell_assignments(cells_part: &str) -> Vec<(String, String)> {
        let mut items = Vec::new();
        let mut current = String::new();
        let mut seen_eq = false;
        let mut in_formula = false;
        let mut rhs_started = false;
        let mut in_quotes = false;
        let mut paren_depth = 0usize;
        let mut idx = 0usize;

        while idx < cells_part.len() {
            let rest_from_idx = &cells_part[idx..];
            let ch = rest_from_idx.chars().next().unwrap();
            let next_idx = idx + ch.len_utf8();

            if ch == '\\' {
                if let Some(next_ch) = cells_part[next_idx..].chars().next() {
                    if next_ch == ';' || next_ch == '?' || next_ch == '\\' {
                        if seen_eq && !rhs_started && !next_ch.is_whitespace() {
                            rhs_started = true;
                        }
                        current.push(next_ch);
                        idx = next_idx + next_ch.len_utf8();
                        continue;
                    }
                }
                current.push(ch);
                idx = next_idx;
                continue;
            }

            if !seen_eq && ch == '=' {
                seen_eq = true;
                current.push(ch);
                idx = next_idx;
                continue;
            }

            if seen_eq && !rhs_started && !ch.is_whitespace() {
                rhs_started = true;
                if ch == '=' {
                    in_formula = true;
                }
            }

            if in_formula && ch == '"' {
                in_quotes = !in_quotes;
                current.push(ch);
                idx = next_idx;
                continue;
            }

            if in_formula && !in_quotes {
                if ch == '(' {
                    paren_depth += 1;
                } else if ch == ')' {
                    paren_depth = paren_depth.saturating_sub(1);
                }
            }

            let can_split = if in_formula {
                !in_quotes && paren_depth == 0
            } else {
                true
            };

            if ch == ';' && can_split {
                let tail = &cells_part[next_idx..];
                if tail.trim().is_empty() || Self::is_cell_key_prefix_at(tail) {
                    let seg = std::mem::take(&mut current);
                    if let Some((k, v)) = seg.trim().split_once('=') {
                        items.push((
                            k.trim().to_string(),
                            v.trim().replace("%3B", ";").replace("%3F", "?"),
                        ));
                    }
                    seen_eq = false;
                    in_formula = false;
                    rhs_started = false;
                    in_quotes = false;
                    paren_depth = 0;
                    idx = next_idx;
                    continue;
                }
            }

            current.push(ch);
            idx = next_idx;
        }
        if let Some((k, v)) = current.trim().split_once('=') {
            items.push((
                k.trim().to_string(),
                v.trim().replace("%3B", ";").replace("%3F", "?"),
            ));
        }
        items
    }

    fn extract_active_sheet_and_expr(raw_expr: &str) -> (&str, &str) {
        if let Some((sh, rest)) = raw_expr.split_once('|') {
            if !sh.trim().is_empty() && !sh.starts_with('=') && !sh.contains('(') {
                return (sh.trim(), rest.trim());
            }
        }
        if let Some((sh, rest)) = raw_expr.split_once('!') {
            if !sh.trim().is_empty() && !sh.starts_with('=') && !sh.contains('(') {
                return (sh.trim(), rest.trim());
            }
        }
        ("Sheet1", raw_expr.trim())
    }

    fn evaluate_workbook_query<'a>(
        cells_part: Option<&str>,
        raw_query: &'a str,
        seed_defaults: bool,
    ) -> Result<(&'a str, &'a str, EvaluatedCellValue), EasyOfficeError> {
        let (active_sheet, expr) = Self::extract_active_sheet_and_expr(raw_query.trim());
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("Sheet1");
        wb.add_sheet("Sheet2");
        if seed_defaults {
            wb.set_cell("Sheet2", "A1", "100")?;
            wb.set_cell("Sheet1", "A1", "1200")?;
            wb.set_cell("Sheet1", "B1", "850")?;
        }
        wb.add_sheet(active_sheet);
        if let Some(cells_str) = cells_part {
            for (lhs, val) in Self::split_cell_assignments(cells_str) {
                let (sh, addr) = lhs.split_once('!').unwrap_or((active_sheet, &lhs));
                let sh_trim = sh.trim();
                let addr_trim = addr.trim();
                if !sh_trim.is_empty() && !addr_trim.is_empty() {
                    wb.add_sheet(sh_trim);
                    wb.set_cell(sh_trim, addr_trim, &val)?;
                }
            }
        }
        let evaluated = if expr.starts_with('=') {
            wb.evaluate_formula_on_sheet(active_sheet, expr)
        } else {
            wb.evaluate_cell(active_sheet, expr)
        };
        Ok((active_sheet, expr, evaluated))
    }

    fn handle_evaluate_sheet(payload: &str) -> Result<String, EasyOfficeError> {
        let (cells_part, query_part) =
            Self::split_unescaped_query(payload).unwrap_or((payload, "A1"));
        let (_, expr, evaluated) =
            Self::evaluate_workbook_query(Some(cells_part), query_part, false)?;
        let rule = ConditionalFormatRule::GreaterThan(100.0);
        let cf_match = rule.matches(&evaluated, &[]);
        let dv_rule = DataValidationRule::NumberRange {
            min: 0.0,
            max: 1_000_000.0,
        };
        let dv_valid = dv_rule.validate(&evaluated);

        Ok(format!(
            "{{\"query\":\"{}\",\"value\":\"{}\",\"conditionalMatch\":{},\"validationPassed\":{}}}",
            Self::escape_json_str(expr),
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

    fn format_ribbon_state_json(
        state: &RibbonWorkspaceState,
        summary: &str,
        can_undo: bool,
        can_redo: bool,
    ) -> String {
        let badge = state.active_contextual_tool().badge_title();
        format!(
            concat!(
                "{{",
                "\"commandId\":\"{}\",",
                "\"summary\":\"{}\",",
                "\"contextualBadge\":\"{}\",",
                "\"catalogCount\":140,",
                "\"isBold\":{},",
                "\"isItalic\":{},",
                "\"isUnderline\":{},",
                "\"stylePresetIndex\":{},",
                "\"insertedTableCount\":{},",
                "\"insertedChartCount\":{},",
                "\"insertedShapeCount\":{},",
                "\"insertedLinkCount\":{},",
                "\"paperSize\":\"{}\",",
                "\"columnCount\":{},",
                "\"isLandscape\":{},",
                "\"marginPresetIndex\":{},",
                "\"autosumAppliedCount\":{},",
                "\"formulaWizardOpen\":{},",
                "\"sortAscending\":{},",
                "\"validationGuardEnabled\":{},",
                "\"tocGenerated\":{},",
                "\"footnoteCount\":{},",
                "\"mailMergeAppliedCount\":{},",
                "\"tableFormulaEvaluatedCount\":{},",
                "\"manuscriptGridEnabled\":{},",
                "\"approvalHeaderEnabled\":{},",
                "\"clickHereFieldCount\":{},",
                "\"doubleDiagonalBorderEnabled\":{},",
                "\"pivotSummaryEnabled\":{},",
                "\"conditionalFormatEnabled\":{},",
                "\"freezePanesEnabled\":{},",
                "\"masterLayoutIndex\":{},",
                "\"snapGuideEnabled\":{},",
                "\"slideTransitionIndex\":{},",
                "\"presenterModeActive\":{},",
                "\"pdfHighlightCount\":{},",
                "\"pdfRotationDegrees\":{},",
                "\"pdfSplitMergeCount\":{},",
                "\"pdfConvertedToOfficeCount\":{},",
                "\"trackChangesEnabled\":{},",
                "\"reviewCommentCount\":{},",
                "\"securityVerifiedCount\":{},",
                "\"statsInspectedCount\":{},",
                "\"splitViewEnabled\":{},",
                "\"ribbonCollapsed\":{},",
                "\"darkThemeEnabled\":{},",
                "\"zoomPercent\":{},",
                "\"executedCount\":{},",
                "\"canUndo\":{},",
                "\"canRedo\":{}",
                "}}"
            ),
            Self::escape_json_str(&state.last_command_id),
            Self::escape_json_str(summary),
            Self::escape_json_str(badge),
            state.is_bold,
            state.is_italic,
            state.is_underline,
            state.style_preset_index,
            state.inserted_table_count,
            state.inserted_chart_count,
            state.inserted_shape_count,
            state.inserted_link_count,
            Self::escape_json_str(&state.paper_size),
            state.column_count,
            state.is_landscape,
            state.margin_preset_index,
            state.autosum_applied_count,
            state.formula_wizard_open,
            state.sort_ascending,
            state.validation_guard_enabled,
            state.toc_generated,
            state.footnote_count,
            state.mail_merge_applied_count,
            state.table_formula_evaluated_count,
            state.manuscript_grid_enabled,
            state.approval_header_enabled,
            state.click_here_field_count,
            state.double_diagonal_border_enabled,
            state.pivot_summary_enabled,
            state.conditional_format_enabled,
            state.freeze_panes_enabled,
            state.master_layout_index,
            state.snap_guide_enabled,
            state.slide_transition_index,
            state.presenter_mode_active,
            state.pdf_highlight_count,
            state.pdf_rotation_degrees,
            state.pdf_split_merge_count,
            state.pdf_converted_to_office_count,
            state.track_changes_enabled,
            state.review_comment_count,
            state.security_verified_count,
            state.stats_inspected_count,
            state.split_view_enabled,
            state.ribbon_collapsed,
            state.dark_theme_enabled,
            state.zoom_percent,
            state.executed_count,
            can_undo,
            can_redo
        )
    }

    fn handle_execute_ribbon_command(payload: &str) -> Result<String, EasyOfficeError> {
        let mut session = shared_ribbon_session()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = session.state.clone();
        let summary = RibbonCommandRouter::execute_command(payload.trim(), &mut session.state)?;
        session.history.push_state(snapshot);
        Ok(Self::format_ribbon_state_json(
            &session.state,
            &summary,
            session.history.can_undo(),
            session.history.can_redo(),
        ))
    }

    fn handle_undo_ribbon_command() -> Result<String, EasyOfficeError> {
        let mut session = shared_ribbon_session()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = session.state.clone();
        if let Some(prev) = session.history.undo(current) {
            session.state = prev;
        }
        Ok(Self::format_ribbon_state_json(
            &session.state,
            "실행 취소 완료",
            session.history.can_undo(),
            session.history.can_redo(),
        ))
    }

    fn handle_redo_ribbon_command() -> Result<String, EasyOfficeError> {
        let mut session = shared_ribbon_session()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = session.state.clone();
        if let Some(next) = session.history.redo(current) {
            session.state = next;
        }
        Ok(Self::format_ribbon_state_json(
            &session.state,
            "다시 실행 완료",
            session.history.can_undo(),
            session.history.can_redo(),
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
        if let Ok(mut session) = shared_ribbon_session().lock() {
            session.state.ribbon_style = style;
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
        let (cells_part, expr_part) = match Self::split_unescaped_query(payload) {
            Some((c, e)) => (Some(c.trim()), e.trim()),
            None => (None, payload.trim()),
        };
        let raw_expr = if expr_part.is_empty() {
            "=Sheet2!A1+50"
        } else {
            expr_part
        };
        let (active_sheet, expr, val) = Self::evaluate_workbook_query(cells_part, raw_expr, true)?;
        Ok(format!(
            "{{\"sheet\":\"{}\",\"formula\":\"{}\",\"value\":\"{}\"}}",
            Self::escape_json_str(active_sheet),
            Self::escape_json_str(expr),
            Self::escape_json_str(&val.to_display_string())
        ))
    }

    fn handle_run_engine_benchmark(payload: &str) -> Result<String, EasyOfficeError> {
        let iters = payload.trim().parse::<usize>().unwrap_or(5).clamp(3, 25);
        Ok(run_deterministic_benchmarks(iters).to_json())
    }

    fn handle_convert_document_format(payload: &str) -> Result<String, EasyOfficeError> {
        let (src_ext, dst_ext, title, body) = if let Some((first, rest)) = payload.split_once('|') {
            if let Some((s, d)) = first.split_once("->") {
                let (t, b) = rest.split_once('|').unwrap_or((rest, ""));
                (s.trim(), d.trim(), t.trim(), b.trim())
            } else if let Some((second, rest2)) = rest.split_once('|') {
                let (t, b) = rest2.split_once('|').unwrap_or((rest2, ""));
                (first.trim(), second.trim(), t.trim(), b.trim())
            } else {
                (first.trim(), rest.trim(), "변환_문서", "본문 텍스트")
            }
        } else if let Some((s, d)) = payload.split_once("->") {
            (s.trim(), d.trim(), "변환_문서", "본문 텍스트")
        } else {
            ("docx", "hwpx", "변환_문서", "본문 텍스트")
        };

        let title = if title.is_empty() {
            "변환_문서"
        } else {
            title
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

        let src_bytes = UniversalFormatConverter::encode(&model, src_fmt)?;
        let dst_bytes =
            UniversalFormatConverter::convert(&src_bytes, Some(src_fmt.extension()), dst_fmt)?;
        let (detected_dst, decoded) =
            UniversalFormatConverter::decode(&dst_bytes, Some(dst_fmt.extension()))?;
        let b64 = Self::encode_base64(&dst_bytes);

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
            dst_fmt.standard_specification(),
            decoded.paragraphs.len(),
            decoded.tables.len(),
            Self::escape_json_str(&decoded.title),
            b64,
            b64
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
            "home" | "홈" | "편집" => ("home", "홈"),
            "insert" | "삽입" | "입력" => ("insert", "삽입"),
            "layout" | "레이아웃" | "쪽·원고지" => ("layout", "레이아웃"),
            "formulas" | "formulasanddata" | "수식·데이터" | "productivity" | "생산성·보안"
            | "표·개체" => ("formulasAndData", "수식·데이터"),
            "module" | "modulespecific" | "모듈 전용" | "시트·차트" => {
                ("moduleSpecific", "모듈 전용")
            }
            "review" | "검토" | "보안·도구" => ("review", "검토"),
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

    pub fn escape_json_str(s: &str) -> String {
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
        if cleaned.len() % 2 != 0 {
            return Err(EasyOfficeError::InvalidArgument(
                "16진수 문자열 길이가 짝수가 아닙니다.".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(cleaned.len() / 2);
        for chunk in cleaned.chunks_exact(2) {
            let hi = Self::hex_val(chunk[0])?;
            let lo = Self::hex_val(chunk[1])?;
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

    fn encode_base64(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0];
            let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TABLE[(b2 & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
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
            "A1=항목;설명;B1=100;C1=foo\\;B1=999?B1",
        )
        .unwrap();
        assert!(eval_json.contains("\"value\":\"100\""));

        let wb_json = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            "Sheet1!A1=10;Sheet1!B1=20;Sheet1!C1=30?=SUM(A1:C1)",
        )
        .unwrap();
        assert!(wb_json.contains("\"value\":\"60\""));

        for cmd in ["evaluate_workbook_formula", "evaluate_sheet"] {
            let cross1 = EasyOfficeIpcRouter::dispatch(
                cmd,
                "Sheet1!A1=100;Sheet2!A1=250?Sheet1|=Sheet2!A1+50",
            )
            .unwrap();
            assert!(
                cross1.contains("\"value\":\"300\""),
                "cmd={cmd} res={cross1}"
            );

            let cross2 = EasyOfficeIpcRouter::dispatch(
                cmd,
                "Sheet1!A1=100;Sheet2!A1=250;Sheet2!B1=50?Sheet2|=SUM(A1:B1)",
            )
            .unwrap();
            assert!(
                cross2.contains("\"value\":\"300\""),
                "cmd={cmd} res={cross2}"
            );
        }

        let conv_json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "docx->xlsx|예산보고서|첫번째문단\n[[AGENCY]]\t행정안전부\n[[TABLE_NEW]]\n[[TABLE_ROW]]\t항목\t금액\n[[TABLE_ROW]]\t인건비\t500\n[[SHEET_CELL]]\t0\t0\t시트셀값\n[[SLIDE]]\t발표제목\t핵심1;;핵심2\t발표노트",
        )
        .unwrap();
        assert!(conv_json.contains("\"convertedBase64\":\""));
        assert!(conv_json.contains("\"tableCount\":1"));
        let b64_marker = "\"convertedBase64\":\"";
        let b64_start = conv_json.find(b64_marker).unwrap() + b64_marker.len();
        let b64_end = conv_json[b64_start..].find('"').unwrap() + b64_start;
        let b64_str = &conv_json[b64_start..b64_end];
        assert!(!b64_str.is_empty());

        let hwp_conv_json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "docx->hwp|공문서|본문문단\n[[AGENCY]]\t행정안전부\n[[TABLE_NEW]]\n[[TABLE_ROW]]\t구분\t비고\n[[TABLE_ROW]]\t승인\t정상",
        )
        .unwrap();
        assert!(hwp_conv_json.contains("\"tableCount\":1"));
        assert!(hwp_conv_json.contains("\"convertedBase64\":\""));

        let smart_json = EasyOfficeIpcRouter::dispatch(
            "run_smart_assistant",
            "첫 번째 핵심 문단입니다.\n[TODO] 보안 감사 보고서 제출\n두 번째 요약 문단입니다.",
        )
        .unwrap();
        assert!(smart_json.contains("\"summaryLines\":["));
        assert!(smart_json.contains("\"actionItems\":[\"[TODO] 보안 감사 보고서 제출\"]"));

        let cells_prefix =
            "A1=Hello (world;B1=He said \"hi;C1=Question\\?;D1=42;E1==IF(D1>10, \"x;y\", \"z\")";
        for (query, expected) in [
            ("A1", "\"value\":\"Hello (world\""),
            ("B1", "\"value\":\"He said \\\"hi\""),
            ("C1", "\"value\":\"Question?\""),
            ("D1", "\"value\":\"42\""),
            ("E1", "\"value\":\"x;y\""),
        ] {
            let res =
                EasyOfficeIpcRouter::dispatch("evaluate_sheet", &format!("{cells_prefix}?{query}"))
                    .unwrap();
            assert!(res.contains(expected), "query={query} res={res}");
        }

        let hangul_pii = DocumentSecurityAuditor::scan_and_scrub_pii(
            "담당자 이메일은security@easyoffice.local입니다.",
        );
        assert_eq!(hangul_pii.masked_emails, 1);
        assert_eq!(hangul_pii.scrubbed_text, "담당자 이메일은***@***입니다.");
    }
}

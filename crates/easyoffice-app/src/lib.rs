use easyoffice_formats::{
    EasyOfficeError, FormatDetector, StrictJsonValue, XmlSanitizer, ZipEntryDescriptor,
    ZipSecurityGuard, ZipSecurityPolicy,
};
use easyoffice_layout::{
    DesktopPlatform, DocumentPageLayout, OfficeModuleKind, PageOrientation, PdfStudioLayout,
    RibbonLayoutStyle, SignatureLavenderPalette, SlideDeckTheme, SlideStageLayout,
};
use easyoffice_sheet::{
    ConditionalFormatRule, DataValidationRule, EvaluatedCellValue, PackedCellCoordinate,
    WorkbookEngine,
};

pub mod benchmark;
mod convert_ipc;
pub mod ffi;
pub mod ribbon;
pub mod self_test;
pub mod smart_tools;

pub use benchmark::*;
pub use ffi::*;
pub use ribbon::*;
pub use self_test::run_self_test;
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

    fn json_sheet_name_or_default<'a>(raw: &'a str, fallback: &'a str) -> &'a str {
        if raw.trim().is_empty() {
            fallback
        } else {
            raw
        }
    }

    fn is_formula_expr(expr: &str) -> bool {
        let s = expr.trim_start();
        if s.starts_with('=') {
            return true;
        }
        if let Some((prefix, rest)) = s.split_once(':') {
            if prefix.eq_ignore_ascii_case("of")
                || prefix.eq_ignore_ascii_case("oooc")
                || prefix.eq_ignore_ascii_case("msoxl")
            {
                return rest.trim_start().starts_with('=');
            }
        }
        false
    }

    fn parse_rpc_json_object(
        payload: &str,
    ) -> Result<Vec<(String, StrictJsonValue)>, EasyOfficeError> {
        let clean = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let root = StrictJsonValue::parse(clean)?;
        match root {
            StrictJsonValue::Object(entries) => Ok(entries),
            _ => Err(EasyOfficeError::InvalidArgument(
                "RPC JSON 페이로드 루트는 객체여야 합니다.".to_string(),
            )),
        }
    }

    fn optional_json_str<'a>(
        entries: &'a [(String, StrictJsonValue)],
        key: &str,
    ) -> Result<Option<&'a str>, EasyOfficeError> {
        match StrictJsonValue::get_field(entries, key) {
            None | Some(StrictJsonValue::Null) => Ok(None),
            Some(StrictJsonValue::Str(s)) => Ok(Some(s.as_str())),
            Some(_) => Err(EasyOfficeError::InvalidArgument(format!(
                "JSON 필드 '{key}'는 문자열이어야 합니다."
            ))),
        }
    }

    fn resolve_default_sheet_from_entries(
        entries: &[(String, StrictJsonValue)],
    ) -> Result<String, EasyOfficeError> {
        if let Some(s) = Self::optional_json_str(entries, "sheet")? {
            if !s.trim().is_empty() {
                return Ok(s.to_string());
            }
        }
        if let Some(sheets_val) = StrictJsonValue::get_field(entries, "sheets") {
            let sheet_items = sheets_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'sheets' 필드는 문자열 배열이어야 합니다.".to_string(),
                )
            })?;
            let mut first_sheet: Option<String> = None;
            for s_item in sheet_items {
                let s_name = s_item.as_str().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'sheets' 항목은 문자열이어야 합니다.".to_string(),
                    )
                })?;
                if s_name.trim().is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "JSON 'sheets' 항목은 비어 있거나 공백만일 수 없습니다.".to_string(),
                    ));
                }
                if first_sheet.is_none() {
                    first_sheet = Some(s_name.to_string());
                }
            }
            return Ok(first_sheet.unwrap_or_default());
        }
        Ok("Sheet1".to_string())
    }

    fn build_workbook_from_json_entries(
        entries: &[(String, StrictJsonValue)],
        default_sheet: &str,
        seed_defaults: bool,
    ) -> Result<WorkbookEngine, EasyOfficeError> {
        let mut wb = WorkbookEngine::new();
        let explicit_sheets_val = StrictJsonValue::get_field(entries, "sheets");
        if let Some(sheets_val) = explicit_sheets_val {
            let sheet_items = sheets_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'sheets' 필드는 문자열 배열이어야 합니다.".to_string(),
                )
            })?;
            for s_item in sheet_items {
                let s_name = s_item.as_str().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'sheets' 항목은 문자열이어야 합니다.".to_string(),
                    )
                })?;
                if s_name.trim().is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "JSON 'sheets' 항목은 비어 있거나 공백만일 수 없습니다.".to_string(),
                    ));
                }
                wb.add_sheet(s_name);
            }
        } else {
            wb.add_sheet("Sheet1");
            wb.add_sheet("Sheet2");
            if !default_sheet.trim().is_empty() {
                wb.add_sheet(default_sheet);
            }
        }
        let mut seen_sheet1 = false;
        let mut seen_sheet2 = false;
        let mut cell_triples: Vec<(String, String, String)> = Vec::new();
        if let Some(cells_val) = StrictJsonValue::get_field(entries, "cells") {
            let cells = cells_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'cells' 필드는 배열이어야 합니다.".to_string(),
                )
            })?;
            for item in cells {
                let c_entries = item.as_object().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'cells' 항목은 객체여야 합니다.".to_string(),
                    )
                })?;
                let raw_sh = Self::optional_json_str(c_entries, "sheet")?.unwrap_or(default_sheet);
                let sh = Self::json_sheet_name_or_default(raw_sh, default_sheet);
                let addr = Self::optional_json_str(c_entries, "cell")?
                    .unwrap_or("")
                    .trim();
                if addr.is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "JSON 'cells' 항목에 비어 있지 않은 'cell' 주소가 필요합니다.".to_string(),
                    ));
                }
                let val = match StrictJsonValue::get_field(c_entries, "value") {
                    None => String::new(),
                    Some(v) => v.to_scalar_string().ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "JSON 셀 'value'는 문자열·숫자·불리언·null 스칼라여야 합니다."
                                .to_string(),
                        )
                    })?,
                };
                if WorkbookEngine::is_same_sheet_identity(sh, "Sheet1") {
                    seen_sheet1 = true;
                }
                if WorkbookEngine::is_same_sheet_identity(sh, "Sheet2") {
                    seen_sheet2 = true;
                }
                cell_triples.push((sh.to_string(), addr.to_string(), val));
            }
        }
        if seed_defaults {
            if !seen_sheet1 && wb.sheet("Sheet1").is_some() {
                wb.set_cell("Sheet1", "A1", "1200")?;
                wb.set_cell("Sheet1", "B1", "850")?;
            }
            if !seen_sheet2 && wb.sheet("Sheet2").is_some() {
                wb.set_cell("Sheet2", "A1", "100")?;
            }
        }
        let has_explicit_sheets = explicit_sheets_val.is_some();
        for (sh, addr, val) in cell_triples {
            if !has_explicit_sheets && !sh.trim().is_empty() {
                wb.add_sheet(&sh);
            }
            if wb.sheet(&sh).is_some() {
                wb.set_cell(&sh, &addr, val)?;
            }
        }
        Ok(wb)
    }

    fn handle_evaluate_sheet(payload: &str) -> Result<String, EasyOfficeError> {
        let rule = ConditionalFormatRule::GreaterThan(100.0);
        let dv_rule = DataValidationRule::NumberRange {
            min: 0.0,
            max: 1_000_000.0,
        };
        let entries = Self::parse_rpc_json_object(payload)?;
        let default_sh_owned = Self::resolve_default_sheet_from_entries(&entries)?;
        let default_sh = default_sh_owned.as_str();
        let wb = Self::build_workbook_from_json_entries(&entries, default_sh, false)?;
        if let Some(targets_val) = StrictJsonValue::get_field(&entries, "targets") {
            let targets = targets_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'targets' 필드는 배열이어야 합니다.".to_string(),
                )
            })?;
            if !targets.is_empty() {
                let mut first_expr = "A1".to_string();
                let mut first_eval = EvaluatedCellValue::Empty;
                let mut values_entries = Vec::with_capacity(targets.len());
                let mut target_values_entries = Vec::with_capacity(targets.len());
                for (idx, item) in targets.iter().enumerate() {
                    let t_entries = item.as_object().ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "JSON 'targets' 항목은 객체여야 합니다.".to_string(),
                        )
                    })?;
                    let sh = Self::json_sheet_name_or_default(
                        Self::optional_json_str(t_entries, "sheet")?.unwrap_or(default_sh),
                        default_sh,
                    );
                    let expr = Self::optional_json_str(t_entries, "cell")?
                        .unwrap_or("")
                        .trim();
                    if expr.is_empty() {
                        return Err(EasyOfficeError::InvalidArgument(
                            "JSON 'targets' 항목에 비어 있지 않은 'cell' 필드가 필요합니다."
                                .to_string(),
                        ));
                    }
                    let val = if Self::is_formula_expr(expr) {
                        wb.evaluate_formula_on_sheet(sh, expr)
                    } else {
                        wb.evaluate_cell(sh, expr)
                    };
                    let display = val.to_display_string();
                    let escaped_display = Self::escape_json_str(&display);
                    values_entries.push(format!(
                        "\"{}!{}\":\"{}\"",
                        Self::escape_json_str(sh),
                        Self::escape_json_str(expr),
                        escaped_display
                    ));
                    target_values_entries.push(format!("\"{escaped_display}\""));
                    if idx == 0 {
                        first_expr = expr.to_string();
                        first_eval = val;
                    }
                }
                let cf_match = rule.matches(&first_eval, &[]);
                let dv_valid = dv_rule.validate(&first_eval);
                return Ok(format!(
                    "{{\"query\":\"{}\",\"value\":\"{}\",\"values\":{{{}}},\"targetValues\":[{}],\"conditionalMatch\":{},\"validationPassed\":{}}}",
                    Self::escape_json_str(&first_expr),
                    Self::escape_json_str(&first_eval.to_display_string()),
                    values_entries.join(","),
                    target_values_entries.join(","),
                    cf_match,
                    dv_valid
                ));
            }
        }
        let anchor_coord = Self::resolve_optional_anchor_coord(&entries)?;
        let expr = match Self::optional_json_str(&entries, "expr")? {
            Some(s) => s,
            None => Self::optional_json_str(&entries, "query")?.unwrap_or("A1"),
        }
        .trim();
        let expr = if expr.is_empty() { "A1" } else { expr };
        let evaluated = if Self::is_formula_expr(expr) {
            wb.evaluate_formula_on_sheet_at(default_sh, anchor_coord, expr)
        } else {
            wb.evaluate_cell(default_sh, expr)
        };
        let cf_match = rule.matches(&evaluated, &[]);
        let dv_valid = dv_rule.validate(&evaluated);
        let display_val = evaluated.to_display_string();
        let escaped_display = Self::escape_json_str(&display_val);
        Ok(format!(
            "{{\"query\":\"{0}\",\"value\":\"{1}\",\"values\":{{\"{2}!{0}\":\"{1}\"}},\"targetValues\":[\"{1}\"],\"conditionalMatch\":{3},\"validationPassed\":{4}}}",
            Self::escape_json_str(expr),
            escaped_display,
            Self::escape_json_str(default_sh),
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
        let policy = ZipSecurityPolicy::default();
        let entries = Self::parse_rpc_json_object(payload)?;
        if let Some(xml_str) = Self::optional_json_str(&entries, "xml")? {
            if !xml_str.trim().is_empty() {
                XmlSanitizer::validate_safe_xml(xml_str)?;
            }
        }
        let mut parsed_entries: Vec<ZipEntryDescriptor> = Vec::new();
        if let Some(entries_val) = StrictJsonValue::get_field(&entries, "entries") {
            let items = entries_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'entries' 필드는 배열이어야 합니다.".to_string(),
                )
            })?;
            for item in items {
                let obj = item.as_object().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'entries' 항목은 객체여야 합니다.".to_string(),
                    )
                })?;
                let name = match Self::optional_json_str(obj, "name")? {
                    Some(n) => n,
                    None => Self::optional_json_str(obj, "path")?.unwrap_or(""),
                }
                .trim();
                if name.is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "ZIP 엔트리 이름('name')이 비어 있습니다.".to_string(),
                    ));
                }
                let compressed = StrictJsonValue::get_field(obj, "compressed")
                    .or_else(|| StrictJsonValue::get_field(obj, "compressedSize"))
                    .and_then(StrictJsonValue::as_u64)
                    .ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "유효하지 않은 compressed_size".to_string(),
                        )
                    })?;
                let uncompressed = StrictJsonValue::get_field(obj, "uncompressed")
                    .or_else(|| StrictJsonValue::get_field(obj, "uncompressedSize"))
                    .and_then(StrictJsonValue::as_u64)
                    .ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "유효하지 않은 uncompressed_size".to_string(),
                        )
                    })?;
                let encrypted = match StrictJsonValue::get_field(obj, "encrypted") {
                    None | Some(StrictJsonValue::Null) => false,
                    Some(StrictJsonValue::Bool(b)) => *b,
                    Some(_) => {
                        return Err(EasyOfficeError::InvalidArgument(
                            "유효하지 않은 encrypted 불리언 값".to_string(),
                        ))
                    }
                };
                parsed_entries.push(ZipEntryDescriptor::new(
                    name,
                    compressed,
                    uncompressed,
                    encrypted,
                ));
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
        let active_mod = match state.active_module {
            OfficeModuleKind::Document => "DOC",
            OfficeModuleKind::Hwp => "HWP",
            OfficeModuleKind::Sheet => "SHEET",
            OfficeModuleKind::Slide => "SLIDE",
            OfficeModuleKind::Pdf => "PDF",
        };
        let ribbon_style = match state.ribbon_style {
            RibbonLayoutStyle::HangulHwp => "hangul",
            RibbonLayoutStyle::StandardOffice => "standard",
        };
        format!(
            concat!(
                "{{",
                "\"commandId\":\"{}\",",
                "\"summary\":\"{}\",",
                "\"contextualBadge\":\"{}\",",
                "\"activeModule\":\"{}\",",
                "\"ribbonStyle\":\"{}\",",
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
            active_mod,
            ribbon_style,
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
        let spans_json = audit
            .spans
            .iter()
            .map(|sp| {
                format!(
                    "{{\"start\":{},\"end\":{},\"replacement\":\"{}\"}}",
                    sp.start_utf16,
                    sp.end_utf16,
                    Self::escape_json_str(&sp.replacement)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!(
            "{{\"scrubbedText\":\"{}\",\"maskedResidentIds\":{},\"maskedPhoneNumbers\":{},\"maskedEmails\":{},\"totalMasked\":{},\"spans\":[{}]}}",
            escaped,
            audit.masked_resident_ids,
            audit.masked_phone_numbers,
            audit.masked_emails,
            audit.total_masked_count(),
            spans_json
        ))
    }

    fn resolve_optional_anchor_coord(
        entries: &[(String, StrictJsonValue)],
    ) -> Result<Option<PackedCellCoordinate>, EasyOfficeError> {
        let Some(raw) = Self::optional_json_str(entries, "cell")? else {
            return Ok(None);
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        PackedCellCoordinate::parse_a1(trimmed)
            .map(Some)
            .map_err(|_| {
                EasyOfficeError::InvalidArgument(format!(
                    "유효하지 않은 셀 좌표('cell'): {trimmed}"
                ))
            })
    }

    fn handle_evaluate_workbook_formula(payload: &str) -> Result<String, EasyOfficeError> {
        let entries = Self::parse_rpc_json_object(payload)?;
        let active_sheet_owned = Self::resolve_default_sheet_from_entries(&entries)?;
        let active_sheet = active_sheet_owned.as_str();
        let anchor_coord = Self::resolve_optional_anchor_coord(&entries)?;
        let expr = Self::optional_json_str(&entries, "expr")?
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("=Sheet2!A1+50")
            .trim();
        let wb = Self::build_workbook_from_json_entries(&entries, active_sheet, true)?;
        let val = if Self::is_formula_expr(expr) {
            wb.evaluate_formula_on_sheet_at(active_sheet, anchor_coord, expr)
        } else {
            wb.evaluate_cell(active_sheet, expr)
        };
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

    pub(crate) fn encode_base64(bytes: &[u8]) -> String {
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

    pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
        easyoffice_formats::PdfSerializer::decode_base64_bytes(input)
    }
}

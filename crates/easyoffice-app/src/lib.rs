use crate::http_server::MAX_IPC_HTTP_BODY_BYTES;
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_layout::{
    DesktopPlatform, DocumentPageLayout, OfficeModuleKind, PageOrientation, PdfStudioLayout,
    RibbonLayoutStyle, SlideDeckTheme, SlideStageLayout,
};
use easyoffice_sheet::{PackedCellCoordinate, WorkbookEngine};

pub mod benchmark;
mod benchmark_baselines;
pub mod benchmark_large;
pub mod benchmark_real;
pub mod cli_args;
mod convert_hwp;
mod convert_ipc;
mod convert_open;
mod convert_password;
mod convert_structured;
mod convert_structured_doc;
mod convert_structured_encode;
#[cfg(test)]
mod editor_merge_tests;
mod eval_limit;
#[cfg(test)]
mod eval_limit_tests;
pub mod ffi;
pub mod http_server;
pub mod hwp_bundle;
mod page_setup_json;
#[cfg(test)]
mod perf_bench;
#[cfg(test)]
mod perf_bench_sheet;
mod pii_identifiers;
pub mod ribbon;
mod ribbon_session;
mod security_ipc;
pub mod self_test;
mod sheet_filter_ipc;
mod sheet_filter_json;
mod sheet_format_ipc;
#[cfg(test)]
mod sheet_ipc_tests;
mod sheet_layout_json;
mod sheet_look_json;
mod sheet_names_ipc;
#[cfg(test)]
mod sheet_names_ipc_tests;
mod sheet_names_json;
mod sheet_page_json;
mod sheet_pivot_ipc;
mod sheet_protect_ipc;
mod sheet_protect_json;
mod sheet_refs_ipc;
#[cfg(test)]
mod sheet_refs_ipc_tests;
#[cfg(test)]
mod sheet_rules_ipc_tests;
mod sheet_rules_json;
mod sheet_session;
mod sheet_sort_ipc;
mod sheet_structure_ipc;
#[cfg(test)]
mod sheet_structure_ipc_tests;
mod sheet_view_json;
mod sheet_view_scripts;
pub mod smart_tools;

pub use benchmark::*;
pub use convert_hwp::{RichHwpWriter, ShellWriters};
pub use convert_structured::RichPdfRenderer;
pub use ffi::*;
pub use http_server::{
    serve_http_on_listener, serve_http_on_listener_with_handler, serve_webui, ConnectionSlotGuard,
    MAX_CONCURRENT_HTTP_THREADS, MAX_CONNECTION_DURATION,
};
pub use ribbon::*;
pub use self_test::run_self_test;
pub use sheet_session::close_sheet_sessions_of;
pub use smart_tools::*;

/// IPC router dispatching the 15 workspace commands to the pure Rust engines. Shared by the Windows
/// desktop shell (`crates/easyoffice-desktop`, in-process `ipc_dispatch`) and the `--serve`
/// developer preview (`POST /api/ipc`).
pub struct EasyOfficeIpcRouter;

impl EasyOfficeIpcRouter {
    pub fn dispatch(command: &str, payload: &str) -> Result<String, EasyOfficeError> {
        Self::dispatch_inner(command, payload, "", ShellWriters::default())
    }

    /// Like [`Self::dispatch`], tagging state the command creates with `owner` (a window label) so
    /// it can be released when that window closes (see [`close_sheet_sessions_of`]).
    pub fn dispatch_for_owner(
        command: &str,
        payload: &str,
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        Self::dispatch_inner(command, payload, owner, ShellWriters::default())
    }

    /// [`Self::dispatch`] for a host that can draw pages: a rich document converted to PDF is
    /// handed to `renderer` as HWPX instead of being written by the engine's text-only PDF
    /// writer (PD-167). The engine PDF stays the fallback when the renderer fails.
    pub fn dispatch_with_pdf_renderer(
        command: &str,
        payload: &str,
        renderer: &RichPdfRenderer<'_>,
    ) -> Result<String, EasyOfficeError> {
        let writers = ShellWriters {
            pdf: Some(renderer),
            hwp: None,
        };
        Self::dispatch_inner(command, payload, "", writers)
    }

    /// [`Self::dispatch_for_owner`] with the writers of a host that has the Hangul layout core:
    /// the page renderer of [`Self::dispatch_with_pdf_renderer`] and the HWP 5.0 writer (PD-178),
    /// without which `.hwp` targets are refused.
    pub fn dispatch_for_owner_with_shell_writers(
        command: &str,
        payload: &str,
        owner: &str,
        writers: ShellWriters<'_>,
    ) -> Result<String, EasyOfficeError> {
        Self::dispatch_inner(command, payload, owner, writers)
    }

    fn dispatch_inner(
        command: &str,
        payload: &str,
        owner: &str,
        writers: ShellWriters<'_>,
    ) -> Result<String, EasyOfficeError> {
        // The same ceiling the HTTP server puts on a request body. The desktop shell calls this
        // directly, so the check lives here: no command decodes or parses what the server would
        // have refused.
        if payload.len() > MAX_IPC_HTTP_BODY_BYTES {
            return Err(EasyOfficeError::PayloadTooLarge {
                actual: payload.len(),
                max: MAX_IPC_HTTP_BODY_BYTES,
            });
        }
        match command {
            "get_workspace_bootstrap" => Self::handle_workspace_bootstrap(),
            "detect_format" => Self::handle_detect_format(payload),
            "evaluate_sheet" | "evaluate_sheet_formula" => {
                Self::handle_evaluate_sheet(payload, owner)
            }
            "evaluate_workbook_formula" => Self::handle_evaluate_workbook_formula(payload),
            "compute_layout" => Self::handle_compute_layout(payload),
            "audit_security" => Self::handle_audit_security(payload),
            "execute_ribbon_command" => Self::handle_execute_ribbon_command(payload),
            "undo_ribbon_command" => Self::handle_ribbon_history_step(payload, true),
            "redo_ribbon_command" => Self::handle_ribbon_history_step(payload, false),
            "switch_ribbon_style" => Self::handle_switch_ribbon_style(payload),
            "run_smart_assistant" => Self::handle_run_smart_assistant(payload),
            "scrub_pii" => Self::handle_scrub_pii(payload),
            "convert_document_format" => Self::handle_convert_document_format(payload, writers),
            "get_ribbon_category_commands" => Self::handle_get_ribbon_category_commands(payload),
            "run_engine_benchmark" => Self::handle_run_engine_benchmark(payload),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "알 수 없는 IPC 명령입니다: {other}"
            ))),
        }
    }

    fn handle_workspace_bootstrap() -> Result<String, EasyOfficeError> {
        let platform = DesktopPlatform::current();
        let json = format!(
            concat!(
                "{{",
                "\"platform\":\"{}\",",
                "\"shortcutModifier\":\"{}\",",
                "\"appVersion\":\"{}\",",
                "\"sheetSessions\":true,",
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
            env!("CARGO_PKG_VERSION"),
        );
        Ok(json)
    }

    fn json_sheet_name_or_default<'a>(raw: &'a str, fallback: &'a str) -> &'a str {
        if raw.trim().is_empty() {
            fallback
        } else {
            raw
        }
    }

    pub(crate) fn parse_rpc_json_object(
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

    pub(crate) fn optional_json_str<'a>(
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
        } else if !default_sheet.trim().is_empty() {
            wb.add_sheet(default_sheet);
        }
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
                cell_triples.push((sh.to_string(), addr.to_string(), val));
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
        let names = sheet_names_json::names_field(entries)?;
        if !names.is_empty() {
            wb.set_names_lenient(&names);
        }
        Ok(wb)
    }

    fn handle_evaluate_sheet(payload: &str, owner: &str) -> Result<String, EasyOfficeError> {
        let entries = Self::parse_rpc_json_object(payload)?;
        if let Some(action) = Self::optional_json_str(&entries, "sessionAction")? {
            return Self::handle_sheet_session_action(action, &entries, owner);
        }
        let default_sh_owned = Self::resolve_default_sheet_from_entries(&entries)?;
        let default_sh = default_sh_owned.as_str();
        let anchor_coord = Self::resolve_optional_anchor_coord(&entries)?;
        let wb = Self::build_workbook_from_json_entries(&entries, default_sh)?;
        if let Some(targets_val) = StrictJsonValue::get_field(&entries, "targets") {
            let targets = targets_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'targets' 필드는 배열이어야 합니다.".to_string(),
                )
            })?;
            if !targets.is_empty() {
                let mut requests: Vec<(&str, &str)> = Vec::with_capacity(targets.len());
                for item in targets {
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
                    requests.push((sh, expr));
                }
                let batch = eval_limit::evaluate(&wb, &requests, anchor_coord);
                return Ok(eval_limit::answer(&requests, &batch));
            }
        }
        let default_cell_label = anchor_coord
            .map(PackedCellCoordinate::to_a1)
            .unwrap_or_else(|| "A1".to_string());
        let expr_raw = match Self::optional_json_str(&entries, "expr")? {
            Some(s) => Some(s),
            None => Self::optional_json_str(&entries, "query")?,
        };
        let expr = match expr_raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => s,
            None => default_cell_label.as_str(),
        };
        let requests = [(default_sh, expr)];
        let batch = eval_limit::evaluate(&wb, &requests, anchor_coord);
        Ok(eval_limit::answer(&requests, &batch))
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

    fn handle_switch_ribbon_style(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let (raw_style, session_id) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
            let style = Self::optional_json_str(&entries, "style")?
                .unwrap_or("")
                .trim()
                .to_string();
            (style, ribbon_session::session_id_from_entries(&entries)?)
        } else {
            (
                trimmed.to_string(),
                ribbon_session::DEFAULT_RIBBON_SESSION_ID.to_string(),
            )
        };
        let style = match raw_style.to_ascii_lowercase().as_str() {
            "hangul" | "hwp" | "hangul_hwp" | "한글" => RibbonLayoutStyle::HangulHwp,
            "standard" | "standard_office" | "표준" => RibbonLayoutStyle::StandardOffice,
            other => {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "지원하지 않는 리본 레이아웃 모드입니다: {other}"
                )))
            }
        };
        ribbon_session::set_session_ribbon_style(&session_id, style);
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
        let trimmed = payload
            .strip_prefix('\u{FEFF}')
            .unwrap_or(payload)
            .trim_start();
        let text_owned: Option<String> = if trimmed.starts_with('{') {
            Self::parse_rpc_json_object(trimmed)
                .ok()
                .and_then(
                    |entries| match StrictJsonValue::get_field(&entries, "text") {
                        Some(StrictJsonValue::Str(s)) => Some(s.clone()),
                        _ => None,
                    },
                )
        } else {
            None
        };
        let doc_text = text_owned.as_deref().unwrap_or(payload);
        let paragraphs: Vec<String> = doc_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let summary = LocalSmartAssistant::summarize_document(&paragraphs, 2);
        let diag = LocalSmartAssistant::diagnose_writing(doc_text);
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
        let default_cell_label = anchor_coord
            .map(PackedCellCoordinate::to_a1)
            .unwrap_or_else(|| "A1".to_string());
        let expr = Self::optional_json_str(&entries, "expr")?
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(default_cell_label.as_str());
        let wb = Self::build_workbook_from_json_entries(&entries, active_sheet)?;
        let requests = [(active_sheet, expr)];
        let batch = eval_limit::evaluate(&wb, &requests, anchor_coord);
        let value = if batch.is_unreached(0) {
            "null".to_string()
        } else {
            format!(
                "\"{}\"",
                Self::escape_json_str(&batch.values[0].to_display_string())
            )
        };
        Ok(eval_limit::with_timed_out(
            format!(
                "{{\"sheet\":\"{}\",\"formula\":\"{}\",\"value\":{value},\"circular\":{}}}",
                Self::escape_json_str(active_sheet),
                Self::escape_json_str(expr),
                batch.circular
            ),
            batch.timed_out,
        ))
    }

    fn handle_run_engine_benchmark(payload: &str) -> Result<String, EasyOfficeError> {
        let iters = payload.trim().parse::<usize>().unwrap_or(5).clamp(3, 25);
        let mut report = run_deterministic_benchmarks(iters);
        report.real_paths = benchmark_real::run_real_path_benchmarks();
        report.all_equivalent &= report.real_paths.iter().all(|r| r.equivalence_verified);
        Ok(report.to_json())
    }

    fn handle_get_ribbon_category_commands(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let (mod_owned, cat_owned) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
            let m = Self::optional_json_str(&entries, "module")?
                .unwrap_or("DOC")
                .trim()
                .to_string();
            let c = Self::optional_json_str(&entries, "category")?
                .unwrap_or("home")
                .trim()
                .to_string();
            (m, c)
        } else {
            let (m, c) = payload.split_once('|').unwrap_or(("DOC", trimmed));
            (m.trim().to_string(), c.trim().to_string())
        };
        let module = match mod_owned.to_ascii_uppercase().as_str() {
            "HWP" | "HWPX" => OfficeModuleKind::Hwp,
            "SHEET" | "XLSX" | "XLS" => OfficeModuleKind::Sheet,
            "SLIDE" | "PPTX" | "PPT" => OfficeModuleKind::Slide,
            "PDF" => OfficeModuleKind::Pdf,
            _ => OfficeModuleKind::Document,
        };
        let (category_key, category_label) = match cat_owned.to_ascii_lowercase().as_str() {
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
            Self::escape_json_str(&cat_owned),
            items.join(",")
        ))
    }

    pub fn escape_json_str(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        Self::escape_json_into(&mut out, s);
        out
    }

    /// Appends `s` with the JSON string escapes applied.
    pub fn escape_json_into(out: &mut String, s: &str) {
        use std::fmt::Write;
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let bytes = s.as_bytes();
        let mut last = 0usize;
        let mut i = 0usize;
        while i + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            let d_quot = word ^ (0x22u64.wrapping_mul(LO));
            let d_bs = word ^ (0x5Cu64.wrapping_mul(LO));
            let m_quot = d_quot.wrapping_sub(LO) & !d_quot;
            let m_bs = d_bs.wrapping_sub(LO) & !d_bs;
            let m_ctrl = !(word | (word << 1) | (word << 2));
            let mask = (m_quot | m_bs | m_ctrl) & HI;
            if mask == 0 {
                i += 8;
                continue;
            }
            i += (mask.trailing_zeros() >> 3) as usize;
            let b = bytes[i];
            if last < i {
                out.push_str(&s[last..i]);
            }
            match b {
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                b'\n' => out.push_str("\\n"),
                b'\r' => out.push_str("\\r"),
                b'\t' => out.push_str("\\t"),
                _ => {
                    let _ = write!(out, "\\u{:04x}", b);
                }
            }
            i += 1;
            last = i;
        }
        while i < bytes.len() {
            let b = bytes[i];
            let escaped = match b {
                b'\\' => "\\\\",
                b'"' => "\\\"",
                b'\n' => "\\n",
                b'\r' => "\\r",
                b'\t' => "\\t",
                0x00..=0x1f => {
                    if last < i {
                        out.push_str(&s[last..i]);
                    }
                    let _ = write!(out, "\\u{:04x}", b);
                    i += 1;
                    last = i;
                    continue;
                }
                _ => {
                    i += 1;
                    continue;
                }
            };
            if last < i {
                out.push_str(&s[last..i]);
            }
            out.push_str(escaped);
            i += 1;
            last = i;
        }
        if last < s.len() {
            out.push_str(&s[last..]);
        }
    }

    pub(crate) fn decode_hex(hex: &str) -> Result<Vec<u8>, EasyOfficeError> {
        const HEX_LUT: [u8; 256] = {
            let mut lut = [0xFFu8; 256];
            let mut i = b'0';
            while i <= b'9' {
                lut[i as usize] = i - b'0';
                i += 1;
            }
            let mut i = b'a';
            while i <= b'f' {
                lut[i as usize] = i - b'a' + 10;
                i += 1;
            }
            let mut i = b'A';
            while i <= b'F' {
                lut[i as usize] = i - b'A' + 10;
                i += 1;
            }
            lut
        };

        let bytes = hex.as_bytes();
        if !bytes.iter().any(|b| b.is_ascii_whitespace()) {
            if bytes.len() % 2 != 0 {
                return Err(EasyOfficeError::InvalidArgument(
                    "16진수 문자열 길이가 짝수가 아닙니다.".to_string(),
                ));
            }
            let mut out = Vec::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let hi = HEX_LUT[chunk[0] as usize];
                let lo = HEX_LUT[chunk[1] as usize];
                if (hi | lo) == 0xFF {
                    return Err(EasyOfficeError::InvalidArgument(
                        "유효하지 않은 16진수 문자입니다.".to_string(),
                    ));
                }
                out.push((hi << 4) | lo);
            }
            return Ok(out);
        }

        let cleaned: Vec<u8> = bytes
            .iter()
            .copied()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        if cleaned.len() % 2 != 0 {
            return Err(EasyOfficeError::InvalidArgument(
                "16진수 문자열 길이가 짝수가 아닙니다.".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(cleaned.len() / 2);
        for chunk in cleaned.chunks_exact(2) {
            let hi = HEX_LUT[chunk[0] as usize];
            let lo = HEX_LUT[chunk[1] as usize];
            if (hi | lo) == 0xFF {
                return Err(EasyOfficeError::InvalidArgument(
                    "유효하지 않은 16진수 문자입니다.".to_string(),
                ));
            }
            out.push((hi << 4) | lo);
        }
        Ok(out)
    }

    pub fn encode_base64(bytes: &[u8]) -> String {
        easyoffice_formats::base64_codec::encode(bytes)
    }

    pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
        easyoffice_formats::PdfSerializer::decode_base64_bytes(input)
    }
}

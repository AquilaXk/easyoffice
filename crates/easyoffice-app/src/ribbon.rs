use easyoffice_formats::EasyOfficeError;
use easyoffice_layout::{ContextualObjectToolKind, OfficeModuleKind, RibbonLayoutStyle};

/// Single command entry in the 140-command (7 categories × 5 modules × 4 actions) context ribbon catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonCommandDescriptor {
    pub id: String,
    pub category_key: &'static str,
    pub category_index: usize,
    pub module: OfficeModuleKind,
    pub title: &'static str,
    pub shortcut: &'static str,
}

/// Bounded generic Undo/Redo state snapshot stack (max 64 snapshots).
#[derive(Debug, Clone)]
pub struct UndoRedoStack<T: Clone> {
    undo_stack: Vec<T>,
    redo_stack: Vec<T>,
    max_depth: usize,
}

impl<T: Clone> Default for UndoRedoStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> UndoRedoStack<T> {
    pub const MAX_SNAPSHOTS: usize = 64;

    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: Self::MAX_SNAPSHOTS,
        }
    }

    pub fn push_state(&mut self, snapshot: T) {
        if self.undo_stack.len() >= self.max_depth {
            let overflow = self.undo_stack.len() + 1 - self.max_depth;
            self.undo_stack.drain(0..overflow);
        }
        self.undo_stack.push(snapshot);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, current: T) -> Option<T> {
        let prev = self.undo_stack.pop()?;
        if self.redo_stack.len() >= self.max_depth {
            self.redo_stack.remove(0);
        }
        self.redo_stack.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.redo_stack.pop()?;
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }
}

/// Mutable workspace state mutated deterministically by the 140 ribbon commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonWorkspaceState {
    pub active_module: OfficeModuleKind,
    pub ribbon_style: RibbonLayoutStyle,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underline: bool,
    pub style_preset_index: usize,
    pub inserted_table_count: usize,
    pub inserted_chart_count: usize,
    pub inserted_shape_count: usize,
    pub inserted_link_count: usize,
    pub paper_size: String,
    pub column_count: usize,
    pub is_landscape: bool,
    pub margin_preset_index: usize,
    pub autosum_applied_count: usize,
    pub formula_wizard_open: bool,
    pub sort_ascending: bool,
    pub validation_guard_enabled: bool,
    pub toc_generated: bool,
    pub footnote_count: usize,
    pub mail_merge_applied_count: usize,
    pub table_formula_evaluated_count: usize,
    pub manuscript_grid_enabled: bool,
    pub approval_header_enabled: bool,
    pub click_here_field_count: usize,
    pub double_diagonal_border_enabled: bool,
    pub pivot_summary_enabled: bool,
    pub conditional_format_enabled: bool,
    pub freeze_panes_enabled: bool,
    pub master_layout_index: usize,
    pub snap_guide_enabled: bool,
    pub slide_transition_index: usize,
    pub presenter_mode_active: bool,
    pub pdf_highlight_count: usize,
    pub pdf_rotation_degrees: u32,
    pub pdf_split_merge_count: usize,
    pub pdf_converted_to_office_count: usize,
    pub track_changes_enabled: bool,
    pub review_comment_count: usize,
    pub security_verified_count: usize,
    pub stats_inspected_count: usize,
    pub split_view_enabled: bool,
    pub ribbon_collapsed: bool,
    pub dark_theme_enabled: bool,
    pub zoom_percent: u32,
    pub last_command_id: String,
    pub executed_count: usize,
}

impl Default for RibbonWorkspaceState {
    fn default() -> Self {
        Self {
            active_module: OfficeModuleKind::Document,
            ribbon_style: RibbonLayoutStyle::StandardOffice,
            is_bold: false,
            is_italic: false,
            is_underline: false,
            style_preset_index: 0,
            inserted_table_count: 0,
            inserted_chart_count: 0,
            inserted_shape_count: 0,
            inserted_link_count: 0,
            paper_size: "A4".to_string(),
            column_count: 1,
            is_landscape: false,
            margin_preset_index: 0,
            autosum_applied_count: 0,
            formula_wizard_open: false,
            sort_ascending: true,
            validation_guard_enabled: true,
            toc_generated: false,
            footnote_count: 0,
            mail_merge_applied_count: 0,
            table_formula_evaluated_count: 0,
            manuscript_grid_enabled: false,
            approval_header_enabled: true,
            click_here_field_count: 0,
            double_diagonal_border_enabled: false,
            pivot_summary_enabled: false,
            conditional_format_enabled: false,
            freeze_panes_enabled: false,
            master_layout_index: 0,
            snap_guide_enabled: true,
            slide_transition_index: 0,
            presenter_mode_active: false,
            pdf_highlight_count: 0,
            pdf_rotation_degrees: 0,
            pdf_split_merge_count: 0,
            pdf_converted_to_office_count: 0,
            track_changes_enabled: false,
            review_comment_count: 0,
            security_verified_count: 0,
            stats_inspected_count: 0,
            split_view_enabled: false,
            ribbon_collapsed: false,
            dark_theme_enabled: false,
            zoom_percent: 100,
            last_command_id: String::new(),
            executed_count: 0,
        }
    }
}

impl RibbonWorkspaceState {
    pub fn active_contextual_tool(&self) -> ContextualObjectToolKind {
        ContextualObjectToolKind::for_module(self.active_module)
    }
}

/// 140-command deterministic Ribbon router (7 categories × 20 commands = 140 commands).
pub struct RibbonCommandRouter;

impl RibbonCommandRouter {
    pub const CATEGORY_KEYS: [&'static str; 7] = [
        "home",
        "insert",
        "layout",
        "formulasAndData",
        "moduleSpecific",
        "review",
        "view",
    ];

    pub const MODULES: [OfficeModuleKind; 5] = [
        OfficeModuleKind::Document,
        OfficeModuleKind::Hwp,
        OfficeModuleKind::Sheet,
        OfficeModuleKind::Slide,
        OfficeModuleKind::Pdf,
    ];

    pub fn module_prefix(module: OfficeModuleKind) -> &'static str {
        match module {
            OfficeModuleKind::Document => "document",
            OfficeModuleKind::Hwp => "hwp",
            OfficeModuleKind::Sheet => "sheet",
            OfficeModuleKind::Slide => "slide",
            OfficeModuleKind::Pdf => "pdf",
        }
    }

    /// Returns the complete 140-command catalog (`7 categories × 5 modules × 4 commands = 140`).
    pub fn all_commands() -> Vec<RibbonCommandDescriptor> {
        let mut catalog = Vec::with_capacity(140);

        for (cat_idx, &cat_key) in Self::CATEGORY_KEYS.iter().enumerate() {
            for &module in &Self::MODULES {
                let prefix = Self::module_prefix(module);
                let specs: [(&str, &'static str, &'static str); 4] = match cat_key {
                    "home" => [
                        ("home-bold", "굵게", "Cmd+B"),
                        ("home-italic", "기울임", "Cmd+I"),
                        ("home-underline", "밑줄", "Cmd+U"),
                        ("home-style", "스타일 적용", "Alt+Cmd+S"),
                    ],
                    "insert" => [
                        ("insert-table", "표 삽입", "Ctrl+Cmd+T"),
                        ("insert-chart", "차트 개체", "Ctrl+Cmd+C"),
                        ("insert-shape", "도형·다이어그램", "Ctrl+Cmd+S"),
                        ("insert-link", "책갈피·링크", "Cmd+K"),
                    ],
                    "layout" => [
                        ("layout-paper", "용지 규격 (A4/Letter)", "Shift+Cmd+P"),
                        ("layout-orientation", "용지 방향 전환", "Alt+Cmd+O"),
                        ("layout-columns", "다단 설정", "Alt+Cmd+M"),
                        ("layout-margins", "여백 프리셋", "Alt+Cmd+G"),
                    ],
                    "formulasAndData" => [
                        ("data-autosum", "자동 합계 (SUM)", "Shift+Cmd+T"),
                        ("data-fn-browser", "함수 마법사", "Shift+F3"),
                        ("data-sort", "오름차순·내림차순 정렬", "Alt+Cmd+R"),
                        ("data-validation", "데이터 유효성 검사", "Alt+Cmd+V"),
                    ],
                    "moduleSpecific" => match module {
                        OfficeModuleKind::Document => [
                            ("mod-toc", "자동 목차 생성", "Ctrl+Alt+T"),
                            ("mod-footnote", "각주·미주 삽입", "Alt+Cmd+F"),
                            ("mod-mailmerge", "메일 머지 치환", "Ctrl+Cmd+M"),
                            ("mod-tableformula", "표 셀 수식 계산", "Ctrl+Cmd+="),
                        ],
                        OfficeModuleKind::Hwp => [
                            ("mod-header", "행정 공문서 두문·결재란", "Ctrl+Alt+H"),
                            ("mod-field", "누름틀 필드 삽입", "Ctrl+Cmd+E"),
                            ("mod-border", "이중선·대각선 테두리", "Ctrl+Cmd+B"),
                            ("mod-manuscript", "200자 원고지 매수 검산", "Ctrl+Cmd+W"),
                        ],
                        OfficeModuleKind::Sheet => [
                            ("mod-pivot", "피벗 테이블 요약", "Alt+Cmd+P"),
                            ("mod-chart", "차트 시각화", "Alt+F1"),
                            ("mod-condfmt", "조건부 서식 규칙", "Ctrl+Cmd+F"),
                            ("mod-freeze", "행·열 틀 고정", "Ctrl+Cmd+L"),
                        ],
                        OfficeModuleKind::Slide => [
                            ("mod-master", "마스터 레이아웃", "Shift+Cmd+M"),
                            ("mod-snap", "스마트 정렬 가이드", "Cmd+;"),
                            ("mod-transition", "화면 전환 효과", "Alt+Cmd+T"),
                            ("mod-presenter", "발표자 도구 시작", "Alt+Cmd+P"),
                        ],
                        OfficeModuleKind::Pdf => [
                            ("mod-highlight", "형광펜·밑줄 주석", "Ctrl+Cmd+H"),
                            ("mod-rotate", "페이지 90° 회전", "Cmd+R"),
                            ("mod-splitmerge", "페이지 분할·병합", "Shift+Cmd+S"),
                            ("mod-tooffice", "편집 문서 변환", "Shift+Cmd+E"),
                        ],
                    },
                    "review" => [
                        ("review-track", "변경 내용 추적", "Shift+Cmd+E"),
                        ("review-comment", "검토 메모 스레드", "Alt+Cmd+C"),
                        ("review-verify", "문서 구조 보안 검사", "Ctrl+Cmd+V"),
                        ("review-stats", "문서 통계·글자 수", "Shift+Cmd+I"),
                    ],
                    "view" => [
                        ("view-split", "좌우 스플릿 뷰", "Ctrl+Cmd+2"),
                        ("view-ribbon", "리본 접기·펼치기", "Alt+Cmd+R"),
                        ("view-theme", "라이트·다크 테마 전환", "Ctrl+Alt+D"),
                        ("view-zoom", "100% 실제 배율", "Cmd+0"),
                    ],
                    _ => unreachable!(),
                };

                for (suffix, title, shortcut) in specs {
                    let id = if cat_key == "moduleSpecific" {
                        let short_mod = match module {
                            OfficeModuleKind::Document => "doc",
                            OfficeModuleKind::Hwp => "hwp",
                            OfficeModuleKind::Sheet => "sheet",
                            OfficeModuleKind::Slide => "slide",
                            OfficeModuleKind::Pdf => "pdf",
                        };
                        format!("{short_mod}-{suffix}")
                    } else {
                        format!("{prefix}-{suffix}")
                    };
                    catalog.push(RibbonCommandDescriptor {
                        id,
                        category_key: cat_key,
                        category_index: cat_idx,
                        module,
                        title,
                        shortcut,
                    });
                }
            }
        }
        catalog
    }

    /// Executes any of the 140 catalog commands deterministically against `RibbonWorkspaceState`.
    pub fn execute_command(
        command_id: &str,
        state: &mut RibbonWorkspaceState,
    ) -> Result<String, EasyOfficeError> {
        let trimmed = command_id.trim();
        let catalog = Self::all_commands();
        let matched = catalog.iter().find(|c| c.id == trimmed).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("등록되지 않은 리본 명령 ID입니다: {trimmed}"))
        })?;

        state.active_module = matched.module;
        state.last_command_id = matched.id.clone();
        state.executed_count += 1;

        if trimmed.ends_with("-home-bold") {
            state.is_bold = !state.is_bold;
        } else if trimmed.ends_with("-home-italic") {
            state.is_italic = !state.is_italic;
        } else if trimmed.ends_with("-home-underline") {
            state.is_underline = !state.is_underline;
        } else if trimmed.ends_with("-home-style") {
            state.style_preset_index = (state.style_preset_index + 1) % 4;
        } else if trimmed.ends_with("-insert-table") {
            state.inserted_table_count += 1;
        } else if trimmed.ends_with("-insert-chart") || trimmed == "sheet-mod-chart" {
            state.inserted_chart_count += 1;
        } else if trimmed.ends_with("-insert-shape") {
            state.inserted_shape_count += 1;
        } else if trimmed.ends_with("-insert-link") {
            state.inserted_link_count += 1;
        } else if trimmed.ends_with("-layout-paper") {
            state.paper_size = if state.paper_size == "A4" {
                "Letter".to_string()
            } else {
                "A4".to_string()
            };
        } else if trimmed.ends_with("-layout-orientation") {
            state.is_landscape = !state.is_landscape;
        } else if trimmed.ends_with("-layout-columns") {
            state.column_count = if state.column_count >= 3 {
                1
            } else {
                state.column_count + 1
            };
        } else if trimmed.ends_with("-layout-margins") {
            state.margin_preset_index = (state.margin_preset_index + 1) % 3;
        } else if trimmed.ends_with("-data-autosum") {
            state.autosum_applied_count += 1;
        } else if trimmed.ends_with("-data-fn-browser") {
            state.formula_wizard_open = !state.formula_wizard_open;
        } else if trimmed.ends_with("-data-sort") {
            state.sort_ascending = !state.sort_ascending;
        } else if trimmed.ends_with("-data-validation") {
            state.validation_guard_enabled = !state.validation_guard_enabled;
        } else if trimmed == "doc-mod-toc" {
            state.toc_generated = !state.toc_generated;
        } else if trimmed == "doc-mod-footnote" {
            state.footnote_count += 1;
        } else if trimmed == "doc-mod-mailmerge" {
            state.mail_merge_applied_count += 1;
        } else if trimmed == "doc-mod-tableformula" {
            state.table_formula_evaluated_count += 1;
        } else if trimmed == "hwp-mod-header" {
            state.approval_header_enabled = !state.approval_header_enabled;
        } else if trimmed == "hwp-mod-field" {
            state.click_here_field_count += 1;
        } else if trimmed == "hwp-mod-border" {
            state.double_diagonal_border_enabled = !state.double_diagonal_border_enabled;
        } else if trimmed == "hwp-mod-manuscript" {
            state.manuscript_grid_enabled = !state.manuscript_grid_enabled;
        } else if trimmed == "sheet-mod-pivot" {
            state.pivot_summary_enabled = !state.pivot_summary_enabled;
        } else if trimmed == "sheet-mod-condfmt" {
            state.conditional_format_enabled = !state.conditional_format_enabled;
        } else if trimmed == "sheet-mod-freeze" {
            state.freeze_panes_enabled = !state.freeze_panes_enabled;
        } else if trimmed == "slide-mod-master" {
            state.master_layout_index = (state.master_layout_index + 1) % 3;
        } else if trimmed == "slide-mod-snap" {
            state.snap_guide_enabled = !state.snap_guide_enabled;
        } else if trimmed == "slide-mod-transition" {
            state.slide_transition_index = (state.slide_transition_index + 1) % 4;
        } else if trimmed == "slide-mod-presenter" {
            state.presenter_mode_active = !state.presenter_mode_active;
        } else if trimmed == "pdf-mod-highlight" {
            state.pdf_highlight_count += 1;
        } else if trimmed == "pdf-mod-rotate" {
            state.pdf_rotation_degrees = (state.pdf_rotation_degrees + 90) % 360;
        } else if trimmed == "pdf-mod-splitmerge" {
            state.pdf_split_merge_count += 1;
        } else if trimmed == "pdf-mod-tooffice" {
            state.pdf_converted_to_office_count += 1;
        } else if trimmed.ends_with("-review-track") {
            state.track_changes_enabled = !state.track_changes_enabled;
        } else if trimmed.ends_with("-review-comment") {
            state.review_comment_count += 1;
        } else if trimmed.ends_with("-review-verify") {
            state.security_verified_count += 1;
        } else if trimmed.ends_with("-review-stats") {
            state.stats_inspected_count += 1;
        } else if trimmed.ends_with("-view-split") {
            state.split_view_enabled = !state.split_view_enabled;
        } else if trimmed.ends_with("-view-ribbon") {
            state.ribbon_collapsed = !state.ribbon_collapsed;
        } else if trimmed.ends_with("-view-theme") {
            state.dark_theme_enabled = !state.dark_theme_enabled;
        } else if trimmed.ends_with("-view-zoom") {
            state.zoom_percent = match state.zoom_percent {
                100 => 125,
                125 => 150,
                150 => 90,
                _ => 100,
            };
        }

        let cat_title = state.ribbon_style.category_titles()[matched.category_index];
        let badge = state.active_contextual_tool().badge_title();
        Ok(format!(
            "[{}] {} ({}) 실행 완료 — 컨텍스트: {}",
            cat_title, matched.title, matched.id, badge
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_140_ribbon_commands_and_bounded_undo_redo_stack() {
        let catalog = RibbonCommandRouter::all_commands();
        assert_eq!(catalog.len(), 140);

        let mut state = RibbonWorkspaceState::default();
        let mut undo_stack = UndoRedoStack::new();

        for cmd in &catalog {
            let mut probe_before = state.clone();
            undo_stack.push_state(state.clone());
            let summary = RibbonCommandRouter::execute_command(&cmd.id, &mut state).unwrap();
            assert!(summary.contains(cmd.title));
            probe_before.active_module = state.active_module;
            probe_before.last_command_id = state.last_command_id.clone();
            probe_before.executed_count = state.executed_count;
            assert_ne!(
                probe_before, state,
                "Command {} did not mutate domain state",
                cmd.id
            );
        }

        assert_eq!(state.executed_count, 140);
        assert_eq!(
            undo_stack.undo_len(),
            UndoRedoStack::<RibbonWorkspaceState>::MAX_SNAPSHOTS
        );

        let before_undo = state.clone();
        let restored = undo_stack.undo(before_undo).unwrap();
        assert_eq!(restored.executed_count, 139);
        assert!(undo_stack.can_redo());

        let redone = undo_stack.redo(restored).unwrap();
        assert_eq!(redone.executed_count, 140);
    }
}

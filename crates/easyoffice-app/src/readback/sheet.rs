//! Structured spreadsheet workbook and A1 range readback + formula/validation checks (PD-248).

use std::collections::HashMap;
use std::fmt::Write;

use easyoffice_formats::{
    effective_image_two_cell, resolve_code, run_total, CellBox, CellValueKind, EasyOfficeError,
    OdsUnpacked, SheetState, SheetStyling, MAX_WORKBOOK_NOTES, MAX_WORKBOOK_SHEET_IMAGES,
    MAX_WORKBOOK_SHEET_IMAGE_BYTES,
};
use easyoffice_sheet::{
    compile_for_page, general_display, validate_input, Budget, CompiledNumFmt, EvaluatedCellValue,
    FormulaError, MemoTable, PackedCellCoordinate, RecalcError, RecalcOutcome, RecalcSession,
    SpillArea, Verdict, WorkbookEngine,
};

use crate::eval_limit::save_eval_time;
use crate::sheet_session::LIMITS;

use super::{push_json_str, push_opt_json_str, same_sheet_name, ReadbackQuery};

const MAX_VALIDATION_CHECKS: usize = 10_000;

const ERROR_BUCKETS: [&str; 8] = [
    "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#N/A", "#NUM!", "#SPILL!", "#CALC!",
];

#[derive(Default, Clone, Copy)]
struct ErrorHistogram {
    counts: [usize; 8],
}

impl ErrorHistogram {
    fn record(&mut self, err: FormulaError) {
        let idx = match err {
            FormulaError::DivByZero => 0,
            FormulaError::Value => 1,
            FormulaError::Ref => 2,
            FormulaError::Name => 3,
            FormulaError::Na => 4,
            FormulaError::Num | FormulaError::Cycle => 5,
            FormulaError::Spill => 6,
            FormulaError::Calc => 7,
        };
        self.counts[idx] += 1;
    }

    fn total(&self) -> usize {
        self.counts.iter().sum()
    }

    fn write_json(&self, out: &mut String) {
        let _ = write!(out, "{{\"total\":{},\"byCode\":{{", self.total());
        for (i, code) in ERROR_BUCKETS.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "\"{code}\":{}", self.counts[i]);
        }
        out.push_str("}}");
    }
}

struct ErrorEntry {
    sheet: String,
    cell: String,
    formula: String,
    error: &'static str,
}

struct DvViolation {
    sheet: String,
    cell: String,
    value: String,
    alert: &'static str,
}

/// Memoizes compiled number formats by `num_fmt` slot (`O(1)` lookup per cell).
struct NumFmtMemo<'a> {
    styling: Option<&'a SheetStyling>,
    slots: Vec<Option<(String, CompiledNumFmt)>>,
}

impl<'a> NumFmtMemo<'a> {
    fn new(styling: Option<&'a SheetStyling>) -> Self {
        Self {
            styling,
            slots: Vec::new(),
        }
    }

    fn entry_at(&mut self, row: u32, col: u32) -> Option<(&str, &CompiledNumFmt)> {
        let st = self.styling?;
        let id = st.cells.get(&(row, col))?.num_fmt;
        if id == 0 {
            return None;
        }
        let idx = usize::from(id);
        if self.slots.len() <= idx {
            self.slots.resize_with(idx + 1, || None);
        }
        if self.slots[idx].is_none() {
            if let Some(code) = resolve_code(id, &st.num_fmts) {
                let compiled = compile_for_page(code);
                self.slots[idx] = Some((code.to_owned(), compiled));
            }
        }
        self.slots[idx]
            .as_ref()
            .map(|(code, fmt)| (code.as_str(), fmt))
    }
}

pub(super) fn append_styling_warnings(
    styling: &[(String, SheetStyling)],
    warnings: &mut Vec<String>,
) {
    for (name, st) in styling {
        warnings.extend(
            st.rules
                .unsupported
                .iter()
                .map(|r| format!("{name} 시트의 {r} 규칙은 지원하지 않아 가져오지 않았습니다.")),
        );
        if st.names_rejected > 0 {
            warnings.push(format!(
                "이름 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                st.names_rejected
            ));
        }
        if st.links_rejected > 0 {
            warnings.push(format!(
                "{name} 시트의 하이퍼링크 {}개는 허용하지 않는 주소이거나 한도를 넘어 가져오지 않았습니다.",
                st.links_rejected
            ));
        }
        if st.tables_rejected > 0 {
            warnings.push(format!(
                "{name} 시트의 표 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                st.tables_rejected
            ));
        }
        if st.merges_dropped > 0 {
            warnings.push(format!(
                "{name} 시트의 병합 {}개는 표와 겹쳐 풀었습니다.",
                st.merges_dropped
            ));
        }
        if st.num_fmts_rejected > 0 {
            warnings.push(format!(
                "서식 코드 {}개는 한도를 넘거나 알 수 없어 일반 서식으로 가져왔습니다.",
                st.num_fmts_rejected
            ));
        }
        if st.fonts_rejected > 0 {
            warnings.push(format!(
                "글꼴 이름 {}개는 한도를 넘거나 올바르지 않아 기본 글꼴로 가져왔습니다.",
                st.fonts_rejected
            ));
        }
        if st.charts_rejected > 0 {
            warnings.push(format!(
                "{name} 시트의 차트 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                st.charts_rejected
            ));
        }
        if st.pivots_rejected > 0 {
            warnings.push(format!(
                "{name} 시트의 피벗 테이블 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                st.pivots_rejected
            ));
        }
        if st.images_rejected > 0 {
            warnings.push(format!(
                "{name} 시트의 그림 {}개는 지원하지 않는 형식이거나 손상되어 가져오지 않았습니다.",
                st.images_rejected
            ));
        }
        if st.notes_rejected > 0 {
            warnings.push(format!(
                "{name} 시트의 메모 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                st.notes_rejected
            ));
        }
    }
}

fn styling_of<'a>(stylings: &'a [(String, SheetStyling)], name: &str) -> Option<&'a SheetStyling> {
    stylings
        .iter()
        .find(|(n, _)| same_sheet_name(n, name))
        .map(|(_, s)| s)
}

pub(super) fn write_workbook_readback(
    out: &mut String,
    sheets: &[OdsUnpacked],
    stylings: &[(String, SheetStyling)],
    query: &ReadbackQuery,
) -> Result<(), EasyOfficeError> {
    let (mut wb_images, mut wb_image_bytes, mut wb_notes) = (0usize, 0usize, 0usize);
    for (_, st) in stylings {
        wb_images = wb_images.saturating_add(st.images.len());
        wb_notes = wb_notes.saturating_add(st.notes.len());
        if wb_images > MAX_WORKBOOK_SHEET_IMAGES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 전체 그림 수({wb_images})가 허용 상한({MAX_WORKBOOK_SHEET_IMAGES})을 초과했습니다."
            )));
        }
        if wb_notes > MAX_WORKBOOK_NOTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 전체 메모 수({wb_notes})가 허용 상한({MAX_WORKBOOK_NOTES})을 초과했습니다."
            )));
        }
        for img in &st.images {
            wb_image_bytes = wb_image_bytes.saturating_add(img.image.bytes().len());
            if wb_image_bytes > MAX_WORKBOOK_SHEET_IMAGE_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "통합 문서 그림 총 크기({wb_image_bytes}바이트)가 허용 상한({MAX_WORKBOOK_SHEET_IMAGE_BYTES}바이트)을 초과했습니다."
                )));
            }
        }
    }
    let mut wb = WorkbookEngine::new();
    for (name, _) in sheets {
        wb.try_add_sheet(name)?;
    }
    for (name, _) in sheets {
        if let Some(st) = styling_of(stylings, name) {
            if !st.tables.is_empty() {
                wb.set_tables(name, st.tables.clone())?;
            }
        }
    }
    for (name, cells) in sheets {
        let st = styling_of(stylings, name);
        let Some(engine) = wb.sheet_mut(name) else {
            continue;
        };
        for &(row, col, ref raw) in cells {
            let Ok(coord) = PackedCellCoordinate::new(row, col) else {
                continue;
            };
            let explicit = st.and_then(|s| s.explicit_cell_kind(row, col, raw));
            engine.set_cell_typed_coord(coord, raw.clone(), explicit);
        }
        if let Some(st) = st {
            if !st.merges.is_empty() {
                engine.set_spill_merges(st.merges.clone());
            }
            let (hidden, filtered) = st.hidden_row_runs();
            if !hidden.is_empty() || !filtered.is_empty() {
                engine.set_hidden_rows(hidden, filtered);
            }
        }
    }
    if let Some((_, first_st)) = stylings.first() {
        if !first_st.defined_names.is_empty() {
            wb.set_names_lenient(&first_st.defined_names);
        }
    }
    let budget = Budget {
        max_edges: LIMITS.max_session_edges,
        max_graph_bytes: LIMITS.max_session_bytes,
        max_bytes: LIMITS.max_session_bytes,
        max_time: save_eval_time(),
    };
    let mut session = RecalcSession::with_budget(wb, budget).map_err(|err| match err {
        RecalcError::Invalid(e) => e,
        RecalcError::OverBudget => EasyOfficeError::ResourceBudgetExceeded(
            "시트 의존 그래프가 허용 크기를 넘었습니다.".to_string(),
        ),
    })?;
    let outcome = session.recalculate();
    write_session_readback(out, &mut session, &outcome, stylings, query)
}

fn is_formula_cell(engine: &easyoffice_sheet::SheetEngine, coord: PackedCellCoordinate) -> bool {
    engine.explicit_cell_kind(coord) != Some(CellValueKind::Text)
        && engine
            .raw_cells()
            .get(&coord)
            .is_some_and(|raw| raw.trim_start().starts_with('='))
}

fn kind_tag(kind: CellValueKind) -> &'static str {
    match kind {
        CellValueKind::Number => "n",
        CellValueKind::Text => "s",
        CellValueKind::Boolean => "b",
        CellValueKind::Error => "e",
        CellValueKind::Formula => "f",
    }
}

fn format_evaluated_display(
    val: &EvaluatedCellValue,
    coord: PackedCellCoordinate,
    memo: &mut NumFmtMemo<'_>,
) -> (Option<String>, String) {
    let raw_display = val.to_display_string();
    let eval_kind = match val {
        EvaluatedCellValue::Number(_) => CellValueKind::Number,
        EvaluatedCellValue::Boolean(_) => CellValueKind::Boolean,
        EvaluatedCellValue::Error(_) => CellValueKind::Error,
        EvaluatedCellValue::Text(_) | EvaluatedCellValue::Empty => CellValueKind::Text,
    };
    if let Some((code, compiled)) = memo.entry_at(coord.row(), coord.col()) {
        let code_owned = code.to_string();
        if let Some(formatted) = compiled.format_typed(&raw_display, eval_kind) {
            return (Some(code_owned), formatted.flat());
        }
        return (Some(code_owned), raw_display);
    }
    if eval_kind == CellValueKind::Number {
        return (None, general_display(&raw_display).into_owned());
    }
    (None, raw_display)
}

fn write_evaluated_json_value(out: &mut String, val: &EvaluatedCellValue) -> &'static str {
    match val {
        EvaluatedCellValue::Empty => {
            out.push_str("null");
            "empty"
        }
        EvaluatedCellValue::Number(n) if n.is_finite() => {
            let _ = write!(out, "{}", general_display(&n.to_string()));
            "number"
        }
        EvaluatedCellValue::Number(_) => {
            out.push_str("null");
            "number"
        }
        EvaluatedCellValue::Text(s) => {
            push_json_str(out, s);
            "text"
        }
        EvaluatedCellValue::Boolean(b) => {
            out.push_str(if *b { "true" } else { "false" });
            "boolean"
        }
        EvaluatedCellValue::Error(e) => {
            push_json_str(out, e.code());
            "error"
        }
    }
}

pub(super) fn write_session_readback(
    out: &mut String,
    session: &mut RecalcSession,
    outcome: &RecalcOutcome,
    stylings: &[(String, SheetStyling)],
    query: &ReadbackQuery,
) -> Result<(), EasyOfficeError> {
    let sheet_names: Vec<String> = session
        .workbook()
        .sheet_names()
        .into_iter()
        .map(ToString::to_string)
        .collect();
    let target_idx = if let Some(wanted) = &query.sheet {
        Some(
            sheet_names
                .iter()
                .position(|n| same_sheet_name(n, wanted))
                .ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {wanted}"))
                })?,
        )
    } else {
        (!sheet_names.is_empty()).then_some(0)
    };

    let spills: Vec<SpillArea> = session.spill_areas();
    let mut blocked_spill_count = 0usize;
    let mut spills_per_sheet: HashMap<String, usize> = HashMap::new();
    for sp in &spills {
        *spills_per_sheet.entry(sp.sheet.clone()).or_insert(0) += 1;
        if sp.blocked {
            blocked_spill_count += 1;
        }
    }

    let mut total_formula_cells = 0usize;
    let mut total_error_cells = 0usize;
    let mut all_errors: Vec<ErrorEntry> = Vec::new();
    let mut dv_violations: Vec<DvViolation> = Vec::new();
    let mut target_values: Option<HashMap<PackedCellCoordinate, EvaluatedCellValue>> = None;
    let mut target_used_box: Option<CellBox> = None;

    out.push_str("\"sheets\":[");
    for (idx, sheet_name) in sheet_names.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let values = session.sheet_values(sheet_name);
        let st = styling_of(stylings, sheet_name);
        let mut hist = ErrorHistogram::default();
        let mut formula_count = 0usize;
        let mut used: Option<CellBox> = None;

        {
            let engine = session.workbook().sheet(sheet_name).expect("sheet exists");
            let mut coords: Vec<PackedCellCoordinate> =
                engine.raw_cells().keys().copied().collect();
            coords.sort_unstable();
            for coord in &coords {
                let b = CellBox::cell(coord.row(), coord.col());
                used = Some(match used {
                    None => b,
                    Some(prev) => CellBox {
                        row0: prev.row0.min(b.row0),
                        col0: prev.col0.min(b.col0),
                        row1: prev.row1.max(b.row1),
                        col1: prev.col1.max(b.col1),
                    },
                });
                if is_formula_cell(engine, *coord) {
                    formula_count += 1;
                }
            }
            for sp in spills
                .iter()
                .filter(|s| s.sheet == *sheet_name && !s.blocked)
            {
                let end_r = sp.anchor.row() + (sp.rows.saturating_sub(1)) as u32;
                let end_c = sp.anchor.col() + (sp.cols.saturating_sub(1)) as u32;
                used = Some(match used {
                    None => CellBox {
                        row0: sp.anchor.row(),
                        col0: sp.anchor.col(),
                        row1: end_r,
                        col1: end_c,
                    },
                    Some(prev) => CellBox {
                        row0: prev.row0.min(sp.anchor.row()),
                        col0: prev.col0.min(sp.anchor.col()),
                        row1: prev.row1.max(end_r),
                        col1: prev.col1.max(end_c),
                    },
                });
            }
            let mut val_coords: Vec<PackedCellCoordinate> = values.keys().copied().collect();
            val_coords.sort_unstable();
            for coord in val_coords {
                if let Some(EvaluatedCellValue::Error(err)) = values.get(&coord) {
                    hist.record(*err);
                    if all_errors.len() < query.max_items {
                        let formula = engine.raw_cells().get(&coord).cloned().unwrap_or_default();
                        all_errors.push(ErrorEntry {
                            sheet: sheet_name.clone(),
                            cell: coord.to_a1(),
                            formula,
                            error: err.code(),
                        });
                    }
                }
            }
        }

        if let Some(st_rules) = st.filter(|s| !s.rules.dv.is_empty()) {
            let mut wb_clone = session.workbook().clone();
            let mut memo = MemoTable::new((idx as u16).saturating_add(1));
            let coords: Vec<(PackedCellCoordinate, String)> = {
                let engine = wb_clone.sheet(sheet_name).expect("sheet exists");
                let mut pairs: Vec<(PackedCellCoordinate, String)> = engine
                    .raw_cells()
                    .iter()
                    .filter(|&(&c, raw)| {
                        engine.explicit_cell_kind(c) == Some(CellValueKind::Text)
                            || !raw.trim_start().starts_with('=')
                    })
                    .map(|(&c, raw)| (c, raw.clone()))
                    .collect();
                pairs.sort_unstable_by_key(|&(c, _)| c);
                pairs
            };
            for (coord, raw) in coords.into_iter().take(MAX_VALIDATION_CHECKS) {
                let (verdict, next_memo) =
                    validate_input(&mut wb_clone, idx, &st_rules.rules.dv, coord, &raw, memo);
                memo = next_memo;
                if let Verdict::Invalid { alert, .. } = verdict {
                    if dv_violations.len() < query.max_items {
                        dv_violations.push(DvViolation {
                            sheet: sheet_name.clone(),
                            cell: coord.to_a1(),
                            value: raw,
                            alert: alert.name(),
                        });
                    }
                }
            }
        }

        total_formula_cells += formula_count;
        total_error_cells += hist.total();
        let engine = session.workbook().sheet(sheet_name).expect("sheet exists");
        let populated_count = engine.raw_cells().len();
        let engine_tables = session.workbook().tables_of(sheet_name);
        write_sheet_summary(
            out,
            sheet_name,
            populated_count,
            formula_count,
            used,
            st,
            &engine_tables,
            spills_per_sheet.get(sheet_name).copied().unwrap_or(0),
            &hist,
        );

        if target_idx == Some(idx) {
            target_used_box = used;
            target_values = Some(values);
        }
    }
    out.push_str("],\"rangeReadback\":");

    if let Some(t_idx) = target_idx {
        let t_name = &sheet_names[t_idx];
        let t_st = styling_of(stylings, t_name);
        let effective_box = query.range.or(target_used_box);
        let vals = target_values.unwrap_or_default();
        let t_engine = session.workbook().sheet(t_name).expect("sheet exists");
        write_range_readback(
            out,
            t_name,
            t_engine,
            t_st,
            effective_box,
            &vals,
            &spills,
            query.max_items,
        );
    } else {
        out.push_str("null");
    }

    let defined_names_count = session.workbook().names().len();
    let (
        mut rej_names,
        mut rej_links,
        mut rej_tables,
        mut rej_merges,
        mut rej_fmts,
        mut rej_fonts,
        mut rej_charts,
        mut rej_pivots,
        mut rej_images,
        mut rej_notes,
    ) = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    for (_, st) in stylings {
        rej_names = rej_names.saturating_add(st.names_rejected);
        rej_links = rej_links.saturating_add(st.links_rejected);
        rej_tables = rej_tables.saturating_add(st.tables_rejected);
        rej_merges = rej_merges.saturating_add(st.merges_dropped);
        rej_fmts = rej_fmts.saturating_add(st.num_fmts_rejected);
        rej_fonts = rej_fonts.saturating_add(st.fonts_rejected);
        rej_charts = rej_charts.saturating_add(st.charts_rejected);
        rej_pivots = rej_pivots.saturating_add(st.pivots_rejected);
        rej_images = rej_images.saturating_add(st.images_rejected);
        rej_notes = rej_notes.saturating_add(st.notes_rejected);
    }

    let _ = write!(
        out,
        ",\"checks\":{{\"circular\":{},\"timedOut\":{},\"totalFormulaCells\":{total_formula_cells},\"totalErrorCells\":{total_error_cells},\"blockedSpillCount\":{blocked_spill_count},\"definedNamesCount\":{defined_names_count},\"errors\":[",
        outcome.circular, outcome.timed_out
    );
    for (i, e) in all_errors.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"sheet\":");
        push_json_str(out, &e.sheet);
        let _ = write!(out, ",\"cell\":\"{}\",\"formula\":", e.cell);
        push_json_str(out, &e.formula);
        let _ = write!(out, ",\"error\":\"{}\"}}", e.error);
    }
    out.push_str("],\"validationViolations\":[");
    for (i, v) in dv_violations.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"sheet\":");
        push_json_str(out, &v.sheet);
        let _ = write!(out, ",\"cell\":\"{}\",\"value\":", v.cell);
        push_json_str(out, &v.value);
        let _ = write!(out, ",\"alert\":\"{}\"}}", v.alert);
    }
    let _ = write!(
        out,
        "],\"rejectedCounts\":{{\"names\":{rej_names},\"links\":{rej_links},\"tables\":{rej_tables},\"merges\":{rej_merges},\"numFmts\":{rej_fmts},\"fonts\":{rej_fonts},\"charts\":{rej_charts},\"pivots\":{rej_pivots},\"images\":{rej_images},\"notes\":{rej_notes}}}}}"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_sheet_summary(
    out: &mut String,
    name: &str,
    populated_cells: usize,
    formula_cells: usize,
    used: Option<CellBox>,
    st: Option<&SheetStyling>,
    engine_tables: &[easyoffice_formats::SheetTable],
    spill_count: usize,
    hist: &ErrorHistogram,
) {
    let vis = match st.map_or(SheetState::Visible, SheetState::of) {
        SheetState::Visible => "visible",
        SheetState::Hidden => "hidden",
        SheetState::VeryHidden => "veryHidden",
    };
    out.push_str("{\"name\":");
    push_json_str(out, name);
    let _ = write!(
        out,
        ",\"visibility\":\"{vis}\",\"populatedCells\":{populated_cells},\"formulaCells\":{formula_cells},\"usedRange\":"
    );
    match used {
        Some(b) => push_json_str(out, &b.to_a1()),
        None => out.push_str("null"),
    }
    let (max_r, max_c) = used.map_or((0, 0), |b| (b.row1 + 1, b.col1 + 1));
    let merge_count = st.map_or(0, |s| s.merges.len());
    let hidden_rows = st.map_or(0, |s| run_total(&s.hidden_rows));
    let hidden_cols = st.map_or(0, |s| run_total(&s.hidden_cols));
    let protected = st.is_some_and(|s| s.protect.is_some());
    let dv_count = st.map_or(0, |s| s.rules.dv.len());
    let cf_count = st.map_or(0, |s| s.rules.cf.len());
    let link_count = st.map_or(0, |s| s.links.len());
    let chart_count = st.map_or(0, |s| s.charts.len());
    let pivot_count = st.map_or(0, |s| s.pivots.len());
    let _ = write!(
        out,
        ",\"rows\":{max_r},\"cols\":{max_c},\"mergeCount\":{merge_count},\"hiddenRowCount\":{hidden_rows},\"hiddenColCount\":{hidden_cols},\"protected\":{protected},\"validationRuleCount\":{dv_count},\"conditionalRuleCount\":{cf_count},\"linkCount\":{link_count},\"spillCount\":{spill_count},\"chartCount\":{chart_count},\"charts\":["
    );
    if let Some(charts) = st.map(|s| s.charts.as_slice()) {
        for (c_idx, ch) in charts.iter().enumerate() {
            if c_idx > 0 {
                out.push(',');
            }
            crate::sheet_chart_json::write_chart_json(out, ch, true);
        }
    }
    let _ = write!(out, "],\"pivotCount\":{pivot_count},\"pivots\":[");
    if let Some(pivots) = st.map(|s| s.pivots.as_slice()) {
        for (p_idx, pv) in pivots.iter().enumerate() {
            if p_idx > 0 {
                out.push(',');
            }
            crate::sheet_pivot_json::write_pivot_json(out, pv);
        }
    }
    let image_count = st.map_or(0, |s| s.images.len());
    let _ = write!(out, "],\"imageCount\":{image_count},\"images\":[");
    if let Some(styling) = st {
        for (i_idx, im) in styling.images.iter().enumerate() {
            if i_idx > 0 {
                out.push(',');
            }
            let (fc, fr, _, _, tc, tr, _, _) =
                effective_image_two_cell(&im.anchor, &styling.columns);
            out.push_str("{\"id\":");
            push_json_str(out, &im.id);
            out.push_str(",\"name\":");
            push_json_str(out, &im.name);
            out.push_str(",\"alt\":");
            push_json_str(out, &im.alt);
            out.push_str(",\"mime\":");
            push_json_str(out, im.image.format().mime());
            let _ = write!(out, ",\"bytes\":{}", im.image.bytes().len());
            out.push_str(",\"fromCell\":\"");
            if let Ok(coord) = PackedCellCoordinate::new(fr, fc) {
                coord.write_a1(out);
            }
            out.push_str("\",\"toCell\":\"");
            if let Ok(coord) = PackedCellCoordinate::new(tr, tc) {
                coord.write_a1(out);
            }
            out.push_str("\"}");
        }
    }
    let note_count = st.map_or(0, |s| s.notes.len());
    let _ = write!(out, "],\"noteCount\":{note_count},\"notes\":");
    if let Some(notes) = st.map(|s| &s.notes) {
        crate::sheet_media_notes_json::write_notes_json(out, notes);
    } else {
        out.push_str("[]");
    }
    out.push_str(",\"tables\":[");

    let tables_from_st = st.map(|s| s.tables.as_slice()).unwrap_or(&[]);
    let tables = if !tables_from_st.is_empty() {
        tables_from_st
    } else {
        engine_tables
    };
    for (t_idx, t) in tables.iter().enumerate() {
        if t_idx > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        push_json_str(out, &t.name);
        let calc_cols = t.calculated_columns.iter().filter(|c| c.is_some()).count();
        let _ = write!(
            out,
            ",\"range\":\"{}\",\"headerRow\":{},\"totalsRow\":{},\"calculatedColumnCount\":{calc_cols},\"style\":",
            t.range.to_a1(),
            t.header,
            t.totals
        );
        push_opt_json_str(out, t.style.as_deref());
        out.push_str(",\"columns\":[");
        for (c_idx, col_name) in t.columns.iter().enumerate() {
            if c_idx > 0 {
                out.push(',');
            }
            push_json_str(out, col_name);
        }
        out.push_str("]}");
    }

    out.push_str("],\"slicers\":[");
    let mut slicer_idx = 0usize;
    for t in tables {
        for sl in &t.slicers {
            if slicer_idx > 0 {
                out.push(',');
            }
            slicer_idx += 1;
            let col_name = t
                .columns
                .get(sl.col as usize)
                .map(String::as_str)
                .unwrap_or("");
            out.push_str("{\"tableName\":");
            push_json_str(out, &t.name);
            out.push_str(",\"columnName\":");
            push_json_str(out, col_name);
            let _ = write!(
                out,
                ",\"col\":{},\"multiSelect\":{},\"caption\":",
                sl.col, sl.multi_select
            );
            push_json_str(out, &sl.caption);
            out.push('}');
        }
    }

    out.push_str("],\"filter\":");
    match st.and_then(|s| s.filter.as_ref()) {
        Some(f) => {
            let _ = write!(
                out,
                "{{\"range\":\"{}\",\"activeColumnCount\":{},\"hiddenRowCount\":{}}}",
                f.range.to_a1(),
                f.columns.len(),
                run_total(&f.hidden)
            );
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"errorCounts\":");
    hist.write_json(out);
    out.push('}');
}

#[allow(clippy::too_many_arguments)]
fn write_range_readback(
    out: &mut String,
    sheet_name: &str,
    engine: &easyoffice_sheet::SheetEngine,
    st: Option<&SheetStyling>,
    target_box: Option<CellBox>,
    values: &HashMap<PackedCellCoordinate, EvaluatedCellValue>,
    spills: &[SpillArea],
    max_items: usize,
) {
    out.push_str("{\"sheet\":");
    push_json_str(out, sheet_name);
    let Some(b) = target_box else {
        out.push_str(
            ",\"range\":null,\"totalInRange\":0,\"returnedCount\":0,\"truncated\":false,\"cells\":[]}",
        );
        return;
    };
    let mut spill_owners: HashMap<PackedCellCoordinate, PackedCellCoordinate> = HashMap::new();
    for sp in spills
        .iter()
        .filter(|s| s.sheet == sheet_name && !s.blocked)
    {
        for r in 0..sp.rows {
            for c in 0..sp.cols {
                if r == 0 && c == 0 {
                    continue;
                }
                if let Ok(at) = PackedCellCoordinate::new(
                    sp.anchor.row() + r as u32,
                    sp.anchor.col() + c as u32,
                ) {
                    if b.contains(at.row(), at.col()) {
                        spill_owners.insert(at, sp.anchor);
                    }
                }
            }
        }
    }

    let mut coords: Vec<PackedCellCoordinate> = values
        .keys()
        .copied()
        .filter(|c| b.contains(c.row(), c.col()))
        .collect();
    for &sp_coord in spill_owners.keys() {
        coords.push(sp_coord);
    }
    coords.sort_unstable();
    coords.dedup();

    let total_in_range = coords.len();
    let truncated = total_in_range > max_items;
    let kept = coords.len().min(max_items);
    let _ = write!(
        out,
        ",\"range\":\"{}\",\"totalInRange\":{total_in_range},\"returnedCount\":{kept},\"truncated\":{truncated},\"cells\":[",
        b.to_a1()
    );

    let mut fmt_memo = NumFmtMemo::new(st);
    for (idx, &coord) in coords.iter().take(kept).enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let raw = engine
            .raw_cells()
            .get(&coord)
            .map(String::as_str)
            .unwrap_or("");
        let formula = is_formula_cell(engine, coord).then_some(raw);
        let val = values
            .get(&coord)
            .cloned()
            .unwrap_or(EvaluatedCellValue::Empty);
        let (num_fmt, display) = format_evaluated_display(&val, coord, &mut fmt_memo);
        let explicit = engine.explicit_cell_kind(coord).map(kind_tag);
        let err_code = match &val {
            EvaluatedCellValue::Error(e) => Some(e.code()),
            _ => None,
        };
        let link = st
            .and_then(|s| s.links.get(&(coord.row(), coord.col())))
            .map(|l| l.target.as_str());
        let spill_anchor = spill_owners.get(&coord).map(|a| a.to_a1());

        let _ = write!(
            out,
            "{{\"cell\":\"{}\",\"row\":{},\"col\":{},\"raw\":",
            coord.to_a1(),
            coord.row(),
            coord.col()
        );
        push_json_str(out, raw);
        out.push_str(",\"formula\":");
        push_opt_json_str(out, formula);
        out.push_str(",\"value\":");
        let v_type = write_evaluated_json_value(out, &val);
        let _ = write!(out, ",\"valueType\":\"{v_type}\",\"display\":");
        push_json_str(out, &display);
        out.push_str(",\"explicitKind\":");
        push_opt_json_str(out, explicit);
        out.push_str(",\"numFmt\":");
        push_opt_json_str(out, num_fmt.as_deref());
        out.push_str(",\"error\":");
        push_opt_json_str(out, err_code);
        out.push_str(",\"link\":");
        push_opt_json_str(out, link);
        out.push_str(",\"spillAnchor\":");
        push_opt_json_str(out, spill_anchor.as_deref());
        out.push('}');
    }
    out.push_str("]}");
}

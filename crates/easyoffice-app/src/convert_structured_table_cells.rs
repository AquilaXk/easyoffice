//! JSON encoding and strict Fail-Closed parsing for rich `cellParagraphs` inside DOC tables.

use std::fmt::Write;

use easyoffice_formats::{
    EasyOfficeError, RichAlignment, RichCellParagraph, RichTable, TableCellParagraphs,
    MAX_CELL_PARAGRAPH_ENTRIES, MAX_PARAGRAPHS_PER_CELL, MAX_RUNS_PER_CELL_PARA,
};

use super::super::{
    as_array, as_object, field, invalid, opt_bounded, parse_alignment, parse_runs, Budget,
    JsonObject, MAX_LINE_SPACING_PERCENT,
};

pub(crate) fn write_cell_paragraphs_json(out: &mut String, table: &RichTable) {
    let entries = table.validated_cell_paragraphs();
    if entries.is_empty() {
        return;
    }
    out.push_str(",\"cellParagraphs\":[");
    for (idx, entry) in entries.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"row\":{},\"col\":{},\"paragraphs\":[",
            entry.row, entry.col
        );
        for (p_idx, p) in entry.paragraphs.iter().enumerate() {
            if p_idx > 0 {
                out.push(',');
            }
            out.push_str("{\"runs\":[");
            crate::convert_open::write_runs_json(out, &p.runs);
            out.push(']');
            if let Some(a) = p.alignment {
                let token = match a {
                    RichAlignment::Left => "left",
                    RichAlignment::Center => "center",
                    RichAlignment::Right => "right",
                    RichAlignment::Justify => "justify",
                };
                let _ = write!(out, ",\"align\":\"{token}\"");
            }
            if let Some(v) = p.line_spacing_percent {
                let _ = write!(out, ",\"lineSpacingPercent\":{v}");
            }
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push(']');
}

pub(crate) fn parse_cell_paragraphs(
    obj: &JsonObject,
    rows: &[Vec<String>],
    formula_cells: &[(u32, u32)],
    budget: &mut Budget,
    warnings: &mut Vec<String>,
) -> Result<Vec<TableCellParagraphs>, EasyOfficeError> {
    let Some(v) = field(obj, "cellParagraphs") else {
        return Ok(Vec::new());
    };
    let arr = as_array(v, "cellParagraphs")?;
    if arr.len() > MAX_CELL_PARAGRAPH_ENTRIES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "표 셀 문단 서식 수가 한도({MAX_CELL_PARAGRAPH_ENTRIES})를 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    let mut dropped = 0usize;
    for item in arr {
        budget.take(1)?;
        let item_obj = as_object(item, "cellParagraphs[]")?;
        let row = super::req_u32(item_obj, "row", "셀 서식의")?;
        let col = super::req_u32(item_obj, "col", "셀 서식의")?;
        let para_arr = as_array(
            field(item_obj, "paragraphs")
                .ok_or_else(|| invalid("구조화 저장: 셀 서식에는 'paragraphs'가 필요합니다."))?,
            "paragraphs",
        )?;
        if para_arr.len() > MAX_PARAGRAPHS_PER_CELL {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "표 셀 내 문단 수가 한도({MAX_PARAGRAPHS_PER_CELL})를 초과했습니다."
            )));
        }
        let mut paragraphs = Vec::with_capacity(para_arr.len());
        for p_val in para_arr {
            budget.take(1)?;
            let p_obj = as_object(p_val, "paragraphs[]")?;
            let runs = parse_runs(p_obj, budget)?;
            if runs.len() > MAX_RUNS_PER_CELL_PARA {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "표 셀 내 런 수가 한도({MAX_RUNS_PER_CELL_PARA})를 초과했습니다."
                )));
            }
            paragraphs.push(RichCellParagraph {
                runs,
                alignment: parse_alignment(p_obj)?,
                line_spacing_percent: opt_bounded(
                    p_obj,
                    "lineSpacingPercent",
                    1,
                    MAX_LINE_SPACING_PERCENT,
                )?
                .map(|v| v as u16),
            });
        }
        let in_bounds = rows
            .get(row as usize)
            .is_some_and(|r| (col as usize) < r.len());
        if in_bounds && !paragraphs.is_empty() && !formula_cells.contains(&(row, col)) {
            out.push(TableCellParagraphs {
                row,
                col,
                paragraphs,
            });
        } else {
            dropped += 1;
        }
    }
    if dropped > 0 {
        warnings.push(format!(
            "표의 셀 문단 서식 {dropped}곳은 범위를 벗어나거나 수식 셀이어서 저장하지 않았습니다."
        ));
    }
    Ok(out)
}

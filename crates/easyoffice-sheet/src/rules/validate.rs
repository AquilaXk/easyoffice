//! Data validation: does an entered value satisfy the rule of its cell?
//!
//! Values are judged the way the sheet would store them: a number when the text is numeric, a
//! boolean for `TRUE`/`FALSE`, text otherwise. A formula entry is not judged (the rule applies to
//! typed values, as in the file formats' reference behaviour), and clearing a cell is always
//! allowed.

use super::data::{Cells, SheetCells};
use crate::coercion::{is_valid_decimal_syntax, parse_iso_date_serial, parse_strict_number};
use crate::{EvalContext, EvaluatedCellValue, MemoTable, PackedCellCoordinate, WorkbookEngine};
use easyoffice_formats::{CellBox, Cmp, DataValidation, DvAlert, DvKind};

/// Outcome of [`validate_input`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Valid,
    Invalid {
        alert: DvAlert,
        title: String,
        message: String,
        /// The rule asks for the alert (`showErrorMessage`); when false the entry passes silently.
        show: bool,
    },
}

fn typed_value(input: &str) -> EvaluatedCellValue {
    let t = input.trim();
    if is_valid_decimal_syntax(t) {
        if let Some(n) = parse_strict_number(t) {
            return EvaluatedCellValue::Number(n);
        }
    }
    if t.eq_ignore_ascii_case("TRUE") {
        return EvaluatedCellValue::Boolean(true);
    }
    if t.eq_ignore_ascii_case("FALSE") {
        return EvaluatedCellValue::Boolean(false);
    }
    EvaluatedCellValue::Text(input.to_string())
}

/// Seconds in a day fraction from `hh:mm[:ss]`.
fn parse_time_fraction(s: &str) -> Option<f64> {
    let mut parts = s.trim().split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next().map_or(Some(0.0), |p| p.parse().ok())?;
    (parts.next().is_none()
        && (0.0..24.0).contains(&h)
        && (0.0..60.0).contains(&m)
        && (0.0..60.0).contains(&sec))
    .then(|| (h * 3600.0 + m * 60.0 + sec) / 86_400.0)
}

fn operand_number(text: &str, kind: DvKind, cells: &mut dyn Cells) -> Option<f64> {
    let expr = text.trim().trim_start_matches('=');
    if let Ok(n) = expr.parse::<f64>() {
        return Some(n);
    }
    if kind == DvKind::Date {
        if let Some(n) = parse_iso_date_serial(expr.trim_matches('"')) {
            return Some(n);
        }
    }
    if kind == DvKind::Time {
        if let Some(n) = parse_time_fraction(expr.trim_matches('"')) {
            return Some(n);
        }
    }
    cells.eval_formula(expr).as_number()
}

fn compare(cmp: Cmp, v: f64, a: f64, b: f64) -> bool {
    match cmp {
        Cmp::Between => v >= a.min(b) && v <= a.max(b),
        Cmp::NotBetween => v < a.min(b) || v > a.max(b),
        Cmp::Equal => v == a,
        Cmp::NotEqual => v != a,
        Cmp::Greater => v > a,
        Cmp::GreaterEqual => v >= a,
        Cmp::Less => v < a,
        Cmp::LessEqual => v <= a,
    }
}

/// Items a list rule offers: the literal items, or the cells of a range reference.
pub(crate) fn list_items(dv: &DataValidation, cells: &mut dyn Cells) -> Vec<String> {
    match dv.a.strip_prefix('=') {
        None => {
            dv.a.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        Some(reference) => {
            let target = reference.rsplit_once('!').map_or(reference, |(_, r)| r);
            let Some(b) = CellBox::parse(target) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            let area = b.area().min(super::data::BLANK_SCAN_CELLS);
            for i in 0..area {
                let width = u64::from(b.col1 - b.col0 + 1);
                let (r, c) = (b.row0 + (i / width) as u32, b.col0 + (i % width) as u32);
                let v = cells.value(PackedCellCoordinate((u64::from(r) << 32) | u64::from(c)));
                if v != EvaluatedCellValue::Empty {
                    out.push(v.to_display_string());
                }
            }
            out
        }
    }
}

fn judge(dv: &DataValidation, cells: &mut dyn Cells, input: &str) -> bool {
    let value = typed_value(input);
    let text_len = input.chars().count() as f64;
    match dv.kind {
        DvKind::Whole | DvKind::Decimal => {
            let EvaluatedCellValue::Number(n) = value else {
                return false;
            };
            if dv.kind == DvKind::Whole && n.fract() != 0.0 {
                return false;
            }
            let (Some(a), b) = (
                operand_number(&dv.a, dv.kind, cells),
                operand_number(&dv.b, dv.kind, cells).unwrap_or(0.0),
            ) else {
                return false;
            };
            compare(dv.cmp, n, a, b)
        }
        DvKind::Date | DvKind::Time => {
            let n = match &value {
                EvaluatedCellValue::Number(n) => Some(*n),
                EvaluatedCellValue::Text(t) if dv.kind == DvKind::Date => {
                    parse_iso_date_serial(t.trim())
                }
                EvaluatedCellValue::Text(t) => parse_time_fraction(t),
                _ => None,
            };
            let Some(n) = n else {
                return false;
            };
            let (Some(a), b) = (
                operand_number(&dv.a, dv.kind, cells),
                operand_number(&dv.b, dv.kind, cells).unwrap_or(0.0),
            ) else {
                return false;
            };
            compare(dv.cmp, n, a, b)
        }
        DvKind::TextLength => {
            let (Some(a), b) = (
                operand_number(&dv.a, dv.kind, cells),
                operand_number(&dv.b, dv.kind, cells).unwrap_or(0.0),
            ) else {
                return false;
            };
            compare(dv.cmp, text_len, a, b)
        }
        DvKind::List => {
            let items = list_items(dv, cells);
            let literal = !dv.a.starts_with('=');
            items.iter().any(|item| {
                if literal {
                    item == input.trim()
                } else {
                    item.eq_ignore_ascii_case(input.trim()) || *item == input.trim()
                }
            })
        }
        DvKind::Checkbox => {
            let (on, off) = dv.checkbox_values();
            let entered = input.trim();
            [on, off].iter().any(|v| v.eq_ignore_ascii_case(entered))
        }
        DvKind::ListMultiple => {
            // Every chosen item (comma separated) must be an item of the list.
            let items = list_items(dv, cells);
            input
                .split(',')
                .map(str::trim)
                .filter(|chosen| !chosen.is_empty())
                .all(|chosen| items.iter().any(|item| item == chosen))
        }
        DvKind::Custom => {
            let r = cells.eval_formula(dv.a.trim().trim_start_matches('='));
            match r {
                EvaluatedCellValue::Boolean(b) => b,
                EvaluatedCellValue::Number(n) => n != 0.0,
                _ => false,
            }
        }
    }
}

/// Judges `input` for the cell `coord` of sheet `sheet` against `rules`. The first rule that
/// covers the cell applies. A custom formula sees `input` in the cell, so the cell is set for the
/// duration of the check and restored afterwards; `memo` is the formula memo of the caller.
pub fn validate_input(
    wb: &mut WorkbookEngine,
    sheet: usize,
    rules: &[DataValidation],
    coord: PackedCellCoordinate,
    input: &str,
    memo: MemoTable,
) -> (Verdict, MemoTable) {
    let Some(dv) = rules.iter().find(|dv| {
        dv.ranges
            .iter()
            .any(|b| b.contains(coord.row(), coord.col()))
    }) else {
        return (Verdict::Valid, memo);
    };
    if input.is_empty() || input.trim_start().starts_with('=') {
        return (Verdict::Valid, memo);
    }
    let previous = wb.sheets()[sheet].cells.get(&coord).cloned();
    if dv.kind == DvKind::Custom {
        wb.sheets_mut()[sheet].set_cell_coord(coord, input);
    }
    let sid = wb.sheets()[sheet].resolved_sheet_id();
    let (ok, memo) = {
        let mut ctx = EvalContext::with_workbook(wb, sid);
        ctx.memo = memo;
        let ok = {
            let mut cells = SheetCells {
                sheet: &wb.sheets()[sheet],
                ctx: &mut ctx,
            };
            judge(dv, &mut cells, input)
        };
        (ok, std::mem::replace(&mut ctx.memo, MemoTable::new(sid)))
    };
    if dv.kind == DvKind::Custom {
        wb.sheets_mut()[sheet].set_cell_coord(coord, previous.unwrap_or_default());
    }
    let verdict = if ok {
        Verdict::Valid
    } else {
        Verdict::Invalid {
            alert: dv.alert,
            title: dv.error_title.clone(),
            message: dv.error.clone(),
            show: dv.show_error,
        }
    };
    (verdict, memo)
}

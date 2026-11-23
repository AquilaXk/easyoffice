//! `BAHTTEXT`, `EUROCONVERT` and `PHONETIC`: text and amounts for a locale.
//!
//! 참조 구현: Univer `packages/engine-formula/src/functions/text/bahttext/index.ts` (`calculate`,
//! `_handleSingleObject`, `_convertNumberToThaiText`, lines 24-122) and
//! `text/euroconvert/index.ts` (`EURO_CURRENCIES`, `calculate`, `_getCurrency`,
//! `_getTriangulationDecimals`, lines 24-184), commit 4767b3b309467c283bbd52b725536ca73861e0fc.
//! Apache-2.0, Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! The function names, the helper names, the order of the checks, the branches and the error values
//! are the reference's. Adapter changes only: a value object is an [`EvaluatedCellValue`], an error
//! is `Err(FormulaError)`, an argument is read through `eval_arg_val`, the reference's `round`
//! is this engine's 15-digit `round_to_digits`. Changes beyond the adapter:
//! - `BAHTTEXT` takes the satang from the value rounded to two decimals (`round`), and carries a
//!   satang count of 100 into the baht. The reference multiplies the two-decimal fraction by 100 in
//!   binary (`0.57 * 100` is `56.99999999999999`, which it then reads digit by digit, including the
//!   dot) and reads `1.995` as 100 satang. A value of 10^15 or more has no exact digits left and is
//!   `#VALUE!` (the reference prints exponent text for it).
//! - `EUROCONVERT` rounds to the calculation precision of the currency, which is the reference's
//!   table (the function's documentation: "Excel uses the calculation precision value to calculate
//!   the result and the display precision value to display the result").
//!
//! `PHONETIC` has no reference. Excel gives the furigana of the first cell of its argument and, when
//! the cell holds none, the cell's text. The XLSX reader drops the `rPh` runs of a shared string
//! (`xlsx_sheet.rs`), so the engine has no furigana to give and every cell is a cell without any.

use crate::functions_lookup_text::bounded_text;
use crate::functions_math::{round_to_digits, Rounding};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};

type CellResult = Result<V, FormulaError>;

// ---- BAHTTEXT ----

/// The Thai words of the places, `10^6, 10^1, 10^2, 10^3, 10^4, 10^5, 10^0` (`units`).
const THAI_UNITS: [&str; 7] = ["ล้าน", "สิบ", "ร้อย", "พัน", "หมื่น", "แสน", ""];
/// The Thai digit words 0 to 9 (`digits`).
const THAI_DIGITS: [&str; 10] = [
    "ศูนย์",
    "หนึ่ง",
    "สอง",
    "สาม",
    "สี่",
    "ห้า",
    "หก",
    "เจ็ด",
    "แปด",
    "เก้า",
];
/// Minus, baht, "exactly" (no satang), satang, two in the tens place, one in the last place
/// (`special`).
const THAI_MINUS: &str = "ลบ";
const THAI_BAHT: &str = "บาท";
const THAI_WHOLE: &str = "ถ้วน";
const THAI_SATANG: &str = "สตางค์";
const THAI_TWO_TENS: &str = "ยี่";
const THAI_ONE_LAST: &str = "เอ็ด";

/// Values from this size on have no exact whole digits in an `f64`.
const MAX_BAHT: f64 = 1e15;

fn convert_number_to_thai_text(number: u64) -> String {
    let number_text = number.to_string();
    let n = number_text.len();
    let mut result = String::new();
    for i in (1..=n).rev() {
        let digit = usize::from(number_text.as_bytes()[n - i] - b'0');
        let mut digit_text = THAI_DIGITS[digit];
        let position = if i > 1 { (i - 1) % 6 } else { 6 };
        if position == 1 && digit == 2 {
            digit_text = THAI_TWO_TENS;
        }
        if digit == 1 {
            match position {
                0 | 6 => result.push_str(if i < n { THAI_ONE_LAST } else { digit_text }),
                1 => {}
                _ => result.push_str(digit_text),
            }
        } else if digit == 0 {
            if position == 0 {
                result.push_str(THAI_UNITS[position]);
            }
            continue;
        } else {
            result.push_str(digit_text);
        }
        result.push_str(THAI_UNITS[position]);
    }
    result
}

fn handle_single_object(number_value: f64) -> CellResult {
    let abs = number_value.abs();
    if abs >= MAX_BAHT {
        return Err(FormulaError::Value);
    }
    let mut integer_part = abs.trunc() as u64;
    let mut decimal_part =
        round_to_digits((abs - integer_part as f64) * 100.0, 0.0, Rounding::Nearest)
            .ok_or(FormulaError::Num)? as u64;
    if decimal_part == 100 {
        integer_part += 1;
        decimal_part = 0;
    }
    let minus = if number_value < 0.0 { THAI_MINUS } else { "" };
    let result = if integer_part == 0 {
        if decimal_part != 0 {
            format!(
                "{minus}{}{THAI_SATANG}",
                convert_number_to_thai_text(decimal_part)
            )
        } else {
            format!("{}{THAI_BAHT}{THAI_WHOLE}", THAI_DIGITS[0])
        }
    } else if decimal_part == 0 {
        format!(
            "{minus}{}{THAI_BAHT}{THAI_WHOLE}",
            convert_number_to_thai_text(integer_part)
        )
    } else {
        format!(
            "{minus}{}{THAI_BAHT}{}{THAI_SATANG}",
            convert_number_to_thai_text(integer_part),
            convert_number_to_thai_text(decimal_part)
        )
    };
    Ok(V::Text(result))
}

// ---- EUROCONVERT ----

/// A currency the reference app's `EUROCONVERT` accepts.
struct EuroCurrency {
    code: &'static str,
    /// Units of the currency in one euro: its irrevocably fixed conversion rate.
    rate: f64,
    /// Decimal places a result in the currency is rounded to: the reference app's calculation precision.
    precision: u8,
}

const fn currency(code: &'static str, rate: f64, precision: u8) -> EuroCurrency {
    EuroCurrency {
        code,
        rate,
        precision,
    }
}

const EURO_CURRENCIES: [EuroCurrency; 14] = [
    currency("BEF", 40.3399, 0),
    currency("LUF", 40.3399, 0),
    currency("DEM", 1.95583, 2),
    currency("ESP", 166.386, 0),
    currency("FRF", 6.55957, 2),
    currency("IEP", 0.787564, 2),
    currency("ITL", 1936.27, 0),
    currency("NLG", 2.20371, 2),
    currency("ATS", 13.7603, 2),
    currency("PTE", 200.482, 0),
    currency("FIM", 5.94573, 2),
    currency("GRD", 340.75, 0),
    currency("SIT", 239.64, 2),
    currency("EUR", 1.0, 2),
];

const EURO: &str = "EUR";

const MIN_TRIANGULATION_PRECISION: f64 = 3.0;
/// VBA's Integer upper bound: the reference app converts the argument with `CInt`, so a larger value overflows.
const MAX_TRIANGULATION_PRECISION: f64 = 32767.0;
/// the reference app never rounds the intermediate euro value to more than 15 decimals.
const MAX_EURO_DECIMALS: f64 = 15.0;

/// A number as a cell value; a value that overflowed is `#NUM!`.
fn finite_num(x: f64) -> CellResult {
    if x.is_finite() {
        Ok(V::Number(x))
    } else {
        Err(FormulaError::Num)
    }
}

/// Rounds half away from zero, as the reference app's `ROUND` does.
fn round_half_away_from_zero(value: f64, decimals: f64) -> Result<f64, FormulaError> {
    round_to_digits(value, decimals, Rounding::Nearest).ok_or(FormulaError::Num)
}

fn get_currency(code: &V) -> Option<&'static EuroCurrency> {
    let V::Text(code) = code else {
        return None;
    };
    EURO_CURRENCIES
        .iter()
        .find(|c| c.code.eq_ignore_ascii_case(code))
}

impl SheetEngine {
    /// `BAHTTEXT(number)`
    pub(crate) fn fn_bahttext(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        let number_value = self.eval_arg_num_blank_zero(&args[0], ctx, depth)?;
        match handle_single_object(number_value)? {
            V::Text(s) => bounded_text(s),
            other => Ok(other),
        }
    }

    /// `EUROCONVERT(number, source, target, [full_precision], [triangulation_precision])`
    pub(crate) fn fn_euroconvert(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if !(3..=5).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let number = self.eval_arg_val(&args[0], ctx, depth)?;
        let source = self.eval_arg_val(&args[1], ctx, depth)?;
        let target = self.eval_arg_val(&args[2], ctx, depth)?;
        let full_precision = match args.get(3) {
            None | Some(Expr::Missing) => V::Boolean(false),
            Some(arg) => self.eval_arg_val(arg, ctx, depth)?,
        };
        // the reference app does not evaluate EUROCONVERT over arrays: a multi-cell argument is invalid.
        let to_number = |v: &V| match v {
            V::Empty => Ok(0.0),
            other => other.as_number().ok_or(FormulaError::Value),
        };
        let number_value = to_number(&number)?;
        let is_full_precision = to_number(&full_precision)? != 0.0;
        let (Some(source_currency), Some(target_currency)) =
            (get_currency(&source), get_currency(&target))
        else {
            return Err(FormulaError::Value);
        };
        let triangulation_decimals = match args.get(4) {
            None | Some(Expr::Missing) => None,
            Some(arg) => {
                let precision = self.eval_arg_val(arg, ctx, depth)?;
                Some(self.get_triangulation_decimals(&precision)?)
            }
        };
        // The same currency on both sides returns the number as it is, without rounding.
        if source_currency.code == target_currency.code {
            return finite_num(number_value);
        }
        let mut result = if source_currency.code == EURO {
            number_value * target_currency.rate
        } else {
            // Member currencies convert through the euro, and only that intermediate value is rounded.
            let mut euros = number_value / source_currency.rate;
            if let Some(decimals) = triangulation_decimals {
                euros = round_half_away_from_zero(euros, decimals)?;
            }
            euros * target_currency.rate
        };
        if !is_full_precision {
            result = round_half_away_from_zero(result, f64::from(target_currency.precision))?;
        }
        finite_num(result)
    }

    /// The decimals the intermediate euro value is rounded to, or the error the argument gives.
    fn get_triangulation_decimals(&self, precision: &V) -> Result<f64, FormulaError> {
        let precision = match precision {
            V::Empty => 0.0,
            other => other.as_number().ok_or(FormulaError::Value)?,
        };
        // the reference app converts the argument with VBA's CInt: 3.5 becomes 4 and 2.5 becomes 2.
        let decimals = precision.round_ties_even();
        if decimals > MAX_TRIANGULATION_PRECISION {
            return Err(FormulaError::Num);
        }
        if decimals < MIN_TRIANGULATION_PRECISION {
            return Err(FormulaError::Value);
        }
        Ok(decimals.min(MAX_EURO_DECIMALS))
    }

    /// `PHONETIC(reference)`: phonetic guide of the upper-left cell when present, else cell text (PD-319).
    pub(crate) fn fn_phonetic(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        let resolved = self.extract_range_and_sheet_ctx(&args[0], ctx, depth)?;
        let first_cell = match &resolved {
            Some((Some(sheet), range, _)) => Expr::sheet_cell(sheet.as_ref(), range.start),
            Some((None, range, _)) => Expr::Cell(range.start),
            None => args[0].clone(),
        };
        let text = self.text_arg(&first_cell, ctx, depth)?;
        if let Some((sheet_opt, range, _)) = resolved {
            if let Some(target) = self.resolve_sheet_ref(sheet_opt.as_deref(), ctx) {
                if !crate::dag::is_formula(target.compiled_asts.get(&range.start)) {
                    if let Some(ph) = target.cell_phonetic_coord(range.start) {
                        return bounded_text(ph.to_string());
                    }
                }
            }
        }
        bounded_text(text)
    }
}

//! Engineering, bitwise (48-bit unsigned), radix conversion (10-place two's complement),
//! series/paired-array math, and Fisher/Standardize statistical functions.
//!
//! Adapted from reference implementation `FormulaJS` (`@formulajs/formulajs`, MIT License).

use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

/// Maximum 48-bit unsigned integer accepted by `BITAND`, `BITOR`, `BITXOR`, `BITLSHIFT`, `BITRSHIFT`.
const MAX_BIT_UINT48: u64 = (1u64 << 48) - 1;
const MAX_BIT_SHIFT: i64 = 53;
const BASE_PLACES_MAX: i64 = 10;

const HEX_NEG_MIN: i64 = -(1i64 << 39);
const HEX_POS_MAX: i64 = (1i64 << 39) - 1;
const HEX_TWOS_COMP: i64 = 1i64 << 40;
const HEX_SIGN_BIT: u64 = 1u64 << 39;

const OCT_NEG_MIN: i64 = -(1i64 << 29);
const OCT_POS_MAX: i64 = (1i64 << 29) - 1;
const OCT_TWOS_COMP: i64 = 1i64 << 30;
const OCT_SIGN_BIT: u64 = 1u64 << 29;

const BIN_NEG_MIN: i64 = -512;
const BIN_POS_MAX: i64 = 511;
const BIN_TWOS_COMP: i64 = 1024;
const BIN_SIGN_BIT: u64 = 512;

#[inline]
fn check_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

#[inline]
fn finite_num(x: f64) -> Result<EvaluatedCellValue, FormulaError> {
    if x.is_finite() {
        Ok(EvaluatedCellValue::Number(x))
    } else {
        Err(FormulaError::Num)
    }
}

#[inline]
fn parse_uint48(n: f64) -> Result<u64, FormulaError> {
    if !n.is_finite() || n < 0.0 || n > (MAX_BIT_UINT48 as f64) {
        return Err(FormulaError::Num);
    }
    let rounded = n.round();
    if (n - rounded).abs() > 1e-9 {
        return Err(FormulaError::Num);
    }
    Ok(rounded as u64)
}

fn format_with_places(
    raw: String,
    places: Option<f64>,
) -> Result<EvaluatedCellValue, FormulaError> {
    let Some(p_raw) = places else {
        return Ok(EvaluatedCellValue::Text(raw));
    };
    let p = p_raw.trunc() as i64;
    if !(1..=BASE_PLACES_MAX).contains(&p) || (p as usize) < raw.len() {
        return Err(FormulaError::Num);
    }
    let width = p as usize;
    if width == raw.len() {
        Ok(EvaluatedCellValue::Text(raw))
    } else {
        let mut out = String::with_capacity(width);
        for _ in 0..(width - raw.len()) {
            out.push('0');
        }
        out.push_str(&raw);
        Ok(EvaluatedCellValue::Text(out))
    }
}

fn binom_f64(n: u64, k: u64) -> Result<f64, FormulaError> {
    let k = k.min(n - k);
    let mut acc = 1.0_f64;
    for i in 1..=k {
        acc = acc * ((n - k + i) as f64) / (i as f64);
        if !acc.is_finite() {
            return Err(FormulaError::Num);
        }
    }
    Ok(acc.round())
}

impl SheetEngine {
    pub(crate) fn eval_eng_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "BITAND" | "BITOR" | "BITXOR" => self.eng_bitwise_pair(name, args, ctx, depth),
            "BITLSHIFT" | "BITRSHIFT" => self.eng_bit_shift(name == "BITLSHIFT", args, ctx, depth),
            "HEX2DEC" => self.eng_radix_to_dec(args, 16, HEX_SIGN_BIT, HEX_TWOS_COMP, ctx, depth),
            "OCT2DEC" => self.eng_radix_to_dec(args, 8, OCT_SIGN_BIT, OCT_TWOS_COMP, ctx, depth),
            "BIN2DEC" => self.eng_radix_to_dec(args, 2, BIN_SIGN_BIT, BIN_TWOS_COMP, ctx, depth),
            "DEC2HEX" => self.eng_dec_to_radix(
                args,
                16,
                HEX_NEG_MIN,
                HEX_POS_MAX,
                HEX_TWOS_COMP,
                ctx,
                depth,
            ),
            "DEC2OCT" => {
                self.eng_dec_to_radix(args, 8, OCT_NEG_MIN, OCT_POS_MAX, OCT_TWOS_COMP, ctx, depth)
            }
            "DEC2BIN" => {
                self.eng_dec_to_radix(args, 2, BIN_NEG_MIN, BIN_POS_MAX, BIN_TWOS_COMP, ctx, depth)
            }
            "MULTINOMIAL" => self.eng_multinomial(args, ctx, depth),
            "SERIESSUM" => self.eng_seriessum(args, ctx, depth),
            "SUMX2MY2" | "SUMX2PY2" | "SUMXMY2" => self.eng_sum_paired(name, args, ctx, depth),
            "STANDARDIZE" => self.eng_standardize(args, ctx, depth),
            "FISHER" => self.eng_fisher(args, ctx, depth),
            "FISHERINV" => self.eng_fisherinv(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    fn eng_bitwise_pair(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 2)?;
        let a = parse_uint48(self.eval_arg_num(&args[0], ctx, depth)?)?;
        let b = parse_uint48(self.eval_arg_num(&args[1], ctx, depth)?)?;
        let res = match name {
            "BITAND" => a & b,
            "BITOR" => a | b,
            _ => a ^ b,
        };
        Ok(EvaluatedCellValue::Number(res as f64))
    }

    fn eng_bit_shift(
        &self,
        is_left: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 2)?;
        let a = parse_uint48(self.eval_arg_num(&args[0], ctx, depth)?)?;
        let shift_raw = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
        if !shift_raw.is_finite() || shift_raw.abs() > (MAX_BIT_SHIFT as f64) {
            return Err(FormulaError::Num);
        }
        let sh = if is_left {
            shift_raw as i64
        } else {
            -(shift_raw as i64)
        };
        if sh >= 0 {
            let s = sh as u32;
            if s >= 48 && a != 0 {
                return Err(FormulaError::Num);
            }
            let res = if s >= 64 { 0 } else { a << s };
            if res > MAX_BIT_UINT48 {
                return Err(FormulaError::Num);
            }
            Ok(EvaluatedCellValue::Number(res as f64))
        } else {
            let s = (-sh) as u32;
            let res = if s >= 64 { 0 } else { a >> s };
            Ok(EvaluatedCellValue::Number(res as f64))
        }
    }

    fn eng_radix_to_dec(
        &self,
        args: &[Expr],
        radix: u32,
        sign_bit: u64,
        twos_comp: i64,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 1, 1)?;
        let val = self.eval_arg_val(&args[0], ctx, depth)?;
        let owned: String;
        let text = match &val {
            EvaluatedCellValue::Boolean(_) => return Err(FormulaError::Value),
            EvaluatedCellValue::Empty => "0",
            EvaluatedCellValue::Number(n) => {
                if !n.is_finite() || *n < 0.0 {
                    return Err(FormulaError::Num);
                }
                owned = format!("{}", n.trunc() as u64);
                owned.as_str()
            }
            EvaluatedCellValue::Text(s) => s.trim(),
            EvaluatedCellValue::Error(e) => return Err(*e),
        };
        if text.is_empty() || text.len() > (BASE_PLACES_MAX as usize) {
            return Err(FormulaError::Num);
        }
        let parsed = u64::from_str_radix(text, radix).map_err(|_| FormulaError::Num)?;
        if text.len() == (BASE_PLACES_MAX as usize) && (parsed & sign_bit) != 0 {
            Ok(EvaluatedCellValue::Number(
                ((parsed as i64) - twos_comp) as f64,
            ))
        } else {
            Ok(EvaluatedCellValue::Number(parsed as f64))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn eng_dec_to_radix(
        &self,
        args: &[Expr],
        radix: u32,
        neg_min: i64,
        pos_max: i64,
        twos_comp: i64,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 1, 2)?;
        if matches!(args.first(), Some(Expr::Boolean(_))) {
            return Err(FormulaError::Value);
        }
        let v0 = self.eval_arg_val(&args[0], ctx, depth)?;
        if matches!(v0, EvaluatedCellValue::Boolean(_)) {
            return Err(FormulaError::Value);
        }
        let n = v0.as_number().ok_or(FormulaError::Value)?.trunc();
        if !n.is_finite() || n < (neg_min as f64) || n > (pos_max as f64) {
            return Err(FormulaError::Num);
        }
        let places = if let Some(arg1) = args.get(1) {
            if matches!(arg1, Expr::Missing) {
                return Err(FormulaError::Num);
            }
            let v1 = self.eval_arg_val(arg1, ctx, depth)?;
            if matches!(v1, EvaluatedCellValue::Boolean(_)) {
                return Err(FormulaError::Value);
            }
            Some(v1.as_number().ok_or(FormulaError::Value)?)
        } else {
            None
        };
        if let Some(p) = places {
            let pt = p.trunc() as i64;
            if !(1..=BASE_PLACES_MAX).contains(&pt) {
                return Err(FormulaError::Num);
            }
        }
        let iv = n as i64;
        if iv < 0 {
            let u = (iv + twos_comp) as u64;
            let s = match radix {
                16 => format!("{u:010X}"),
                8 => format!("{u:010o}"),
                _ => format!("{u:010b}"),
            };
            return Ok(EvaluatedCellValue::Text(s));
        }
        let u = iv as u64;
        let raw = match radix {
            16 => format!("{u:X}"),
            8 => format!("{u:o}"),
            _ => format!("{u:b}"),
        };
        format_with_places(raw, places)
    }

    fn eng_multinomial(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        let mut total_n = 0u64;
        let mut acc = 1.0_f64;
        for arg in args {
            if let Some((sheet_opt, min_r, max_r, min_c, max_c, _)) =
                self.extract_streaming_range_ctx(arg, ctx, depth)?
            {
                for r in min_r..=max_r {
                    for c in min_c..=max_c {
                        let Ok(coord) = crate::PackedCellCoordinate::new(r, c as u32) else {
                            continue;
                        };
                        let cell_val =
                            self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
                        if let EvaluatedCellValue::Error(e) = cell_val {
                            return Err(e);
                        }
                        if let EvaluatedCellValue::Number(x) = cell_val {
                            let t = x.trunc();
                            if !t.is_finite() || t < 0.0 || t > 10_000.0 {
                                return Err(FormulaError::Num);
                            }
                            let k = t as u64;
                            total_n = total_n.checked_add(k).ok_or(FormulaError::Num)?;
                            acc *= binom_f64(total_n, k)?;
                            if !acc.is_finite() {
                                return Err(FormulaError::Num);
                            }
                        }
                    }
                }
            } else {
                let x = self.eval_arg_num(arg, ctx, depth)?.trunc();
                if !x.is_finite() || x < 0.0 || x > 10_000.0 {
                    return Err(FormulaError::Num);
                }
                let k = x as u64;
                total_n = total_n.checked_add(k).ok_or(FormulaError::Num)?;
                acc *= binom_f64(total_n, k)?;
                if !acc.is_finite() {
                    return Err(FormulaError::Num);
                }
            }
        }
        finite_num(acc)
    }

    fn eng_seriessum(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 4, 4)?;
        let x = self.eval_arg_num(&args[0], ctx, depth)?;
        let n = self.eval_arg_num(&args[1], ctx, depth)?;
        let m = self.eval_arg_num(&args[2], ctx, depth)?;
        let coeff_vals = if matches!(args[3], Expr::Range(_) | Expr::SheetRange { .. }) {
            self.eval_range_vec(&args[3], ctx, depth)?
        } else {
            vec![EvaluatedCellValue::Number(
                self.eval_arg_num(&args[3], ctx, depth)?,
            )]
        };
        let mut coeffs = Vec::with_capacity(coeff_vals.len());
        for v in coeff_vals {
            match v {
                EvaluatedCellValue::Error(e) => return Err(e),
                EvaluatedCellValue::Number(c) => coeffs.push(c),
                _ => {}
            }
        }
        if coeffs.is_empty() {
            return Err(FormulaError::Value);
        }
        let mut sum = 0.0_f64;
        for (i, c) in coeffs.into_iter().enumerate() {
            let exp = n + (i as f64) * m;
            let term = c * x.powf(exp);
            if !term.is_finite() {
                return Err(FormulaError::Num);
            }
            sum += term;
        }
        finite_num(sum)
    }

    fn eng_sum_paired(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 2)?;
        let xs = self.eval_range_vec(&args[0], ctx, depth)?;
        let ys = self.eval_range_vec(&args[1], ctx, depth)?;
        if xs.len() != ys.len() || xs.is_empty() {
            return Err(FormulaError::Na);
        }
        let mut sum = 0.0_f64;
        for (vx, vy) in xs.iter().zip(ys.iter()) {
            if let EvaluatedCellValue::Error(e) = vx {
                return Err(*e);
            }
            if let EvaluatedCellValue::Error(e) = vy {
                return Err(*e);
            }
            if let (EvaluatedCellValue::Number(x), EvaluatedCellValue::Number(y)) = (vx, vy) {
                sum += match name {
                    "SUMX2MY2" => x * x - y * y,
                    "SUMX2PY2" => x * x + y * y,
                    _ => {
                        let d = x - y;
                        d * d
                    }
                };
            }
        }
        finite_num(sum)
    }

    fn eng_standardize(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 3)?;
        let x = self.eval_arg_num(&args[0], ctx, depth)?;
        let mean = self.eval_arg_num(&args[1], ctx, depth)?;
        let sd = self.eval_arg_num(&args[2], ctx, depth)?;
        if sd <= 0.0 {
            return Err(FormulaError::Num);
        }
        finite_num((x - mean) / sd)
    }

    fn eng_fisher(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 1, 1)?;
        let x = self.eval_arg_num(&args[0], ctx, depth)?;
        if x <= -1.0 || x >= 1.0 {
            return Err(FormulaError::Num);
        }
        finite_num(0.5 * ((1.0 + x) / (1.0 - x)).ln())
    }

    fn eng_fisherinv(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 1, 1)?;
        let y = self.eval_arg_num(&args[0], ctx, depth)?;
        finite_num(y.tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitwise_48bit_and_radix_conversions_match_formulajs() {
        let s = SheetEngine::new("EngTest");
        assert_eq!(
            s.evaluate_formula_str("=BITAND(13, 25)"),
            EvaluatedCellValue::Number(9.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=BITOR(23, 10)"),
            EvaluatedCellValue::Number(31.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=BITXOR(5, 3)"),
            EvaluatedCellValue::Number(6.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=BITLSHIFT(4, 2)"),
            EvaluatedCellValue::Number(16.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=BITRSHIFT(13, 2)"),
            EvaluatedCellValue::Number(3.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=BITLSHIFT(281474976710655, 1)"),
            EvaluatedCellValue::Error(FormulaError::Num)
        );
        assert_eq!(
            s.evaluate_formula_str("=HEX2DEC(\"A5\")"),
            EvaluatedCellValue::Number(165.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=HEX2DEC(\"FFFFFFFF5B\")"),
            EvaluatedCellValue::Number(-165.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DEC2HEX(100, 4)"),
            EvaluatedCellValue::Text("0064".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=DEC2HEX(-54)"),
            EvaluatedCellValue::Text("FFFFFFFFCA".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=BIN2DEC(\"1100100\")"),
            EvaluatedCellValue::Number(100.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=BIN2DEC(\"1111111111\")"),
            EvaluatedCellValue::Number(-1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DEC2BIN(9, 4)"),
            EvaluatedCellValue::Text("1001".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=DEC2BIN(-100)"),
            EvaluatedCellValue::Text("1110011100".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=OCT2DEC(\"54\")"),
            EvaluatedCellValue::Number(44.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=OCT2DEC(\"7777777533\")"),
            EvaluatedCellValue::Number(-165.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DEC2OCT(58, 3)"),
            EvaluatedCellValue::Text("072".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=DEC2OCT(-100)"),
            EvaluatedCellValue::Text("7777777634".to_string())
        );
    }

    #[test]
    fn multinomial_seriessum_paired_sums_fisher_standardize_and_fin_extensions() {
        let mut s = SheetEngine::new("MathStatFinTest");
        assert_eq!(
            s.evaluate_formula_str("=MULTINOMIAL(2, 3, 4)"),
            EvaluatedCellValue::Number(1260.0)
        );
        for (i, (x, y)) in [(2, 1), (3, 4), (9, 2), (1, 0), (8, 7), (7, 5), (5, 3)]
            .into_iter()
            .enumerate()
        {
            let r = i + 1;
            s.set_cell(&format!("A{r}"), x.to_string()).unwrap();
            s.set_cell(&format!("B{r}"), y.to_string()).unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=SUMX2MY2(A1:A7, B1:B7)"),
            EvaluatedCellValue::Number(129.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMX2PY2(A1:A7, B1:B7)"),
            EvaluatedCellValue::Number(337.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMXMY2(A1:A7, B1:B7)"),
            EvaluatedCellValue::Number(61.0)
        );
        s.set_cell("C1", "1").unwrap();
        s.set_cell("C2", "2").unwrap();
        s.set_cell("C3", "3").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=SERIESSUM(2, 1, 1, C1:C3)"),
            EvaluatedCellValue::Number(34.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=STANDARDIZE(42, 40, 1.5)"),
            EvaluatedCellValue::Number(4.0 / 3.0)
        );
        let EvaluatedCellValue::Number(f) = s.evaluate_formula_str("=FISHER(0.75)") else {
            panic!("expected FISHER number");
        };
        assert!((f - 0.9729550745).abs() < 1e-8);
        let EvaluatedCellValue::Number(fi) = s.evaluate_formula_str("=FISHERINV(0.9729550745)")
        else {
            panic!("expected FISHERINV number");
        };
        assert!((fi - 0.75).abs() < 1e-8);

        let EvaluatedCellValue::Number(pdur) =
            s.evaluate_formula_str("=PDURATION(0.025, 2000, 2200)")
        else {
            panic!("expected PDURATION number");
        };
        assert!((pdur - 3.859866).abs() < 1e-5);
        let EvaluatedCellValue::Number(rri) = s.evaluate_formula_str("=RRI(96, 10000, 11000)")
        else {
            panic!("expected RRI number");
        };
        assert!((rri - 0.000993307).abs() < 1e-7);
        assert_eq!(
            s.evaluate_formula_str("=DOLLARDE(1.02, 16)"),
            EvaluatedCellValue::Number(1.125)
        );
        assert_eq!(
            s.evaluate_formula_str("=DOLLARFR(1.125, 16)"),
            EvaluatedCellValue::Number(1.02)
        );
    }

    #[test]
    fn subtotal_coords_cache_and_korean_alias_ascii_fast_path() {
        let mut s = SheetEngine::new("SubtotalCache");
        s.set_cell("A1", "10").unwrap();
        s.set_cell("A2", "20").unwrap();
        s.set_cell("A3", "=SUBTOTAL(9, A1:A2)").unwrap();
        let a3 = crate::PackedCellCoordinate::parse_a1("A3").unwrap();
        assert!(s.subtotal_coords.contains(&a3));
        assert_eq!(
            s.evaluate_formula_str("=SUBTOTAL(9, A1:A3)"),
            EvaluatedCellValue::Number(30.0)
        );
        s.set_cell("A3", "40").unwrap();
        assert!(!s.subtotal_coords.contains(&a3));
        assert_eq!(
            s.evaluate_formula_str("=SUBTOTAL(9, A1:A3)"),
            EvaluatedCellValue::Number(70.0)
        );

        let ascii = crate::parser::normalize_korean_function_aliases("SUM");
        assert!(matches!(ascii, std::borrow::Cow::Borrowed("SUM")));
        let ko = crate::parser::normalize_korean_function_aliases("합계");
        assert_eq!(ko.as_ref(), "SUM");
        assert_eq!(
            s.evaluate_formula_str("=합계(A1:A3)"),
            EvaluatedCellValue::Number(70.0)
        );
    }
}

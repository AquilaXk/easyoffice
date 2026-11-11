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
    if !(min..=max).contains(&args.len()) {
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
    if !n.is_finite() || !(0.0..=(MAX_BIT_UINT48 as f64)).contains(&n) {
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
            "BIN2HEX" => self.eng_radix_to_radix(
                args,
                (2, BIN_SIGN_BIT, BIN_TWOS_COMP),
                (16, HEX_NEG_MIN, HEX_POS_MAX, HEX_TWOS_COMP),
                ctx,
                depth,
            ),
            "BIN2OCT" => self.eng_radix_to_radix(
                args,
                (2, BIN_SIGN_BIT, BIN_TWOS_COMP),
                (8, OCT_NEG_MIN, OCT_POS_MAX, OCT_TWOS_COMP),
                ctx,
                depth,
            ),
            "OCT2BIN" => self.eng_radix_to_radix(
                args,
                (8, OCT_SIGN_BIT, OCT_TWOS_COMP),
                (2, BIN_NEG_MIN, BIN_POS_MAX, BIN_TWOS_COMP),
                ctx,
                depth,
            ),
            "OCT2HEX" => self.eng_radix_to_radix(
                args,
                (8, OCT_SIGN_BIT, OCT_TWOS_COMP),
                (16, HEX_NEG_MIN, HEX_POS_MAX, HEX_TWOS_COMP),
                ctx,
                depth,
            ),
            "HEX2BIN" => self.eng_radix_to_radix(
                args,
                (16, HEX_SIGN_BIT, HEX_TWOS_COMP),
                (2, BIN_NEG_MIN, BIN_POS_MAX, BIN_TWOS_COMP),
                ctx,
                depth,
            ),
            "HEX2OCT" => self.eng_radix_to_radix(
                args,
                (16, HEX_SIGN_BIT, HEX_TWOS_COMP),
                (8, OCT_NEG_MIN, OCT_POS_MAX, OCT_TWOS_COMP),
                ctx,
                depth,
            ),
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
        if !n.is_finite() || !((neg_min as f64)..=(pos_max as f64)).contains(&n) {
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

    fn eng_radix_to_radix(
        &self,
        args: &[Expr],
        src: (u32, u64, i64),
        dst: (u32, i64, i64, i64),
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 1, 2)?;
        if matches!(args.first(), Some(Expr::Boolean(_))) {
            return Err(FormulaError::Value);
        }
        let dec = self.eng_radix_to_dec(
            std::slice::from_ref(&args[0]),
            src.0,
            src.1,
            src.2,
            ctx,
            depth,
        )?;
        let EvaluatedCellValue::Number(n) = dec else {
            return Err(FormulaError::Value);
        };
        let mut dec_args = Vec::with_capacity(args.len());
        dec_args.push(Expr::Number(n));
        if let Some(arg1) = args.get(1) {
            dec_args.push(arg1.clone());
        }
        self.eng_dec_to_radix(&dec_args, dst.0, dst.1, dst.2, dst.3, ctx, depth)
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
            if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                for dr in 0..spec.3 {
                    for dc in 0..spec.4 {
                        let cell_val = self.eval_streaming_cell(&spec, dr, dc, ctx, depth)?;
                        if let EvaluatedCellValue::Number(x) = cell_val {
                            let t = x.trunc();
                            if !t.is_finite() || !(0.0..=10_000.0).contains(&t) {
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
                if !x.is_finite() || !(0.0..=10_000.0).contains(&x) {
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
    fn multinomial_reads_range_arguments() {
        let mut s = SheetEngine::new("MultinomialRange");
        for (r, v) in [(2, 2), (3, 3), (4, 4)] {
            s.set_cell(&format!("A{r}"), v.to_string()).unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=MULTINOMIAL(A2:A4)"),
            EvaluatedCellValue::Number(1260.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=MULTINOMIAL(A2:A3, 4)"),
            EvaluatedCellValue::Number(1260.0)
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

    #[test]
    fn cross_radix_skew_p_percentrank_days360_dollar_steyx_combina_permutationa() {
        let mut s = SheetEngine::new("Round2UpgradeTest");
        // Cross-radix 10-place two's complement conversions
        assert_eq!(
            s.evaluate_formula_str("=BIN2HEX(\"11111011\", 4)"),
            EvaluatedCellValue::Text("00FB".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=BIN2HEX(\"1111111111\")"),
            EvaluatedCellValue::Text("FFFFFFFFFF".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=BIN2OCT(\"1001\", 3)"),
            EvaluatedCellValue::Text("011".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=BIN2OCT(\"1111111111\")"),
            EvaluatedCellValue::Text("7777777777".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=OCT2BIN(\"3\", 3)"),
            EvaluatedCellValue::Text("011".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=OCT2BIN(\"7777777000\")"),
            EvaluatedCellValue::Text("1000000000".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=OCT2HEX(\"100\", 4)"),
            EvaluatedCellValue::Text("0040".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=OCT2HEX(\"7777777533\")"),
            EvaluatedCellValue::Text("FFFFFFFF5B".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=HEX2BIN(\"F\", 8)"),
            EvaluatedCellValue::Text("00001111".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=HEX2BIN(\"FFFFFFFFFF\")"),
            EvaluatedCellValue::Text("1111111111".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=HEX2OCT(\"F\", 3)"),
            EvaluatedCellValue::Text("017".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=HEX2OCT(\"FFFFFFFF00\")"),
            EvaluatedCellValue::Text("7777777400".to_string())
        );

        // SKEW.P
        for (i, v) in [3, 4, 5, 2, 3, 4, 5, 6, 4, 7].into_iter().enumerate() {
            s.set_cell(&format!("A{}", i + 1), v.to_string()).unwrap();
        }
        let EvaluatedCellValue::Number(skew_p) = s.evaluate_formula_str("=SKEW.P(A1:A10)") else {
            panic!("expected SKEW.P number");
        };
        assert!((skew_p - 0.30319333935499).abs() < 1e-8);

        // PERCENTRANK, PERCENTRANK.INC, PERCENTRANK.EXC
        for (i, v) in [1, 2, 3, 4, 6, 8, 9, 11, 12, 13].into_iter().enumerate() {
            s.set_cell(&format!("B{}", i + 1), v.to_string()).unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=PERCENTRANK(B1:B10, 4)"),
            EvaluatedCellValue::Number(0.333)
        );
        assert_eq!(
            s.evaluate_formula_str("=PERCENTRANK.INC(B1:B10, 5, 4)"),
            EvaluatedCellValue::Number(0.3888)
        );
        assert_eq!(
            s.evaluate_formula_str("=PERCENTRANK.EXC(B1:B10, 4)"),
            EvaluatedCellValue::Number(0.363)
        );
        assert_eq!(
            s.evaluate_formula_str("=PERCENTRANK.INC(B1:B10, 20)"),
            EvaluatedCellValue::Error(FormulaError::Na)
        );

        // DAYS360 (U.S. NASD 30/360 and European 30E/360)
        assert_eq!(
            s.evaluate_formula_str("=DAYS360(DATE(2024,1,30), DATE(2024,2,1))"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DAYS360(DATE(2024,1,1), DATE(2024,3,31), FALSE)"),
            EvaluatedCellValue::Number(90.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DAYS360(DATE(2024,1,1), DATE(2024,3,31), TRUE)"),
            EvaluatedCellValue::Number(89.0)
        );

        // DOLLAR
        assert_eq!(
            s.evaluate_formula_str("=DOLLAR(1234.567, 2)"),
            EvaluatedCellValue::Text("$1,234.57".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=DOLLAR(-1234.567, -2)"),
            EvaluatedCellValue::Text("($1,200)".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=DOLLAR(-0.001, 2)"),
            EvaluatedCellValue::Text("$0.00".to_string())
        );

        // STEYX
        for (i, (y, x)) in [(2, 6), (3, 5), (9, 11), (1, 7), (8, 5), (7, 4), (5, 4)]
            .into_iter()
            .enumerate()
        {
            s.set_cell(&format!("C{}", i + 1), y.to_string()).unwrap();
            s.set_cell(&format!("D{}", i + 1), x.to_string()).unwrap();
        }
        let EvaluatedCellValue::Number(steyx) = s.evaluate_formula_str("=STEYX(C1:C7, D1:D7)")
        else {
            panic!("expected STEYX number");
        };
        assert!((steyx - 3.3057189502).abs() < 1e-6);

        // PERMUTATIONA and COMBINA
        assert_eq!(
            s.evaluate_formula_str("=PERMUTATIONA(3, 2)"),
            EvaluatedCellValue::Number(9.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=PERMUTATIONA(0, 0)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COMBINA(4, 3)"),
            EvaluatedCellValue::Number(20.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COMBINA(10, 3)"),
            EvaluatedCellValue::Number(220.0)
        );
    }
}

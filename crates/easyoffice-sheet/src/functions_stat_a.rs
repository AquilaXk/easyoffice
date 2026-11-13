//! Statistics over numbers, text and logical values: `MAXA`, `MINA`, `STDEVA`, `STDEVPA`,
//! `VARA`, `VARPA` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/functions/statistical/{maxa,mina,stdeva,
//! stdevpa,vara,varpa}/index.ts` and `src/functions/base-function.ts` (`flattenArray`) of the Univer
//! formula engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright
//! 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Text in a range counts as 0 and logical values as 1 and 0; a text or logical value written
//! straight into the call is converted (non-numeric text is `#VALUE!`). Blank cells are skipped; the
//! reference counts a blank as 0 (PD-201 lists the difference).

use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError as E, SheetEngine};

type Fallible<T> = Result<T, E>;

fn is_reference(arg: &Expr) -> bool {
    match arg.peeled() {
        Expr::Cell(_) | Expr::Range(_) | Expr::SheetCell { .. } | Expr::SheetRange { .. } => true,
        Expr::FuncCall { name, .. } => {
            crate::functions_matrix::is_array_fn(name)
                || matches!(name.as_str(), "INDIRECT" | "OFFSET" | "TRANSPOSE")
        }
        _ => false,
    }
}

impl SheetEngine {
    /// Calls `f` with every value of the arguments as the "A" functions read them.
    fn visit_a_values(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
        mut f: impl FnMut(f64),
    ) -> Fallible<()> {
        if args.is_empty() {
            return Err(E::Value);
        }
        for arg in args {
            if is_reference(arg) {
                self.visit_args_streaming(std::slice::from_ref(arg), ctx, depth, |v| {
                    match v {
                        V::Number(n) => f(n),
                        V::Boolean(b) => f(f64::from(u8::from(b))),
                        V::Text(_) => f(0.0),
                        V::Empty | V::Error(_) => {}
                    }
                    Ok(())
                })?;
                continue;
            }
            match self.eval_expr_ctx(arg, ctx, depth) {
                V::Error(e) => return Err(e),
                V::Boolean(b) => f(f64::from(u8::from(b))),
                V::Empty => f(0.0),
                other => f(other.as_number().ok_or(E::Value)?),
            }
        }
        Ok(())
    }

    pub(crate) fn eval_stat_a_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<V> {
        if matches!(name, "MAXA" | "MINA") {
            let is_max = name == "MAXA";
            let mut best: Option<f64> = None;
            self.visit_a_values(args, ctx, depth, |x| {
                best = Some(match best {
                    Some(b) if is_max => b.max(x),
                    Some(b) => b.min(x),
                    None => x,
                });
            })?;
            return Ok(V::Number(best.unwrap_or(0.0)));
        }
        let (mut count, mut mean, mut m2) = (0usize, 0.0f64, 0.0f64);
        self.visit_a_values(args, ctx, depth, |x| {
            count += 1;
            let delta = x - mean;
            mean += delta / count as f64;
            m2 += delta * (x - mean);
        })?;
        let sample = matches!(name, "STDEVA" | "VARA");
        let divisor = count as f64 - f64::from(u8::from(sample));
        if divisor <= 0.0 {
            return Err(E::DivByZero);
        }
        let variance = m2 / divisor;
        Ok(V::Number(if name.starts_with("STDEV") {
            variance.sqrt()
        } else {
            variance
        }))
    }
}

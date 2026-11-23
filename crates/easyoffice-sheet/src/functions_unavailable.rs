//! `WEBSERVICE`, `RTD`, `FILTERXML`, `IMAGE`, `CALL`, `REGISTER.ID` and `STOCKHISTORY`:
//! names the engine knows and cannot serve offline or under workbook security policy (PD-271, PD-318).
//!
//! 참조 구현: none. These functions reach out over a network, connect to a real-time or market-data
//! server, or load external libraries (`CALL`/`REGISTER.ID`), which an untrusted document must not
//! trigger automatically (`FILTERXML` awaits a bounded cleanroom evaluator). Each function checks its
//! argument count and required positional arguments (`#VALUE!` when invalid), evaluates non-missing
//! arguments first to propagate any error in them, and then returns its documented error literal:
//! - `WEBSERVICE` -> `#VALUE!`
//! - `FILTERXML` -> `#VALUE!`
//! - `RTD` -> `#N/A`
//! - `IMAGE`, `CALL`, `REGISTER.ID` -> `#BLOCKED!`
//! - `STOCKHISTORY` -> `#CONNECT!`

use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

fn req(args: &[Expr], idx: usize) -> bool {
    args.get(idx).is_some_and(|a| !matches!(a, Expr::Missing))
}

impl SheetEngine {
    pub(crate) fn eval_unavailable_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let (arity_ok, err) = match name {
            "WEBSERVICE" => (args.len() == 1 && req(args, 0), FormulaError::Value),
            "FILTERXML" => (
                args.len() == 2 && req(args, 0) && req(args, 1),
                FormulaError::Value,
            ),
            "RTD" => (
                args.len() >= 3 && req(args, 0) && req(args, 2),
                FormulaError::Na,
            ),
            "IMAGE" => (
                (1..=5).contains(&args.len()) && req(args, 0),
                FormulaError::Blocked,
            ),
            "CALL" => (!args.is_empty() && req(args, 0), FormulaError::Blocked),
            "REGISTER.ID" => (
                (2..=3).contains(&args.len()) && req(args, 0) && req(args, 1),
                FormulaError::Blocked,
            ),
            "STOCKHISTORY" => (
                (2..=11).contains(&args.len()) && req(args, 0) && req(args, 1),
                FormulaError::Connect,
            ),
            _ => (false, FormulaError::Name),
        };
        if !arity_ok {
            return Err(FormulaError::Value);
        }
        for arg in args.iter().filter(|a| !matches!(a, Expr::Missing)) {
            self.eval_arg_val(arg, ctx, depth)?;
        }
        Err(err)
    }
}

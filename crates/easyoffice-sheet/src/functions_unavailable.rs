//! `WEBSERVICE`, `RTD` and `FILTERXML`: names the engine knows and cannot serve (PD-271).
//!
//! 참조 구현: none. Both functions need a network or a server that a document must not reach on
//! its own (an untrusted document would make the program call out), so neither is built. The
//! function documentation says what a call that cannot be served gives, and that is what these
//! return, instead of `#NAME?` for a name the reference app knows: `WEBSERVICE` is `#VALUE!` ("if the
//! arguments are unable to return the data") and `RTD` is `#N/A` (no real-time data server). `FILTERXML` has no
//! maintained XPath crate to run on (the one tried has no release since 2019, panics on a prefixed
//! name and has no work budget), so it answers `#VALUE!`, the documented error for an XML text it
//! cannot read. The
//! arguments are read first, so an error in one of them is that error. The other functions that
//! need a service (`STOCKHISTORY`, `DETECTLANGUAGE`, `TRANSLATE`, `IMAGE`, `CALL`, `REGISTER.ID`
//! and the cube functions) are left as `#NAME?`: their documented errors (`#BLOCKED!`,
//! `#CONNECT!`) are error values this engine does not have.

use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

impl SheetEngine {
    pub(crate) fn eval_unavailable_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let arity_ok = match name {
            "WEBSERVICE" => args.len() == 1,
            "FILTERXML" => args.len() == 2,
            _ => args.len() >= 3,
        };
        if !arity_ok {
            return Err(FormulaError::Value);
        }
        for arg in args.iter().filter(|a| !matches!(a, Expr::Missing)) {
            self.eval_arg_val(arg, ctx, depth)?;
        }
        Err(if name == "RTD" {
            FormulaError::Na
        } else {
            FormulaError::Value
        })
    }
}

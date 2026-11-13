//! Which formulas need the array evaluator (PD-217): those that call an array function, write an
//! array constant or a spill range reference, or calculate with ranges where one value is wanted.
//! The answer is worked out when a cell is compiled, so an ordinary formula pays nothing.

use crate::functions_matrix::is_array_fn;
use crate::Expr;

/// Functions that make or consume arrays and run only in the array evaluator.
pub(crate) fn is_spill_fn(name: &str) -> bool {
    matches!(
        name,
        "FILTER"
            | "SORT"
            | "SORTBY"
            | "UNIQUE"
            | "SEQUENCE"
            | "RANDARRAY"
            | "LET"
            | "LAMBDA"
            | "MAP"
            | "REDUCE"
            | "SCAN"
            | "BYROW"
            | "BYCOL"
            | "MAKEARRAY"
    )
}

/// Functions that take one value per argument: given an array they calculate once per element.
pub(crate) fn is_lifted_fn(name: &str) -> bool {
    matches!(
        name,
        "IF" | "IFS"
            | "IFERROR"
            | "IFNA"
            | "SWITCH"
            | "NOT"
            | "ABS"
            | "SIGN"
            | "ROUND"
            | "ROUNDUP"
            | "ROUNDDOWN"
            | "INT"
            | "TRUNC"
            | "CEILING"
            | "FLOOR"
            | "MOD"
            | "QUOTIENT"
            | "SQRT"
            | "POWER"
            | "EXP"
            | "LN"
            | "LOG"
            | "LOG10"
            | "SIN"
            | "COS"
            | "TAN"
            | "ASIN"
            | "ACOS"
            | "ATAN"
            | "ATAN2"
            | "SINH"
            | "COSH"
            | "TANH"
            | "DEGREES"
            | "RADIANS"
            | "EVEN"
            | "ODD"
            | "MROUND"
            | "FACT"
            | "LEN"
            | "LEFT"
            | "RIGHT"
            | "MID"
            | "TRIM"
            | "UPPER"
            | "LOWER"
            | "PROPER"
            | "SUBSTITUTE"
            | "REPLACE"
            | "FIND"
            | "SEARCH"
            | "EXACT"
            | "REPT"
            | "VALUE"
            | "TEXT"
            | "CHAR"
            | "CODE"
            | "DATE"
            | "YEAR"
            | "MONTH"
            | "DAY"
            | "TIME"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "DAYS"
            | "EDATE"
            | "ISNUMBER"
            | "ISTEXT"
            | "ISBLANK"
            | "ISERROR"
            | "ISERR"
            | "ISNA"
            | "ISLOGICAL"
            | "ISNONTEXT"
            | "ISEVEN"
            | "ISODD"
    )
}

/// Whether `expr` can be an array of more than one value.
pub(crate) fn arrayish(expr: &Expr) -> bool {
    match expr.peeled() {
        Expr::Range(r) => r.start != r.end,
        Expr::SheetRange { range, .. } => range.start != range.end,
        Expr::Array(lit) => lit.rows * lit.cols > 1,
        Expr::SpillRef { .. } | Expr::Var(_) | Expr::Call { .. } => true,
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            arrayish(inner)
        }
        Expr::Binary { lhs, rhs, .. } => arrayish(lhs) || arrayish(rhs),
        Expr::FuncCall { name, args } => {
            let name = name.as_str();
            is_spill_fn(name)
                || name == "TRANSPOSE"
                || is_array_fn(name)
                || (name == "XLOOKUP" && xlookup_returns_array(args))
                || (is_lifted_fn(name) && args.iter().any(arrayish))
        }
        _ => false,
    }
}

/// `XLOOKUP` whose return range has more than one value for a match: more columns than the lookup
/// range's one column, or more rows than its one row.
pub(crate) fn xlookup_returns_array(args: &[Expr]) -> bool {
    let size = |e: &Expr| match e.peeled() {
        Expr::Range(r) | Expr::SheetRange { range: r, .. } => Some(r.dimensions()),
        _ => None,
    };
    match (args.get(1).and_then(size), args.get(2).and_then(size)) {
        (Some((_, 1)), Some((_, ret_cols))) => ret_cols > 1,
        (Some((1, _)), Some((ret_rows, _))) => ret_rows > 1,
        _ => false,
    }
}

/// Whether a formula must be calculated by the array evaluator: its result is an array, or a part of
/// it is an array function, a constant, a spill range reference, a call, or an operator or a
/// one-value function applied to ranges. The arguments of `SUMPRODUCT` are left to the scalar engine,
/// which calculates them element by element itself.
pub(crate) fn needs_array(root: &Expr) -> bool {
    arrayish(root) || {
        let mut seen = std::collections::HashSet::new();
        scan(root, &mut seen)
    }
}

fn scan(expr: &Expr, seen: &mut std::collections::HashSet<*const crate::SharedExpr>) -> bool {
    match expr {
        Expr::Shared(shared) => {
            seen.insert(std::sync::Arc::as_ptr(shared)) && scan(&shared.expr, seen)
        }
        Expr::Array(_) | Expr::SpillRef { .. } | Expr::Call { .. } | Expr::Var(_) => true,
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            arrayish(inner) || scan(inner, seen)
        }
        Expr::Binary { lhs, rhs, .. } => {
            arrayish(lhs) || arrayish(rhs) || scan(lhs, seen) || scan(rhs, seen)
        }
        Expr::FuncCall { name, args } => {
            let name = name.as_str();
            if is_spill_fn(name) {
                return true;
            }
            if name == "SUMPRODUCT" {
                return args.iter().any(|a| {
                    matches!(
                        a.peeled(),
                        Expr::Array(_) | Expr::SpillRef { .. } | Expr::Call { .. } | Expr::Var(_)
                    ) || scan_spill_only(a)
                });
            }
            (is_lifted_fn(name) && args.iter().any(arrayish)) || args.iter().any(|a| scan(a, seen))
        }
        _ => false,
    }
}

/// Whether `expr` holds an array function, as the arguments of `SUMPRODUCT` may.
fn scan_spill_only(expr: &Expr) -> bool {
    let mut found = false;
    crate::names::walk(expr, |node| {
        found |= matches!(node, Expr::FuncCall { name, .. } if is_spill_fn(name));
    });
    found
}

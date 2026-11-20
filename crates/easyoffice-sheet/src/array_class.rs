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
            | "TAKE"
            | "DROP"
            | "TOCOL"
            | "TOROW"
            | "VSTACK"
            | "HSTACK"
            | "CHOOSEROWS"
            | "CHOOSECOLS"
            | "EXPAND"
            | "WRAPROWS"
            | "WRAPCOLS"
            | "ISOMITTED"
            | "TEXTSPLIT"
            | "TRIMRANGE"
            | "REGEXTEST"
            | "REGEXEXTRACT"
            | "REGEXREPLACE"
            | "ARRAYTOTEXT"
            | "GROUPBY"
            | "PIVOTBY"
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
            | "HYPERLINK"
            | "ACOSH"
            | "ASINH"
            | "ATANH"
            | "ACOT"
            | "ACOTH"
            | "COT"
            | "COTH"
            | "CSC"
            | "CSCH"
            | "SEC"
            | "SECH"
            | "COMBIN"
            | "COMBINA"
            | "FACTDOUBLE"
            | "SQRTPI"
            | "ROUNDBANK"
            | "CEILING.MATH"
            | "CEILING.PRECISE"
            | "FLOOR.MATH"
            | "FLOOR.PRECISE"
            | "ISO.CEILING"
            | "BASE"
            | "DECIMAL"
            | "ARABIC"
            | "ROMAN"
            | "CLEAN"
            | "UNICHAR"
            | "UNICODE"
            | "ENCODEURL"
            | "NUMBERVALUE"
            | "DOLLAR"
            | "FIXED"
            | "TEXTBEFORE"
            | "TEXTAFTER"
            | "LENB"
            | "LEFTB"
            | "RIGHTB"
            | "MIDB"
            | "FINDB"
            | "SEARCHB"
            | "REPLACEB"
            | "ASC"
            | "DBCS"
            | "JIS"
            | "BAHTTEXT"
            | "WEEKDAY"
            | "WEEKNUM"
            | "ISOWEEKNUM"
            | "DATEVALUE"
            | "TIMEVALUE"
            | "EOMONTH"
            | "DAYS360"
            | "DATEDIF"
            | "YEARFRAC"
            | "BITAND"
            | "BITOR"
            | "BITXOR"
            | "BITLSHIFT"
            | "BITRSHIFT"
            | "HEX2DEC"
            | "DEC2HEX"
            | "BIN2DEC"
            | "DEC2BIN"
            | "OCT2DEC"
            | "DEC2OCT"
            | "BIN2HEX"
            | "BIN2OCT"
            | "OCT2BIN"
            | "OCT2HEX"
            | "HEX2BIN"
            | "HEX2OCT"
            | "DELTA"
            | "GESTEP"
            | "ERF"
            | "ERFC"
            | "ERF.PRECISE"
            | "ERFC.PRECISE"
            | "PHI"
            | "GAUSS"
            | "FISHER"
            | "FISHERINV"
            | "NORM.DIST"
            | "NORMDIST"
            | "NORM.INV"
            | "NORMINV"
            | "NORM.S.DIST"
            | "NORMSDIST"
            | "NORM.S.INV"
            | "NORMSINV"
    )
}

/// Whether `expr` can be an array of more than one value.
pub(crate) fn arrayish(expr: &Expr) -> bool {
    match expr.peeled() {
        Expr::Range(r) => r.start != r.end,
        Expr::SheetRange(sr) => sr.range.start != sr.range.end,
        Expr::Array(lit) => lit.rows * lit.cols > 1,
        Expr::SpillRef(_) | Expr::Var(_) | Expr::Call(_) => true,
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            arrayish(inner)
        }
        Expr::Binary { lhs, rhs, .. } => arrayish(lhs) || arrayish(rhs),
        Expr::FuncCall(f) => {
            let name = f.name.as_str();
            let args = &f.args;
            is_spill_fn(name)
                || name == "TRANSPOSE"
                || is_array_fn(name)
                || (name == "XLOOKUP" && xlookup_returns_array(args))
                || (name == "INDEX" && index_returns_line(args))
                || (is_lifted_fn(name) && args.iter().any(arrayish))
        }
        _ => false,
    }
}

/// `INDEX` that gives a row, a column or the whole range: a zero or left out row or column number, or
/// a row number alone on a range of more than one row and column (the numbers are read as written).
pub(crate) fn index_returns_line(args: &[Expr]) -> bool {
    let zero = |e: &Expr| {
        matches!(e.peeled(), Expr::Missing) || matches!(e.peeled(), Expr::Number(n) if *n == 0.0)
    };
    match args {
        [array, row] => arrayish(array) && !matches!(row.peeled(), Expr::Missing),
        [_, row, col] => zero(row) || zero(col),
        _ => false,
    }
}

/// `XLOOKUP` whose return range has more than one value for a match: more columns than the lookup
/// range's one column, or more rows than its one row.
pub(crate) fn xlookup_returns_array(args: &[Expr]) -> bool {
    let size = |e: &Expr| match e.peeled() {
        Expr::Range(r) => Some(r.dimensions()),
        Expr::SheetRange(sr) => Some(sr.range.dimensions()),
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
        let mut seen = crate::DetSet::default();
        scan(root, &mut seen)
    }
}

fn scan(expr: &Expr, seen: &mut crate::DetSet<*const crate::SharedExpr>) -> bool {
    match expr {
        Expr::Shared(shared) => {
            seen.insert(std::sync::Arc::as_ptr(shared)) && scan(&shared.expr, seen)
        }
        Expr::Array(_) | Expr::SpillRef(_) | Expr::Call(_) | Expr::Var(_) => true,
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            arrayish(inner) || scan(inner, seen)
        }
        Expr::Binary { lhs, rhs, .. } => {
            arrayish(lhs) || arrayish(rhs) || scan(lhs, seen) || scan(rhs, seen)
        }
        Expr::FuncCall(f) => {
            let name = f.name.as_str();
            let args = &f.args;
            if is_spill_fn(name) || (name == "HYPERLINK" && (1..=2).contains(&args.len())) {
                return true;
            }
            if name == "INDEX" && index_returns_line(args) {
                return true;
            }
            // `TYPE` of an array is 64, which only the array evaluator can tell.
            if name == "TYPE" && args.first().is_some_and(arrayish) {
                return true;
            }
            if name == "SUMPRODUCT" {
                return args.iter().any(|a| {
                    matches!(
                        a.peeled(),
                        Expr::Array(_) | Expr::SpillRef(_) | Expr::Call(_) | Expr::Var(_)
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
        found |= matches!(
            node,
            Expr::FuncCall(f)
                if is_spill_fn(&f.name)
                    || (f.name == "HYPERLINK" && (1..=2).contains(&f.args.len()))
        );
    });
    found
}

/// Whether `expr` contains at least one syntactically valid `HYPERLINK` call (`1..=2` arguments),
/// traversing `Expr::Shared` defined-name subtrees at most once per shared pointer.
pub(crate) fn expr_contains_valid_hyperlink(expr: &Expr) -> bool {
    let mut seen = crate::DetSet::default();
    let mut stack = vec![expr];
    while let Some(node) = stack.pop() {
        match node {
            Expr::Shared(shared) => {
                if seen.insert(std::sync::Arc::as_ptr(shared)) {
                    stack.push(&shared.expr);
                }
            }
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                stack.push(inner);
            }
            Expr::Binary { lhs, rhs, .. } => {
                stack.push(lhs);
                stack.push(rhs);
            }
            Expr::FuncCall(f) => {
                if f.name == "HYPERLINK" && (1..=2).contains(&f.args.len()) {
                    return true;
                }
                stack.extend(f.args.iter());
            }
            Expr::Call(c) => {
                stack.push(&c.callee);
                stack.extend(c.args.iter());
            }
            _ => {}
        }
    }
    false
}

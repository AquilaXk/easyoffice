//! `[MS-XLSX]` `_xleta.` eta-reduced built-in lambda function table and token-position helpers.

use super::xlsx_formula_text::Tok;

/// Case-insensitive OOXML formula symbol prefixes stripped iteratively.
const SINGLE_PREFIXES_CI: [&str; 4] = ["_xleta.", "_xlfn.", "_xlws.", "_xlpm."];

/// Sorted uppercase built-in spreadsheet functions that may serialize with `_xleta.` inside explicit
/// eta-reduced lambda argument positions (mirrors engine canonical functions excluding `TRUE`/`FALSE`).
pub(crate) const BUILTIN_ETA_FUNCTIONS: [&str; 313] = [
    "ABS",
    "ACCRINT",
    "ACCRINTM",
    "ACOSH",
    "ACOT",
    "ACOTH",
    "ADDRESS",
    "AGGREGATE",
    "AMORDEGRC",
    "AMORLINC",
    "AND",
    "AREAS",
    "ARRAYTOTEXT",
    "ASC",
    "ASINH",
    "ATANH",
    "AVERAGE",
    "AVERAGE.WEIGHTED",
    "AVERAGEA",
    "AVERAGEIF",
    "AVERAGEIFS",
    "BAHTTEXT",
    "BESSELI",
    "BESSELJ",
    "BESSELK",
    "BESSELY",
    "BINOM.DIST.RANGE",
    "BYCOL",
    "BYROW",
    "CEILING",
    "CEILING.MATH",
    "CEILING.PRECISE",
    "CELL",
    "CHAR",
    "CHISQ.TEST",
    "CHITEST",
    "CHOOSE",
    "CHOOSECOLS",
    "CHOOSEROWS",
    "CODE",
    "COLUMN",
    "COLUMNS",
    "CONCAT",
    "CONCATENATE",
    "CONFIDENCE",
    "CONFIDENCE.NORM",
    "CONFIDENCE.T",
    "CONVERT",
    "COS",
    "COT",
    "COTH",
    "COUNT",
    "COUNTA",
    "COUNTBLANK",
    "COUNTIF",
    "COUNTIFS",
    "COUPDAYBS",
    "COUPDAYS",
    "COUPDAYSNC",
    "COUPNCD",
    "COUPNUM",
    "COUPPCD",
    "COVAR",
    "CSC",
    "CSCH",
    "DATE",
    "DAVERAGE",
    "DAY",
    "DAYS",
    "DBCS",
    "DCOUNT",
    "DCOUNTA",
    "DDB",
    "DGET",
    "DISC",
    "DMAX",
    "DMIN",
    "DPRODUCT",
    "DROP",
    "DSTDEV",
    "DSTDEVP",
    "DSUM",
    "DURATION",
    "DVAR",
    "DVARP",
    "EDATE",
    "ENCODEURL",
    "EUROCONVERT",
    "EXACT",
    "EXP",
    "EXPAND",
    "EXPON.DIST",
    "EXPONDIST",
    "F.DIST",
    "F.DIST.RT",
    "F.INV",
    "F.INV.RT",
    "F.TEST",
    "FDIST",
    "FILTER",
    "FILTERXML",
    "FIND",
    "FINDB",
    "FINV",
    "FLOOR",
    "FLOOR.MATH",
    "FLOOR.PRECISE",
    "FORECAST.ETS",
    "FORECAST.ETS.CONFINT",
    "FORECAST.ETS.SEASONALITY",
    "FORECAST.ETS.STAT",
    "FORMULATEXT",
    "FREQUENCY",
    "FTEST",
    "FV",
    "GETPIVOTDATA",
    "GROUPBY",
    "GROWTH",
    "HLOOKUP",
    "HOUR",
    "HSTACK",
    "HYPERLINK",
    "HYPGEOM.DIST",
    "HYPGEOMDIST",
    "IF",
    "IFERROR",
    "IFNA",
    "IFS",
    "IMCOSH",
    "IMCOT",
    "IMCOTH",
    "IMCSC",
    "IMCSCH",
    "IMLOG",
    "IMSEC",
    "IMSECH",
    "IMSINH",
    "IMTAN",
    "IMTANH",
    "INDEX",
    "INFO",
    "INT",
    "IPMT",
    "IRR",
    "ISBLANK",
    "ISERR",
    "ISERROR",
    "ISEVEN",
    "ISFORMULA",
    "ISLOGICAL",
    "ISNA",
    "ISNONTEXT",
    "ISNUMBER",
    "ISO.CEILING",
    "ISODD",
    "ISOMITTED",
    "ISPMT",
    "ISREF",
    "ISTEXT",
    "JIS",
    "LAMBDA",
    "LARGE",
    "LEFT",
    "LEFTB",
    "LEN",
    "LENB",
    "LET",
    "LINEST",
    "LN",
    "LOG",
    "LOG10",
    "LOGEST",
    "LOGINV",
    "LOGNORM.DIST",
    "LOGNORM.INV",
    "LOGNORMDIST",
    "LOOKUP",
    "LOWER",
    "MAKEARRAY",
    "MAP",
    "MARGINOFERROR",
    "MATCH",
    "MAX",
    "MAXA",
    "MDETERM",
    "MDURATION",
    "MEDIAN",
    "MID",
    "MIDB",
    "MIN",
    "MINA",
    "MINUTE",
    "MINVERSE",
    "MMULT",
    "MOD",
    "MODE",
    "MODE.MULT",
    "MONTH",
    "MUNIT",
    "NEGBINOM.DIST",
    "NEGBINOMDIST",
    "NOT",
    "NPER",
    "NPV",
    "ODDFPRICE",
    "ODDFYIELD",
    "ODDLPRICE",
    "ODDLYIELD",
    "OFFSET",
    "OR",
    "PERCENTILE",
    "PERCENTOF",
    "PHONETIC",
    "PI",
    "PIVOTBY",
    "PMT",
    "POISSON",
    "POISSON.DIST",
    "POWER",
    "PPMT",
    "PRICE",
    "PRICEDISC",
    "PRICEMAT",
    "PROB",
    "PRODUCT",
    "PROPER",
    "PV",
    "QUARTILE",
    "QUOTIENT",
    "RAND",
    "RANDARRAY",
    "RANK",
    "RATE",
    "RECEIVED",
    "REDUCE",
    "REGEXEXTRACT",
    "REGEXREPLACE",
    "REGEXTEST",
    "REPLACE",
    "REPLACEB",
    "REPT",
    "RIGHT",
    "RIGHTB",
    "ROUND",
    "ROUNDBANK",
    "ROUNDDOWN",
    "ROUNDUP",
    "ROW",
    "ROWS",
    "SCAN",
    "SEARCH",
    "SEARCHB",
    "SEC",
    "SECH",
    "SECOND",
    "SEQUENCE",
    "SHEET",
    "SHEETS",
    "SIGN",
    "SIN",
    "SLN",
    "SMALL",
    "SORT",
    "SORTBY",
    "SQRT",
    "STDEV",
    "STDEVA",
    "STDEVP",
    "STDEVPA",
    "SUBSTITUTE",
    "SUM",
    "SUMIF",
    "SUMIFS",
    "SUMPRODUCT",
    "SWITCH",
    "T.TEST",
    "TAKE",
    "TAN",
    "TBILLEQ",
    "TBILLPRICE",
    "TBILLYIELD",
    "TEXT",
    "TEXTJOIN",
    "TEXTSPLIT",
    "TIME",
    "TOCOL",
    "TOROW",
    "TRANSPOSE",
    "TREND",
    "TRIM",
    "TRIMRANGE",
    "TRUNC",
    "TTEST",
    "UNIQUE",
    "UPPER",
    "VALUE",
    "VAR",
    "VARA",
    "VARP",
    "VARPA",
    "VDB",
    "VLOOKUP",
    "VSTACK",
    "WEIBULL",
    "WEIBULL.DIST",
    "WRAPCOLS",
    "WRAPROWS",
    "XLOOKUP",
    "XOR",
    "YEAR",
    "YIELD",
    "YIELDDISC",
    "YIELDMAT",
];

#[inline]
pub(crate) fn is_builtin_eta_function(upper: &str) -> bool {
    BUILTIN_ETA_FUNCTIONS.binary_search(&upper).is_ok()
}

#[inline]
pub(crate) fn has_ascii_prefix_ci(word: &str, prefix: &str) -> bool {
    word.len() >= prefix.len()
        && word.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

#[inline]
pub(crate) fn strip_file_prefixes(mut word: &str) -> &str {
    loop {
        let Some(prefix) = SINGLE_PREFIXES_CI
            .iter()
            .find(|&&p| word.len() > p.len() && has_ascii_prefix_ci(word, p))
        else {
            return word;
        };
        word = &word[prefix.len()..];
    }
}

pub(crate) fn prev_significant(tokens: &[Tok], i: usize) -> Option<&Tok> {
    tokens[..i]
        .iter()
        .rev()
        .find(|t| !matches!(t, Tok::Punct(c) if c.is_whitespace()))
}

pub(crate) fn next_significant(tokens: &[Tok], i: usize) -> Option<&Tok> {
    tokens
        .get(i + 1..)?
        .iter()
        .find(|t| !matches!(t, Tok::Punct(c) if c.is_whitespace()))
}

/// Returns a per-token mask indicating whether the token sits inside an explicit eta-reduced
/// lambda argument slot (propagating through nested `HSTACK`/`VSTACK` and parenthesized expressions,
/// and reset by any other function call such as `LAMBDA` or `LET`).
pub(crate) fn eta_token_mask(tokens: &[Tok]) -> Vec<bool> {
    struct OpenParen {
        open_idx: usize,
        arg_count: usize,
        saw_content: bool,
        brace_depth: usize,
    }
    let mut args_total = vec![0usize; tokens.len()];
    let mut opens: Vec<OpenParen> = Vec::new();
    let mut brace_depth = 0usize;
    for (i, tok) in tokens.iter().enumerate() {
        match tok {
            Tok::Punct(c) if c.is_whitespace() => {}
            Tok::Punct('{') => {
                if let Some(top) = opens.last_mut() {
                    top.saw_content = true;
                }
                brace_depth += 1;
            }
            Tok::Punct('}') => brace_depth = brace_depth.saturating_sub(1),
            Tok::Punct('(') => {
                if let Some(top) = opens.last_mut() {
                    top.saw_content = true;
                }
                opens.push(OpenParen {
                    open_idx: i,
                    arg_count: 1,
                    saw_content: false,
                    brace_depth,
                });
            }
            Tok::Punct(')') => {
                if let Some(frame) = opens.pop() {
                    args_total[frame.open_idx] = if frame.saw_content {
                        frame.arg_count
                    } else {
                        0
                    };
                }
            }
            Tok::Punct(',') => {
                if let Some(top) = opens.last_mut() {
                    top.saw_content = true;
                    if brace_depth == top.brace_depth {
                        top.arg_count += 1;
                    }
                }
            }
            _ => {
                if let Some(top) = opens.last_mut() {
                    top.saw_content = true;
                }
            }
        }
    }

    struct Frame {
        name: Option<String>,
        arg_idx: usize,
        total_args: usize,
        parent_eta: bool,
        brace_depth: usize,
    }
    impl Frame {
        fn is_eta(&self, cur_brace_depth: usize) -> bool {
            if cur_brace_depth != self.brace_depth {
                return false;
            }
            match self.name.as_deref() {
                None | Some("HSTACK" | "VSTACK") => self.parent_eta,
                Some("GROUPBY" | "MAKEARRAY") => self.arg_idx == 2,
                Some("PIVOTBY") => self.arg_idx == 3,
                Some("BYROW" | "BYCOL") => self.arg_idx == 1,
                Some("REDUCE" | "SCAN") => {
                    (2..=3).contains(&self.total_args) && self.arg_idx + 1 == self.total_args
                }
                Some("MAP") => self.total_args >= 2 && self.arg_idx + 1 == self.total_args,
                Some(_) => false,
            }
        }
    }

    let mut mask = vec![false; tokens.len()];
    let mut frames: Vec<Frame> = Vec::new();
    let mut brace_depth = 0usize;
    for (i, tok) in tokens.iter().enumerate() {
        let active = frames.last().is_some_and(|frame| frame.is_eta(brace_depth));
        mask[i] = active;
        match tok {
            Tok::Punct('{') => brace_depth += 1,
            Tok::Punct('}') => brace_depth = brace_depth.saturating_sub(1),
            Tok::Punct('(') => {
                let name = match prev_significant(tokens, i) {
                    Some(Tok::Word(w)) => Some(strip_file_prefixes(w).to_ascii_uppercase()),
                    _ => None,
                };
                frames.push(Frame {
                    name,
                    arg_idx: 0,
                    total_args: args_total[i],
                    parent_eta: active,
                    brace_depth,
                });
            }
            Tok::Punct(')') => {
                frames.pop();
            }
            Tok::Punct(',') => {
                if let Some(top) = frames.last_mut() {
                    if brace_depth == top.brace_depth {
                        top.arg_idx += 1;
                    }
                }
            }
            _ => {}
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_eta_functions_are_strictly_sorted_and_unique() {
        for pair in BUILTIN_ETA_FUNCTIONS.windows(2) {
            assert!(
                pair[0] < pair[1],
                "out of order: {} vs {}",
                pair[0],
                pair[1]
            );
        }
        assert!(!is_builtin_eta_function("TRUE"));
        assert!(!is_builtin_eta_function("FALSE"));
        assert!(is_builtin_eta_function("PERCENTOF"));
        assert!(is_builtin_eta_function("SUM"));
    }
}

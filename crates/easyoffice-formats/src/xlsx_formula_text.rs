//! The formula text of a file and of the page (PD-217). A file names the functions that came after
//! the first spreadsheet version with a prefix (`_xlfn.`, and `_xlws.` for the two that exist only
//! in worksheets), the variables of `LET` and `LAMBDA` with `_xlpm.`, and the spill range
//! operator as a call (`_xlfn.ANCHORARRAY(A1)`). The page writes `FILTER(...)`, `x`, `A1#`.
//!
//! Both directions scan the text; neither parses it, and quoted text and quoted sheet names pass
//! untouched.

/// Functions a file spells `_xlfn._xlws.NAME`.
const WORKSHEET_ONLY: [&str; 2] = ["FILTER", "SORT"];

/// Functions a file spells `_xlfn.NAME`: every function added after the first release of the file
/// format, as [MS-XLSX] 2.2.2.1 lists the "future functions" a formula names with that prefix (the
/// statistical and distribution names with a dot, the 2013 text and logic functions, the dynamic
/// array family). Sorted, upper case; a function the engine does not have is listed too, so a formula
/// that names it is spelled the way the file format wants. Functions of this engine with no entry
/// (`AVERAGE.WEIGHTED`, `MARGINOFERROR`) are its own and are written as they are.
const FUTURE: [&str; 154] = [
    "ACOT",
    "ACOTH",
    "AGGREGATE",
    "ANCHORARRAY",
    "ARABIC",
    "ARRAYTOTEXT",
    "BASE",
    "BETA.DIST",
    "BETA.INV",
    "BINOM.DIST",
    "BINOM.DIST.RANGE",
    "BINOM.INV",
    "BITAND",
    "BITLSHIFT",
    "BITOR",
    "BITRSHIFT",
    "BITXOR",
    "BYCOL",
    "BYROW",
    "CEILING.MATH",
    "CEILING.PRECISE",
    "CHISQ.DIST",
    "CHISQ.DIST.RT",
    "CHISQ.INV",
    "CHISQ.INV.RT",
    "CHISQ.TEST",
    "CHOOSECOLS",
    "CHOOSEROWS",
    "COMBINA",
    "CONCAT",
    "CONFIDENCE.NORM",
    "CONFIDENCE.T",
    "COT",
    "COTH",
    "COVARIANCE.P",
    "COVARIANCE.S",
    "CSC",
    "CSCH",
    "DAYS",
    "DECIMAL",
    "DROP",
    "ERF.PRECISE",
    "ERFC.PRECISE",
    "EXPAND",
    "EXPON.DIST",
    "F.DIST",
    "F.DIST.RT",
    "F.INV",
    "F.INV.RT",
    "F.TEST",
    "FILTERXML",
    "FLOOR.MATH",
    "FLOOR.PRECISE",
    "FORECAST.ETS",
    "FORECAST.ETS.CONFINT",
    "FORECAST.ETS.SEASONALITY",
    "FORECAST.ETS.STAT",
    "FORECAST.LINEAR",
    "FORMULATEXT",
    "GAMMA",
    "GAMMA.DIST",
    "GAMMA.INV",
    "GAMMALN.PRECISE",
    "GAUSS",
    "HSTACK",
    "HYPGEOM.DIST",
    "IFNA",
    "IFS",
    "IMCOSH",
    "IMCOT",
    "IMCSC",
    "IMCSCH",
    "IMSEC",
    "IMSECH",
    "IMSINH",
    "IMTAN",
    "ISFORMULA",
    "ISOMITTED",
    "ISOWEEKNUM",
    "LAMBDA",
    "LET",
    "LOGNORM.DIST",
    "LOGNORM.INV",
    "MAKEARRAY",
    "MAP",
    "MAXIFS",
    "MINIFS",
    "MODE.MULT",
    "MODE.SNGL",
    "MUNIT",
    "NEGBINOM.DIST",
    "NETWORKDAYS.INTL",
    "NORM.DIST",
    "NORM.INV",
    "NORM.S.DIST",
    "NORM.S.INV",
    "NUMBERVALUE",
    "PDURATION",
    "PERCENTILE.EXC",
    "PERCENTILE.INC",
    "PERCENTRANK.EXC",
    "PERCENTRANK.INC",
    "PERMUTATIONA",
    "PHI",
    "POISSON.DIST",
    "QUARTILE.EXC",
    "QUARTILE.INC",
    "QUERYSTRING",
    "RANDARRAY",
    "RANK.AVG",
    "RANK.EQ",
    "REDUCE",
    "RRI",
    "SCAN",
    "SEC",
    "SECH",
    "SEQUENCE",
    "SHEET",
    "SHEETS",
    "SINGLE",
    "SKEW.P",
    "SORTBY",
    "STDEV.P",
    "STDEV.S",
    "SWITCH",
    "T.DIST",
    "T.DIST.2T",
    "T.DIST.RT",
    "T.INV",
    "T.INV.2T",
    "T.TEST",
    "TAKE",
    "TEXTAFTER",
    "TEXTBEFORE",
    "TEXTJOIN",
    "TEXTSPLIT",
    "TOCOL",
    "TOROW",
    "UNICHAR",
    "UNICODE",
    "UNIQUE",
    "VALUETOTEXT",
    "VAR.P",
    "VAR.S",
    "VSTACK",
    "WEBSERVICE",
    "WEIBULL.DIST",
    "WORKDAY.INTL",
    "WRAPCOLS",
    "WRAPROWS",
    "XLOOKUP",
    "XMATCH",
    "XOR",
    "Z.TEST",
];

const FILE_PREFIXES: [&str; 3] = ["_xlfn._xlws.", "_xlfn.", "_xlpm."];

#[derive(Clone, PartialEq, Debug)]
enum Tok {
    /// A word: a function, a name, a cell address, a number.
    Word(String),
    /// Quoted text or a quoted sheet name, with its quotes.
    Quoted(String),
    Punct(char),
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '$')
}

fn tokenize(text: &str) -> Vec<Tok> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == c {
                    if chars.get(j + 1) == Some(&c) {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            out.push(Tok::Quoted(chars[i..end].iter().collect()));
            i = end;
        } else if is_word_char(c) {
            let mut j = i;
            while j < chars.len() && is_word_char(chars[j]) {
                j += 1;
            }
            out.push(Tok::Word(chars[i..j].iter().collect()));
            i = j;
        } else {
            out.push(Tok::Punct(c));
            i += 1;
        }
    }
    out
}

fn join(tokens: &[Tok]) -> String {
    let mut out = String::new();
    for token in tokens {
        match token {
            Tok::Word(s) | Tok::Quoted(s) => out.push_str(s),
            Tok::Punct(c) => out.push(*c),
        }
    }
    out
}

/// The formula text of the page (no leading `=`) as a file spells it.
pub fn to_file_formula(text: &str) -> String {
    if !text.chars().any(|c| c.is_ascii_alphabetic() || c == '#') {
        return text.to_string();
    }
    let tokens = optional_parameters(anchor_calls(tokenize(text)));
    let variables = declared_variables(&tokens);
    let mut out = String::with_capacity(text.len() + 16);
    for (i, token) in tokens.iter().enumerate() {
        let Tok::Word(word) = token else {
            out.push_str(&join(std::slice::from_ref(token)));
            continue;
        };
        let upper = word.to_ascii_uppercase();
        let called = matches!(tokens.get(i + 1), Some(Tok::Punct('(')));
        let qualified = matches!(tokens.get(i + 1), Some(Tok::Punct('!')))
            || (i > 0 && tokens[i - 1] == Tok::Punct('!'));
        if word.starts_with("_xlfn.") {
            out.push_str(word);
        } else if called && WORKSHEET_ONLY.contains(&upper.as_str()) {
            out.push_str("_xlfn._xlws.");
            out.push_str(&upper);
        } else if called
            && FUTURE.binary_search(&upper.as_str()).is_ok()
            && !variables.contains(&upper)
        {
            out.push_str("_xlfn.");
            out.push_str(&upper);
        } else if !qualified && variables.contains(&upper) {
            out.push_str("_xlpm.");
            out.push_str(word);
        } else {
            out.push_str(word);
        }
    }
    out
}

/// The formula text of a file (no leading `=`) as the page spells it.
pub fn from_file_formula(text: &str) -> String {
    if !text.contains("_xl") && !text.contains("ANCHORARRAY") {
        return text.to_string();
    }
    let mut tokens: Vec<Tok> = Vec::new();
    for token in tokenize(text) {
        let Tok::Word(word) = token else {
            tokens.push(token);
            continue;
        };
        if let Some(rest) = word.strip_prefix("_xlop.") {
            // An optional `LAMBDA` parameter is written `[x]` on the page.
            tokens.extend([
                Tok::Punct('['),
                Tok::Word(rest.to_string()),
                Tok::Punct(']'),
            ]);
            continue;
        }
        let plain = FILE_PREFIXES
            .iter()
            .find_map(|prefix| word.strip_prefix(prefix))
            .map_or(word.clone(), str::to_string);
        tokens.push(Tok::Word(plain));
    }
    join(&spill_operators(tokens))
}

/// `[x]` in an argument list as the word `_xlop.x`.
fn optional_parameters(tokens: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let before_ok = matches!(out.last(), Some(Tok::Punct('(' | ',')))
            || out
                .iter()
                .rev()
                .find(|t| !matches!(t, Tok::Punct(c) if c.is_whitespace()))
                .is_some_and(|t| matches!(t, Tok::Punct('(' | ',')));
        if let (true, Tok::Punct('['), Some(Tok::Word(w)), Some(Tok::Punct(']'))) =
            (before_ok, &tokens[i], tokens.get(i + 1), tokens.get(i + 2))
        {
            if !w.contains('.') {
                out.push(Tok::Word(format!("_xlop.{w}")));
                i += 3;
                continue;
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

/// `A1#` as `_xlfn.ANCHORARRAY(A1)`.
fn anchor_calls(tokens: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if token == Tok::Punct('#') && matches!(out.last(), Some(Tok::Word(_))) {
            let mut start = out.len() - 1;
            if start >= 2 && out[start - 1] == Tok::Punct('!') {
                start -= 2;
            }
            let reference: Vec<Tok> = out.split_off(start);
            out.push(Tok::Word("_xlfn.ANCHORARRAY".to_string()));
            out.push(Tok::Punct('('));
            out.extend(reference);
            out.push(Tok::Punct(')'));
        } else {
            out.push(token);
        }
    }
    out
}

/// `ANCHORARRAY(A1)` as `A1#`.
fn spill_operators(tokens: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let is_call = matches!(&tokens[i], Tok::Word(w) if w.eq_ignore_ascii_case("ANCHORARRAY"))
            && tokens.get(i + 1) == Some(&Tok::Punct('('));
        if is_call {
            if let Some(close) = tokens[i + 2..].iter().position(|t| *t == Tok::Punct(')')) {
                let inner = &tokens[i + 2..i + 2 + close];
                if !inner.contains(&Tok::Punct('(')) {
                    out.extend(inner.iter().cloned());
                    out.push(Tok::Punct('#'));
                    i += close + 3;
                    continue;
                }
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

/// The variables of every `LET` (the names before a value) and `LAMBDA` (its parameters), upper case.
fn declared_variables(tokens: &[Tok]) -> std::collections::HashSet<String> {
    struct Call {
        name: String,
        depth: usize,
        arg: usize,
    }
    let mut vars = std::collections::HashSet::new();
    let mut calls: Vec<Call> = Vec::new();
    let mut depth = 0usize;
    let mut arg_start = true;
    let next_significant = |from: usize| {
        tokens[from..]
            .iter()
            .find(|t| !matches!(t, Tok::Punct(c) if c.is_whitespace()))
    };
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Tok::Punct(c) if c.is_whitespace() => {}
            Tok::Punct('(') => {
                if let Some(Tok::Word(name)) = i.checked_sub(1).map(|j| &tokens[j]) {
                    calls.push(Call {
                        name: name.to_ascii_uppercase().replace("_XLFN.", ""),
                        depth: depth + 1,
                        arg: 0,
                    });
                }
                depth += 1;
                arg_start = true;
            }
            Tok::Punct(')') => {
                if calls.last().is_some_and(|c| c.depth == depth) {
                    calls.pop();
                }
                depth = depth.saturating_sub(1);
                arg_start = false;
            }
            Tok::Punct(',') => {
                if let Some(call) = calls.last_mut().filter(|c| c.depth == depth) {
                    call.arg += 1;
                }
                arg_start = true;
            }
            Tok::Word(word) => {
                let follows = matches!(next_significant(i + 1), Some(Tok::Punct(',' | ')')));
                if let (true, true, Some(call)) = (arg_start, follows, calls.last()) {
                    let declares = match call.name.as_str() {
                        "LET" => call.arg % 2 == 0,
                        "LAMBDA" => true,
                        _ => false,
                    };
                    if declares
                        && call.depth == depth
                        && !word.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        vars.insert(
                            word.trim_start_matches("_xlpm.")
                                .trim_start_matches("_xlop.")
                                .to_ascii_uppercase(),
                        );
                    }
                }
                arg_start = false;
            }
            _ => arg_start = false,
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_variables_and_the_spill_operator_get_their_file_spelling() {
        assert_eq!(
            to_file_formula("FILTER(A1:B9, B1:B9>3)"),
            "_xlfn._xlws.FILTER(A1:B9, B1:B9>3)"
        );
        assert_eq!(
            to_file_formula("SORT(UNIQUE(A1:A9))"),
            "_xlfn._xlws.SORT(_xlfn.UNIQUE(A1:A9))"
        );
        assert_eq!(to_file_formula("SUM(A1#)"), "SUM(_xlfn.ANCHORARRAY(A1))");
        assert_eq!(
            to_file_formula("SUM('My Sheet'!B2#)"),
            "SUM(_xlfn.ANCHORARRAY('My Sheet'!B2))"
        );
        assert_eq!(
            to_file_formula("LET(x, 2, y, x+1, y*x)"),
            "_xlfn.LET(_xlpm.x, 2, _xlpm.y, _xlpm.x+1, _xlpm.y*_xlpm.x)"
        );
        assert_eq!(
            to_file_formula("MAP(A1:A3, LAMBDA(n, n*2))"),
            "_xlfn.MAP(A1:A3, _xlfn.LAMBDA(_xlpm.n, _xlpm.n*2))"
        );
        assert_eq!(
            to_file_formula("A1&\"FILTER(\"&#N/A"),
            "A1&\"FILTER(\"&#N/A"
        );
        assert_eq!(to_file_formula("SUM(A1:A3)"), "SUM(A1:A3)");
    }

    #[test]
    fn the_table_of_future_functions_is_sorted_and_every_name_round_trips() {
        assert!(
            FUTURE.windows(2).all(|w| w[0] < w[1]),
            "sorted, no duplicates"
        );
        for name in FUTURE
            .iter()
            .filter(|n| !matches!(**n, "ANCHORARRAY" | "LAMBDA" | "LET"))
        {
            let page = format!("{name}(A1, 2)");
            let file = to_file_formula(&page);
            assert!(
                file == format!("_xlfn.{name}(A1, 2)")
                    || file == format!("_xlfn._xlws.{name}(A1, 2)"),
                "{file}"
            );
            assert_eq!(from_file_formula(&file), page);
        }
        // a name that is no future function, a name inside text, and a legacy name stay as they are
        for text in [
            "SUM(A1)",
            "VLOOKUP(A1, B:C, 2, 0)",
            "\"IFS(\"&A1",
            "STDEV(A1:A3)",
            "FDIST(1,2,3)",
        ] {
            assert_eq!(to_file_formula(text), text);
        }
        assert_eq!(
            to_file_formula("IFS(A1>1, \"a\", TRUE, \"b\")"),
            "_xlfn.IFS(A1>1, \"a\", TRUE, \"b\")"
        );
        assert_eq!(
            to_file_formula("STDEV.S(A1:A3)+IFNA(B1, 0)"),
            "_xlfn.STDEV.S(A1:A3)+_xlfn.IFNA(B1, 0)"
        );
    }

    #[test]
    fn optional_lambda_parameters_have_their_own_spelling() {
        let page = "LAMBDA(a, [b], IF(ISOMITTED(b), a, a*b))(1)";
        let file = to_file_formula(page);
        assert_eq!(
            file,
            "_xlfn.LAMBDA(_xlpm.a, _xlop.b, IF(_xlfn.ISOMITTED(_xlpm.b), _xlpm.a, _xlpm.a*_xlpm.b))(1)"
        );
        assert_eq!(from_file_formula(&file), page);
    }

    #[test]
    fn the_file_spelling_reads_back_as_the_page_spelling() {
        for text in [
            "FILTER(A1:B9, B1:B9>3)",
            "SUM(A1#)",
            "SUM('My Sheet'!B2#)",
            "LET(x, 2, y, x+1, y*x)",
            "MAP(A1:A3, LAMBDA(n, n*2))",
            "XLOOKUP(\"a\", A:A, B:C)",
            "#N/A",
        ] {
            assert_eq!(from_file_formula(&to_file_formula(text)), text, "{text}");
        }
        assert_eq!(from_file_formula("_xlfn.ANCHORARRAY(B2)"), "B2#");
    }
}

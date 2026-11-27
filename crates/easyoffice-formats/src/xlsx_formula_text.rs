//! The formula text of a file and of the page (PD-217). A file names the functions that came after
//! the first spreadsheet version with a prefix (`_xlfn.`, and `_xlws.` for the two that exist only
//! in worksheets), the variables of `LET` and `LAMBDA` with `_xlpm.`, and the spill range
//! operator as a call (`_xlfn.ANCHORARRAY(A1)`). The page writes `FILTER(...)`, `x`, `A1#`.
//!
//! Both directions scan the text; neither parses it, and quoted text and quoted sheet names pass
//! untouched.

/// Functions a file spells `_xlfn._xlws.NAME` ([MS-XLSX] 2.2.2 `worksheet-only-function-list`).
const WORKSHEET_ONLY: [&str; 3] = ["FILTER", "PY", "SORT"];

/// Functions a file spells `_xlfn.NAME`: every function added after the first release of the file
/// format. The names are the "future functions" of [MS-XLSX] 2.2.2 (the statistical and
/// distribution names with a dot, the 2013 and later text and logic functions, the dynamic array
/// family) plus the ones that rule leaves out or lists outside its prefix group but that files
/// carry with the prefix (`IFS`, `SWITCH`, `TEXTJOIN`, `CONCAT`, `MAXIFS`, `MINIFS`, `XMATCH`,
/// `IMAGE`, the forecast family, `ANCHORARRAY`, `SINGLE`, `ARRAYTOTEXT`, `VALUETOTEXT`, the 2024 and later
/// additions `REGEXTEST`, `REGEXEXTRACT`, `REGEXREPLACE`, `TRIMRANGE`, `PERCENTOF`, `GROUPBY`, `PIVOTBY`).
/// `tests/fixtures/xlsx_future_functions.txt` holds the list; a test checks this table against it.
/// `NETWORKDAYS.INTL`, `WORKDAY.INTL`, `ECMA.CEILING` and `ISO.CEILING` are written plain. Sorted,
/// upper case; a function the engine does not have is listed too, so a formula that names it is
/// spelled the way the file format wants. Functions of this engine with no entry
/// (`AVERAGE.WEIGHTED`, `MARGINOFERROR`) are its own and are written as they are.
const FUTURE: [&str; 168] = [
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
    "COPILOT",
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
    "FIELDVALUE",
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
    "GROUPBY",
    "HSTACK",
    "HYPGEOM.DIST",
    "IFNA",
    "IFS",
    "IMAGE",
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
    "LONGTEXT",
    "MAKEARRAY",
    "MAP",
    "MAXIFS",
    "MINIFS",
    "MODE.MULT",
    "MODE.SNGL",
    "MUNIT",
    "NEGBINOM.DIST",
    "NORM.DIST",
    "NORM.INV",
    "NORM.S.DIST",
    "NORM.S.INV",
    "NUMBERVALUE",
    "PDURATION",
    "PERCENTILE.EXC",
    "PERCENTILE.INC",
    "PERCENTOF",
    "PERCENTRANK.EXC",
    "PERCENTRANK.INC",
    "PERMUTATIONA",
    "PHI",
    "PIVOTBY",
    "POISSON.DIST",
    "PQSOURCE",
    "PYTHON_STR",
    "PYTHON_TYPE",
    "PYTHON_TYPENAME",
    "QUARTILE.EXC",
    "QUARTILE.INC",
    "QUERYSTRING",
    "RANDARRAY",
    "RANK.AVG",
    "RANK.EQ",
    "REDUCE",
    "REGEXEXTRACT",
    "REGEXREPLACE",
    "REGEXTEST",
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
    "STOCKHISTORY",
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
    "TRIMRANGE",
    "UNICHAR",
    "UNICODE",
    "UNIQUE",
    "VALUETOTEXT",
    "VAR.P",
    "VAR.S",
    "VSTACK",
    "WEBSERVICE",
    "WEIBULL.DIST",
    "WRAPCOLS",
    "WRAPROWS",
    "XLOOKUP",
    "XMATCH",
    "XOR",
    "Z.TEST",
];

use super::xlsx_single_operator::{single_calls, single_operators};

use super::xlsx_eta_funcs::{
    eta_token_mask, has_ascii_prefix_ci, is_builtin_eta_function, next_significant,
    prev_significant, strip_file_prefixes,
};

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Tok {
    /// A word: a function, a name, a cell address, a number.
    Word(String),
    /// Quoted text or a quoted sheet name, with its quotes.
    Quoted(String),
    Punct(char),
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '$')
}

#[inline]
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')
}

fn tokenize(text: &str) -> Vec<Tok> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'[' {
            let named = i > 0 && text[..i].chars().last().is_some_and(is_word_char);
            let opens = bytes[i + 1..]
                .iter()
                .find(|&&c| c != b' ')
                .is_some_and(|&c| matches!(c, b'@' | b'#' | b'['));
            if named || opens {
                if let Some(end) = crate::structured_ref::group_end(text, i) {
                    out.push(Tok::Quoted(text[i..end].to_string()));
                    i = end;
                    continue;
                }
            }
        }
        if b == b'"' || b == b'\'' {
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b {
                    if j + 1 < bytes.len() && bytes[j + 1] == b {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(bytes.len());
            out.push(Tok::Quoted(text[i..end].to_string()));
            i = end;
        } else if b.is_ascii() {
            if is_word_byte(b) {
                let mut j = i + 1;
                while j < bytes.len() && is_word_byte(bytes[j]) {
                    j += 1;
                }
                if j < bytes.len() && !bytes[j].is_ascii() {
                    let mut char_bytes = 0;
                    for ch in text[j..].chars() {
                        if is_word_char(ch) {
                            char_bytes += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                    j += char_bytes;
                }
                out.push(Tok::Word(text[i..j].to_string()));
                i = j;
            } else {
                out.push(Tok::Punct(b as char));
                i += 1;
            }
        } else {
            let ch = text[i..].chars().next().unwrap();
            let ch_len = ch.len_utf8();
            if is_word_char(ch) {
                let mut j = i + ch_len;
                let mut char_bytes = 0;
                for c in text[j..].chars() {
                    if is_word_char(c) {
                        char_bytes += c.len_utf8();
                    } else {
                        break;
                    }
                }
                j += char_bytes;
                out.push(Tok::Word(text[i..j].to_string()));
                i = j;
            } else {
                out.push(Tok::Punct(ch));
                i += ch_len;
            }
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

/// The longest formula, in characters, the spreadsheet format allows (PD-363). A longer text is a
/// hostile or damaged file: its spelling is not converted, the text is kept as it is.
const MAX_FORMULA_CHARS: usize = 8192;

fn over_length_limit(text: &str) -> bool {
    // A character is at least one byte, so a short text needs no count.
    text.len() > MAX_FORMULA_CHARS && text.chars().count() > MAX_FORMULA_CHARS
}

/// The formula text of the page (no leading `=`) as a file spells it. A text over the length or
/// nesting limit comes back unchanged.
pub fn to_file_formula(text: &str) -> String {
    if over_length_limit(text)
        || !text
            .chars()
            .any(|c| c.is_ascii_alphabetic() || matches!(c, '#' | '@'))
    {
        return text.to_string();
    }
    let Some(singles) = single_calls(anchor_calls(tokenize(text))) else {
        return text.to_string();
    };
    let tokens = optional_parameters(singles);
    let variables = declared_variables(&tokens);
    let eta_mask = eta_token_mask(&tokens);
    let mut out = String::with_capacity(text.len() + 16);
    for (i, token) in tokens.iter().enumerate() {
        let Tok::Word(word) = token else {
            out.push_str(&join(std::slice::from_ref(token)));
            continue;
        };
        let upper = word.to_ascii_uppercase();
        let next_sig = next_significant(&tokens, i);
        let prev_sig = prev_significant(&tokens, i);
        let called = matches!(next_sig, Some(Tok::Punct('(')));
        let qualified = matches!(next_sig, Some(Tok::Punct('!' | ':')))
            || matches!(prev_sig, Some(Tok::Punct('!' | ':')))
            || matches!(next_sig, Some(Tok::Quoted(q)) if q.starts_with('['));
        if has_ascii_prefix_ci(word, "_xlfn.") || has_ascii_prefix_ci(word, "_xleta.") {
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
        } else if eta_mask[i]
            && !called
            && !qualified
            && !variables.contains(&upper)
            && is_builtin_eta_function(&upper)
        {
            out.push_str("_xleta.");
            out.push_str(&upper);
        } else {
            out.push_str(word);
        }
    }
    out
}

fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
    let nb = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(nb.len())
        .any(|w| w.eq_ignore_ascii_case(nb))
}

/// The formula text of a file (no leading `=`) as the page spells it. A text over the length or
/// nesting limit comes back unchanged.
pub fn from_file_formula(text: &str) -> String {
    if over_length_limit(text)
        || !contains_ascii_ci(text, "_xl")
            && !text.contains("ANCHORARRAY")
            && !contains_ascii_ci(text, "SINGLE")
    {
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
        tokens.push(Tok::Word(strip_file_prefixes(&word).to_string()));
    }
    match single_operators(spill_operators(tokens)) {
        Some(tokens) => join(&tokens),
        None => text.to_string(),
    }
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
                if let Some(Tok::Word(name)) = prev_significant(tokens, i) {
                    calls.push(Call {
                        name: strip_file_prefixes(name).to_ascii_uppercase(),
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
                // A variable is followed by its value (`LET`) or the next parameter (`LAMBDA`); the
                // last argument, which ends at `)`, is the body.
                let follows = matches!(next_significant(i + 1), Some(Tok::Punct(',')));
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
    fn the_last_argument_of_let_and_lambda_is_a_body_not_a_variable() {
        assert_eq!(to_file_formula("LET(x,1,B1)"), "_xlfn.LET(_xlpm.x,1,B1)");
        assert_eq!(
            to_file_formula("LAMBDA(x,TRUE)"),
            "_xlfn.LAMBDA(_xlpm.x,TRUE)"
        );
        assert_eq!(
            to_file_formula("LET(x,1,y,2,y)"),
            "_xlfn.LET(_xlpm.x,1,_xlpm.y,2,_xlpm.y)"
        );
        assert_eq!(
            to_file_formula("LAMBDA(x,y,y)"),
            "_xlfn.LAMBDA(_xlpm.x,_xlpm.y,_xlpm.y)"
        );
    }

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

    /// The names of one `[section]` of the list the tables are checked against.
    fn listed(section: &str) -> Vec<&'static str> {
        include_str!("../tests/fixtures/xlsx_future_functions.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .scan("", |current, line| {
                if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                    *current = name;
                    return Some(None);
                }
                Some((*current == section).then_some(line.trim()))
            })
            .flatten()
            .collect()
    }

    #[test]
    fn the_tables_hold_exactly_the_listed_names() {
        let mut future = [listed("spec"), listed("observed")].concat();
        future.sort_unstable();
        assert_eq!(future.len(), 168, "a name repeats between the sections");
        assert_eq!(FUTURE.to_vec(), future);
        let mut worksheet = listed("worksheet");
        worksheet.sort_unstable();
        assert_eq!(WORKSHEET_ONLY.to_vec(), worksheet);
        // the three kinds of spelling never overlap
        for name in listed("plain") {
            assert!(
                !FUTURE.contains(&name) && !WORKSHEET_ONLY.contains(&name),
                "{name}"
            );
            let page = format!("{name}(A1, 2)");
            assert_eq!(to_file_formula(&page), page);
            assert_eq!(from_file_formula(&page), page);
        }
        assert!(FUTURE.iter().all(|n| !WORKSHEET_ONLY.contains(n)));
    }

    #[test]
    fn the_table_of_future_functions_is_sorted_and_every_name_round_trips() {
        assert!(
            FUTURE.windows(2).all(|w| w[0] < w[1]),
            "sorted, no duplicates"
        );
        for name in FUTURE
            .iter()
            .filter(|n| !matches!(**n, "ANCHORARRAY" | "LAMBDA" | "LET" | "SINGLE"))
            .chain(WORKSHEET_ONLY.iter())
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

    #[test]
    fn implicit_intersection_operator_round_trips_as_single() {
        for (page, file) in [
            ("@A1:A5", "_xlfn.SINGLE(A1:A5)"),
            (
                "SUM(@A1:A5, @B1:B5)",
                "SUM(_xlfn.SINGLE(A1:A5), _xlfn.SINGLE(B1:B5))",
            ),
            ("@'My Sheet'!A1:A5", "_xlfn.SINGLE('My Sheet'!A1:A5)"),
            ("@A1#", "_xlfn.SINGLE(_xlfn.ANCHORARRAY(A1))"),
            ("@SEQUENCE(5)", "_xlfn.SINGLE(_xlfn.SEQUENCE(5))"),
            ("@Table1[Amount]", "_xlfn.SINGLE(Table1[Amount])"),
            ("@{10,20}", "_xlfn.SINGLE({10,20})"),
            ("@(A1+B1)", "_xlfn.SINGLE((A1+B1))"),
            ("(@B1:B4):D2", "(_xlfn.SINGLE(B1:B4)):D2"),
            (
                "SUM(@OFFSET(B1,0,0):B4)",
                "SUM(_xlfn.SINGLE(OFFSET(B1,0,0):B4))",
            ),
            ("@(B1:B2*3)", "_xlfn.SINGLE((B1:B2*3))"),
            ("SUM(Table1[@Amount])", "SUM(Table1[@Amount])"),
            ("\"@literal\"", "\"@literal\""),
        ] {
            assert_eq!(to_file_formula(page), file, "to_file {page}");
            assert_eq!(from_file_formula(file), page, "from_file {file}");
        }
        assert_eq!(from_file_formula("_xlfn.SINGLE(A1+B1)"), "@(A1+B1)");
        assert_eq!(from_file_formula("SINGLE(A1, 2)"), "SINGLE(A1, 2)");
    }
}

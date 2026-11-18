//! The number formatter against format-code test vectors that other spreadsheet libraries keep
//! (reference implementations, see `THIRD_PARTY_NOTICES.md`):
//!
//! - `tests/data/ssf/*`: the SSF test data (Apache-2.0, Copyright (C) 2012-present SheetJS LLC;
//!   `test/comma.tsv`, `exp.tsv`, `fraction.json`, `implied.json`, `oddities.json`, `is_date.json`
//!   and a sample of every 997th row of `dates.tsv` and every 4093rd of `times.tsv`). The
//!   expected texts were recorded from the spreadsheet program. The JSON files are flattened to
//!   `code TAB value TAB expected` rows (`n:` number, `s:` text; `\t`, `\n`, `\\` escaped).
//! - `tests/data/ssf/en_examples.tsv`: the English-locale cases of IronCalc `base/src/formatter/test`
//!   (MIT OR Apache-2.0, Copyright (c) IronCalc contributors), flattened the same way.
//!
//! A row this formatter shows differently on purpose is listed in [`KNOWN`] with the reason.

use easyoffice_sheet::{
    code_shows_date, format_cell_value, format_number_with_code, EvaluatedCellValue, SheetEngine,
};
use std::fs;

/// `(code, value, why)`: rows where the expected text is not what the sheet shows.
/// A `value` of `""` covers every value of the code.
const KNOWN: &[(&str, &str, &str)] = &[
    // The vectors' `General` is the cell display at the default column width (about 11
    // characters); this formatter's `General` is the full value as `TEXT` gives it.
    (
        "General",
        "",
        "General is the full-precision text, the cell width rule is the page's",
    ),
    ("@", "", "a text cell shows its stored text as typed"),
    // Behaviors of the reference that the program's documentation or output contradict.
    ("@@", "", "a number in a text section"),
    (
        "hh:mm:ss am/pm",
        "n:0.5",
        "lower-case am/pm keeps the case of the code (program help)",
    ),
    (
        "# ?/??",
        "",
        "the other formatter trims the placeholder spaces of an empty fraction",
    ),
    (
        "000.0",
        "n:-0.00001",
        "a negative value that rounds to zero keeps its sign",
    ),
    ("00-.-0?0", "", "digits after a literal"),
    ("0 \"\"\"Millions\"\"\"", "", "doubled quotes"),
    // Era, Buddhist-year and fractional-hour codes, locale month and weekday names.
    ("e", "", "era year"),
    ("b", "", "Buddhist year"),
    ("B2yyyymmdd", "", "Hijri calendar"),
    ("hh.000", "", "fraction of an hour"),
    ("hh .00", "", "fraction of an hour"),
    ("hh  .0", "", "fraction of an hour"),
    ("hh .00 .000", "", "fraction of an hour"),
    ("A\"TODO\"", "", "invalid code"),
    ("\\r", "", "invalid code"),
    (
        "0#######0.##0##0######00######0",
        "",
        "more than 10 fraction placeholders",
    ),
    (
        "[Red]General ;[Blue]General\\ ;[Green]Generalp;[Yellow]General'",
        "",
        "General in sections",
    ),
    (
        "# ??/?????????",
        "n:0.123251512342345",
        "the numerator takes its own width",
    ),
    (
        "# ?? / ?????????",
        "n:0.123251512342345",
        "the numerator takes its own width",
    ),
    (
        "_($* #,##0_);_($* (#,##0);_($* \"-\"_);_(@_)",
        "s:",
        "the text section of an empty text keeps its padding",
    ),
    ("General", "n:1234.56", "decimal comma locale"),
];

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn data(name: &str) -> String {
    fs::read_to_string(format!(
        "{}/tests/data/ssf/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn known(code: &str, value: &str) -> bool {
    KNOWN
        .iter()
        .any(|(c, v, _)| *c == code && (v.is_empty() || *v == value))
}

/// The display the cell shows: `None` (General) reads as the empty text the vectors give for a
/// number the code cannot show, or as the text itself.
fn shown(code: &str, value: &str) -> String {
    let (kind, text) = value.split_at(2);
    match format_cell_value(code, text) {
        Some(f) => f.text,
        None if kind == "n:" => String::new(),
        None => text.to_string(),
    }
}

fn check_rows(file: &str, min_rows: usize) {
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for line in data(file).lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let (code, value, expect) = (unescape(f[0]), unescape(f[1]), unescape(f[2]));
        if known(&code, &value) {
            continue;
        }
        checked += 1;
        let got = shown(&code, &value);
        if got != expect {
            failures.push(format!("{code:?} {value:?}: want {expect:?} got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{file}: {}", failures.join("\n"));
    assert!(checked >= min_rows, "{file}: only {checked} rows checked");
}

/// A matrix file: a header row of codes, then `value TAB text per code`; a text that starts
/// with `|` is not checked, `#` runs (255) stand for an empty text.
fn check_matrix(file: &str, min_cells: usize) {
    let text = data(file);
    let mut lines = text.lines();
    let headers: Vec<&str> = lines.next().expect("header").split('\t').collect();
    let (mut checked, mut failures) = (0usize, Vec::new());
    for line in lines.filter(|l| l.len() > 2) {
        let line = line.replace(&"#".repeat(255), "");
        let cells: Vec<&str> = line.split('\t').collect();
        let Ok(value) = cells[0].parse::<f64>() else {
            continue;
        };
        for (code, expect) in headers.iter().zip(&cells).skip(1) {
            if expect.starts_with('|') {
                continue;
            }
            checked += 1;
            let got = format_number_with_code(value, code).unwrap_or_default();
            if got != *expect {
                failures.push(format!("{code:?} {value}: want {expect:?} got {got:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{file}: {}", failures.join("\n"));
    assert!(checked >= min_cells, "{file}: only {checked} cells checked");
}

#[test]
fn fractions_follow_the_programs_search() {
    check_rows("fraction.tsv", 100);
}

#[test]
fn implied_builtin_ids_and_generals() {
    check_rows("implied.tsv", 500);
}

#[test]
fn oddities() {
    check_rows("oddities.tsv", 200);
}

#[test]
fn english_cases_of_the_other_formatter() {
    check_rows("en_examples.tsv", 120);
}

#[test]
fn thousands_scaling_and_grouping() {
    check_matrix("comma.tsv", 100);
}

#[test]
fn scientific_exponent_is_a_multiple_of_the_integer_placeholders() {
    check_matrix("exp.tsv", 170);
}

#[test]
fn date_codes() {
    check_matrix("dates.tsv", 3_000);
}

#[test]
fn time_codes() {
    check_matrix("times.tsv", 3_000);
}

#[test]
fn which_codes_show_a_date() {
    let mut failures = Vec::new();
    for line in data("is_date.tsv").lines().filter(|l| !l.is_empty()) {
        let (code, want) = line.rsplit_once('\t').expect("row");
        let code = unescape(code);
        if code_shows_date(&code, 1.5) != (want == "1") {
            failures.push(format!("{code:?} want {want}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The `TEXT` function and the cell display are one formatter: the same code and number give the
/// same text, and a date code on a serial it cannot show is `#VALUE!` in `TEXT` and General in a cell.
#[test]
fn the_text_function_gives_what_the_formatter_gives() {
    let engine = SheetEngine::new("Sheet1");
    let mut compared = 0;
    for line in data("implied.tsv").lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let (code, value) = (unescape(f[0]), unescape(f[1]));
        let Some(number) = value.strip_prefix("n:").and_then(|v| v.parse::<f64>().ok()) else {
            continue;
        };
        let formula = format!("=TEXT({number},\"{}\")", code.replace('"', "\"\""));
        match (
            engine.evaluate_formula_str(&formula),
            format_number_with_code(number, &code),
        ) {
            (EvaluatedCellValue::Text(text), Ok(want)) => assert_eq!(text, want, "{formula}"),
            (EvaluatedCellValue::Error(_), Err(_)) => {}
            (got, want) => panic!("{formula}: TEXT gave {got:?}, the formatter {want:?}"),
        }
        compared += 1;
    }
    assert!(compared >= 500, "{compared}");
}

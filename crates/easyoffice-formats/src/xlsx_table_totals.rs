//! The totals row of a table in SpreadsheetML (PD-232): the `totalsRowFunction`, `totalsRowLabel` and
//! `totalsRowFormula` of a `tableColumn` (ECMA-376 18.5.1.3, 18.18.80 `ST_TotalsRowFunction`).
//!
//! The file carries the totals row twice: as cells and as these column attributes, which the spreadsheet
//! program's function list shows. The writer derives them from the cells, so they never disagree with
//! the sheet: a `SUBTOTAL` over the column with a function number the list writes (101 to 105, 107, 109,
//! 110) is that function, any other formula is `custom` with its text, and a text constant is the label.
//! The reader keeps none of them, the cells hold the same content (a file whose attributes name a function
//! its cells do not hold shows the cells, as the spreadsheet program does).
//!
//! The function numbers are the list of the spreadsheet program's totals row (its support page on totaling
//! the data of a table shows `=SUBTOTAL(109,[Midwest])` as the sum); the 100s skip the rows a filter hides.

use std::fmt::Write;

use crate::sheet_table::MAX_TABLE_TEXT_CHARS;
use crate::XmlSanitizer;

/// Characters of a formula a cell takes (the spreadsheet program's limit).
pub(crate) const MAX_TOTALS_FORMULA_CHARS: usize = 8_192;

/// `SUBTOTAL` function number to `totalsRowFunction`.
const FUNCTIONS: [(u32, &str); 8] = [
    (101, "average"),
    (102, "countNums"),
    (103, "count"),
    (104, "max"),
    (105, "min"),
    (107, "stdDev"),
    (109, "sum"),
    (110, "var"),
];

/// What the totals-row cell of one column says of the column.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnTotals {
    function: Option<&'static str>,
    label: Option<String>,
    /// The formula of a `custom` column, without the `=`.
    formula: Option<String>,
}

/// A table column reference's quoting: `'` before `[`, `]`, `#` and `'`.
fn quoted(column: &str) -> String {
    let mut out = String::with_capacity(column.len());
    for c in column.chars() {
        if matches!(c, '\'' | '[' | ']' | '#') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

/// Whether `arg` names the data of `column` of `table`: `T[c]`, `T[[c]]`, `[c]` (the table's own column) or the
/// `[#Data]` form, without regard to case.
fn names_column(arg: &str, table: &str, column: &str) -> bool {
    let (t, c) = (table.to_lowercase(), quoted(column).to_lowercase());
    let arg = arg.trim().to_lowercase();
    [
        format!("{t}[{c}]"),
        format!("{t}[[{c}]]"),
        format!("[{c}]"),
        format!("[[{c}]]"),
        format!("{t}[[#data],[{c}]]"),
        format!("[[#data],[{c}]]"),
    ]
    .contains(&arg)
}

/// The `totalsRowFunction` of the formula `body` (no `=`) when it is a `SUBTOTAL` of the column's data.
fn subtotal_function(body: &str, table: &str, column: &str) -> Option<&'static str> {
    let body = body.trim();
    let head = body.get(..9)?;
    if !head.eq_ignore_ascii_case("SUBTOTAL(") || !body.ends_with(')') {
        return None;
    }
    let (code, arg) = body[9..body.len() - 1].split_once(',')?;
    let code: u32 = code.trim().parse().ok()?;
    let function = FUNCTIONS.iter().find(|(n, _)| *n == code)?.1;
    names_column(arg, table, column).then_some(function)
}

/// The totals-row cell `raw` (a formula with its `=`, or text) of the column `column` of `table`.
pub(crate) fn column_totals(raw: Option<&str>, table: &str, column: &str) -> ColumnTotals {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return ColumnTotals::default();
    };
    if let Some(body) = raw.strip_prefix('=') {
        if let Some(function) = subtotal_function(body, table, column) {
            return ColumnTotals {
                function: Some(function),
                ..ColumnTotals::default()
            };
        }
        // A formula over the limit is a cell the file keeps as it is, not a column the list can describe.
        return if body.chars().count() <= MAX_TOTALS_FORMULA_CHARS {
            ColumnTotals {
                function: Some("custom"),
                formula: Some(body.to_string()),
                ..ColumnTotals::default()
            }
        } else {
            ColumnTotals::default()
        };
    }
    ColumnTotals {
        label: (raw.chars().count() <= MAX_TABLE_TEXT_CHARS).then(|| raw.to_string()),
        ..ColumnTotals::default()
    }
}

/// The attributes of `tableColumn` that follow `name`.
pub(crate) fn write_attributes(out: &mut String, totals: &ColumnTotals) {
    if let Some(function) = totals.function {
        let _ = write!(out, " totalsRowFunction=\"{function}\"");
    }
    if let Some(label) = &totals.label {
        let _ = write!(out, " totalsRowLabel=\"{}\"", XmlSanitizer::escape(label));
    }
}

/// The child element of a `custom` column, or `None` for a column that has none.
pub(crate) fn formula_child(totals: &ColumnTotals) -> Option<String> {
    totals.formula.as_ref().map(|f| {
        format!(
            "<totalsRowFormula>{}</totalsRowFormula>",
            XmlSanitizer::escape(f)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(raw: &str) -> ColumnTotals {
        column_totals(Some(raw), "Sales", "Q1 total")
    }

    #[test]
    fn every_function_of_the_list_is_read_from_its_subtotal() {
        for (code, function) in FUNCTIONS {
            let totals = of(&format!("=SUBTOTAL({code},Sales[Q1 total])"));
            assert_eq!(totals.function, Some(function), "{code}");
            assert!(totals.formula.is_none() && totals.label.is_none());
        }
    }

    #[test]
    fn the_columns_own_forms_of_the_reference_count() {
        for arg in [
            "Sales[Q1 total]",
            "sales[[Q1 TOTAL]]",
            "[Q1 total]",
            "[[Q1 total]]",
            "Sales[[#Data],[Q1 total]]",
        ] {
            assert_eq!(
                of(&format!("=subtotal( 109 , {arg} )")).function,
                Some("sum"),
                "{arg}"
            );
        }
    }

    #[test]
    fn a_column_with_brackets_is_quoted_in_the_reference() {
        let totals = column_totals(Some("=SUBTOTAL(109,T[[a'[b'#]])"), "T", "a[b#");
        assert_eq!(totals.function, Some("sum"));
    }

    #[test]
    fn another_formula_is_custom_and_keeps_its_text() {
        for raw in [
            "=SUM(Sales[Q1 total])",
            "=SUBTOTAL(9,Sales[Q1 total])",
            "=SUBTOTAL(106,Sales[Q1 total])",
            "=SUBTOTAL(109,Sales[Other])",
            "=SUBTOTAL(109,Sales[Q1 total],1)",
        ] {
            let totals = of(raw);
            assert_eq!(totals.function, Some("custom"), "{raw}");
            assert_eq!(totals.formula.as_deref(), Some(&raw[1..]));
        }
    }

    #[test]
    fn a_text_is_the_label_and_an_empty_cell_says_nothing() {
        assert_eq!(of("요약").label.as_deref(), Some("요약"));
        assert_eq!(column_totals(None, "Sales", "Q1"), ColumnTotals::default());
        assert_eq!(
            column_totals(Some(""), "Sales", "Q1"),
            ColumnTotals::default()
        );
    }

    #[test]
    fn a_formula_or_label_over_its_limit_is_not_written() {
        let huge = format!("=SUM({})", "A1,".repeat(MAX_TOTALS_FORMULA_CHARS));
        assert_eq!(of(&huge), ColumnTotals::default());
        let long = "x".repeat(MAX_TABLE_TEXT_CHARS + 1);
        assert_eq!(of(&long), ColumnTotals::default());
    }

    #[test]
    fn the_attributes_and_the_child_are_escaped() {
        let totals = of("=A1<\"&\">B1");
        let mut out = String::new();
        write_attributes(&mut out, &totals);
        assert_eq!(out, " totalsRowFunction=\"custom\"");
        let child = formula_child(&totals).unwrap();
        assert!(child.contains("&lt;") && child.contains("&amp;") && !child.contains("<\""));
        let label = of("a\"<b");
        let mut out = String::new();
        write_attributes(&mut out, &label);
        assert!(out.contains("totalsRowLabel=\"a&quot;&lt;b\""), "{out}");
    }
}

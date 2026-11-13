//! Defined names of a workbook (PD-214): a name for a cell range, a constant or a formula, visible in the
//! whole workbook or in one sheet.
//!
//! 참조 구현: Univer `packages/sheets/src/basics/defined-name-utils.ts` (`validateDefinedName`: empty,
//! duplicate, invalid, same as a sheet or a function) and `packages/engine-formula/src/services/
//! defined-names.service.ts` (fields `name`, `formulaOrRefString`, `comment`, `localSheetId`, `hidden`);
//! IronCalc `base/src/model.rs` (`DefinedNameEntry`, `is_valid_defined_name`, comment of at most 255
//! characters; Apache-2.0 or MIT). The syntax follows ECMA-376 18.2.5 `definedName` and the rules of the
//! spreadsheet programs: a letter, `_` or `\` first, then letters, digits, `_`, `.`, `\` and `?`, at most
//! 255 characters, never a cell address (`A1`, `XFD1048576`), a row/column style address (`R1C1`, `R`, `C`)
//! or a boolean. Case never tells two names apart.
//!
//! Every size is bounded so a hostile file or payload cannot make the engine build an unbounded table.

use crate::CellBox;

/// Names a workbook may carry. Past this a reader drops the rest and a request is refused.
pub const MAX_NAMES: usize = 10_000;
/// Characters of a name (ECMA-376 18.2.5).
pub const MAX_NAME_CHARS: usize = 255;
/// Characters of what a name refers to, as many as a formula may have.
pub const MAX_REFERS_TO_CHARS: usize = 8_192;
/// Characters of a name's comment.
pub const MAX_NAME_COMMENT_CHARS: usize = 255;

/// One defined name. `refers_to` is a formula without the leading `=`; its references name their sheet
/// (`Sheet1!$A$1:$B$2`). `scope` is the sheet the name belongs to, `None` for the whole workbook.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefinedName {
    pub name: String,
    pub scope: Option<String>,
    pub refers_to: String,
    pub comment: String,
    pub hidden: bool,
}

/// Why a text cannot be the name of a defined name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    TooLong,
    /// The first character is not a letter, `_` or `\`.
    BadStart(char),
    /// A character a name cannot hold (a space, an operator, a bracket, ...).
    BadChar(char),
    /// Reads as a cell address (`A1`).
    CellAddress,
    /// Reads as a row/column address (`R1C1`, `R`, `C`).
    RowColumnAddress,
    /// `TRUE` or `FALSE`.
    Boolean,
}

/// The first rule `name` breaks. Whether another name or a sheet already has the name is for the caller.
pub fn defined_name_problem(name: &str) -> Option<NameProblem> {
    let Some(first) = name.chars().next() else {
        return Some(NameProblem::Empty);
    };
    if name.chars().count() > MAX_NAME_CHARS {
        return Some(NameProblem::TooLong);
    }
    if !(first.is_alphabetic() || first == '_' || first == '\\') {
        return Some(NameProblem::BadStart(first));
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?')))
    {
        return Some(NameProblem::BadChar(c));
    }
    if looks_like_cell(name) {
        return Some(NameProblem::CellAddress);
    }
    if looks_like_row_column(name) {
        return Some(NameProblem::RowColumnAddress);
    }
    (name.eq_ignore_ascii_case("TRUE") || name.eq_ignore_ascii_case("FALSE"))
        .then_some(NameProblem::Boolean)
}

/// `A1` to `XFD1048576`: one to three letters, then digits, inside the sheet's grid.
fn looks_like_cell(name: &str) -> bool {
    let letters = name.bytes().take_while(u8::is_ascii_alphabetic).count();
    let digits = &name.as_bytes()[letters..];
    (1..=3).contains(&letters)
        && !digits.is_empty()
        && digits.iter().all(u8::is_ascii_digit)
        && CellBox::parse(name).is_some()
}

/// `R`, `C`, `RC`, `R1`, `C1`, `R1C1`: an optional `R` with digits, then an optional `C` with digits.
fn looks_like_row_column(name: &str) -> bool {
    let rest = name.strip_prefix(['R', 'r']);
    let after_row = rest.map(|r| r.trim_start_matches(|c: char| c.is_ascii_digit()));
    let rest = match (rest, after_row) {
        (Some(_), Some(after)) => after,
        _ => name,
    };
    let after_col = match rest.strip_prefix(['C', 'c']) {
        Some(r) => r.trim_start_matches(|c: char| c.is_ascii_digit()),
        None => rest,
    };
    after_col.is_empty() && !name.is_empty()
}

/// The text a name is compared by: names differ without regard to case.
pub fn fold_name(name: &str) -> String {
    if name.is_ascii() {
        name.to_ascii_lowercase()
    } else {
        name.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_names_a_sheet_can_hold() {
        for name in [
            "Rate",
            "세율",
            "_hidden",
            "a.b",
            "Total_2024",
            "\\path",
            "Q1_Sales",
            "ABCD1",
            "XFE1",
            "Rx",
            "Cc1x",
            "RCX",
        ] {
            assert_eq!(defined_name_problem(name), None, "{name:?}");
        }
    }

    #[test]
    fn rejects_what_reads_as_an_address_or_breaks_the_syntax() {
        let cases = [
            ("", NameProblem::Empty),
            ("1abc", NameProblem::BadStart('1')),
            ("a b", NameProblem::BadChar(' ')),
            ("a-b", NameProblem::BadChar('-')),
            ("a$1", NameProblem::BadChar('$')),
            ("A1", NameProblem::CellAddress),
            ("a1", NameProblem::CellAddress),
            ("XFD1048576", NameProblem::CellAddress),
            ("R1C1", NameProblem::RowColumnAddress),
            ("r", NameProblem::RowColumnAddress),
            ("C", NameProblem::RowColumnAddress),
            ("rc", NameProblem::RowColumnAddress),
            ("R12", NameProblem::CellAddress),
            ("R2C", NameProblem::RowColumnAddress),
            ("true", NameProblem::Boolean),
            ("False", NameProblem::Boolean),
        ];
        for (name, problem) in cases {
            assert_eq!(defined_name_problem(name), Some(problem), "{name:?}");
        }
        let long = "a".repeat(MAX_NAME_CHARS + 1);
        assert_eq!(defined_name_problem(&long), Some(NameProblem::TooLong));
        assert_eq!(defined_name_problem(&long[1..]), None);
    }

    #[test]
    fn folding_ignores_case_in_any_script() {
        assert_eq!(fold_name("Rate"), fold_name("RATE"));
        assert_eq!(fold_name("Über"), fold_name("üBER"));
    }
}

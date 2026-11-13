//! The defined names of a workbook in ODS (PD-214): `table:named-expressions` with `table:named-range`
//! (a name for one cell range) and `table:named-expression` (any other formula), as a child of
//! `office:spreadsheet` for a name of the workbook and as a child of the `table:table` for a name of that
//! sheet (OASIS ODF 1.2 9.4.11 to 9.4.14). The model is `sheet_names.rs`.
//!
//! 동작 참고: LibreOffice `sc/source/filter/xml/xmlexprt.cxx` (`WriteNamedRange`) and `xmlnexpi.cxx` (MPL-2.0,
//! behaviour only, no code copied): a range goes out as `table:cell-range-address` in the `$Sheet.$A$1:.$B$2`
//! spelling and a formula as `table:expression`, both with `table:base-cell-address`; a hidden formula is
//! `loext:hidden`, a hidden range a `hidden` token of `table:range-usable-as`.
//!
//! Modifications: the comment of a name is kept as the foreign attribute `eo:comment`. A range that spans
//! sheets or is not one plain rectangle is written and read as a formula.

use std::fmt::Write;

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_balanced_elements};
use crate::sheet_names::{
    defined_name_problem, fold_name, DefinedName, MAX_NAMES, MAX_NAME_COMMENT_CHARS,
    MAX_REFERS_TO_CHARS,
};
use crate::{CellBox, OdfCodec, SheetStyling, XmlSanitizer};

/// The namespace of `loext:hidden`.
pub(crate) const LOEXT_NS: &str =
    "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0";

fn col_letters(col: u32) -> String {
    let mut buf = [0u8; 4];
    crate::rtf_xml_helpers::write_col_letters(&mut buf, col as usize).to_string()
}

fn ods_sheet(name: &str) -> String {
    if !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
    {
        format!("${name}")
    } else {
        format!("$'{}'", name.replace('\'', "''"))
    }
}

/// `Sheet1!$A$1:$B$2` as the sheet and its rectangle, when that is all the formula is.
fn plain_range(refers_to: &str) -> Option<(String, CellBox)> {
    let text = refers_to.trim();
    let at = if let Some(rest) = text.strip_prefix('\'') {
        let mut i = 0;
        let bytes = rest.as_bytes();
        loop {
            let q = rest[i..].find('\'')? + i;
            if bytes.get(q + 1) == Some(&b'\'') {
                i = q + 2;
            } else {
                break q + 2;
            }
        }
    } else {
        text.find('!')?
    };
    let (sheet, range) = text.split_at(at);
    let range = range.strip_prefix('!')?;
    if range.contains([
        '(', ',', ' ', '+', '-', '*', '/', '^', '&', '=', '<', '>', '"', ';',
    ]) {
        return None;
    }
    let sheet = sheet
        .strip_prefix('\'')
        .map_or(sheet, |s| s.strip_suffix('\'').unwrap_or(s))
        .replace("''", "'");
    Some((sheet, CellBox::parse(range)?)).filter(|(s, _)| !s.is_empty())
}

fn absolute(cell: (u32, u32)) -> String {
    format!("${}${}", col_letters(cell.1), cell.0 + 1)
}

fn push_one(out: &mut String, def: &DefinedName, base_sheet: &str) {
    let _ = write!(out, " table:name=\"{}\"", XmlSanitizer::escape(&def.name));
    let _ = write!(
        out,
        " table:base-cell-address=\"{}.$A$1\"",
        XmlSanitizer::escape(&ods_sheet(base_sheet))
    );
    if !def.comment.is_empty() {
        let _ = write!(
            out,
            " eo:comment=\"{}\"",
            XmlSanitizer::escape(&def.comment)
        );
    }
}

/// `<table:named-expressions>` for the names of one scope (`scope` is the sheet, `None` for the workbook),
/// empty when there are none. `first_sheet` is where the base cell of a workbook's name is.
pub(crate) fn names_xml(names: &[DefinedName], scope: Option<&str>, first_sheet: &str) -> String {
    let same = |def: &DefinedName| match (def.scope.as_deref(), scope) {
        (None, None) => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let base = scope.unwrap_or(first_sheet);
    let mut out = String::new();
    for def in names.iter().filter(|d| same(d)) {
        if let Some((sheet, b)) = plain_range(&def.refers_to) {
            out.push_str("<table:named-range");
            push_one(&mut out, def, base);
            let to = if (b.row0, b.col0) == (b.row1, b.col1) {
                String::new()
            } else {
                format!(":.{}", absolute((b.row1, b.col1)))
            };
            let _ = write!(
                out,
                " table:cell-range-address=\"{}\"",
                XmlSanitizer::escape(&format!(
                    "{}.{}{to}",
                    ods_sheet(&sheet),
                    absolute((b.row0, b.col0))
                ))
            );
            if def.hidden {
                out.push_str(" table:range-usable-as=\"hidden\"");
            }
        } else {
            out.push_str("<table:named-expression");
            push_one(&mut out, def, base);
            let _ = write!(
                out,
                " table:expression=\"of:={}\"",
                XmlSanitizer::escape(&OdfCodec::to_open_formula_syntax(&def.refers_to))
            );
            if def.hidden {
                out.push_str(" loext:hidden=\"true\"");
            }
        }
        out.push_str("/>");
    }
    if out.is_empty() {
        out
    } else {
        format!("<table:named-expressions>{out}</table:named-expressions>")
    }
}

/// `$Sheet1.$A$1` or `$'My Data'.A1` as the sheet and the cell text; a part after `:` may leave the sheet out.
fn split_address(part: &str) -> Option<(Option<String>, &str)> {
    let part = part.trim().trim_start_matches('$');
    if let Some(rest) = part.strip_prefix('\'') {
        let mut i = 0;
        let bytes = rest.as_bytes();
        loop {
            let q = rest[i..].find('\'')? + i;
            if bytes.get(q + 1) == Some(&b'\'') {
                i = q + 2;
            } else {
                let sheet = rest[..q].replace("''", "'");
                return Some((Some(sheet), rest[q + 1..].strip_prefix('.')?));
            }
        }
    }
    let dot = part.find('.')?;
    let sheet = &part[..dot];
    Some((
        (!sheet.is_empty()).then(|| sheet.to_string()),
        &part[dot + 1..],
    ))
}

fn quote(sheet: &str) -> String {
    if sheet.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !sheet.starts_with(|c: char| c.is_ascii_digit())
    {
        sheet.to_string()
    } else {
        format!("'{}'", sheet.replace('\'', "''"))
    }
}

/// `$Sheet1.$A$1:.$B$2` as `Sheet1!$A$1:$B$2`; `None` for an address that spans sheets or is not a range.
fn range_from_ods(address: &str) -> Option<String> {
    let (from, to) = address.split_once(':').unwrap_or((address, address));
    let (sheet, first) = split_address(from)?;
    let sheet = sheet?;
    let (other, last) = split_address(to)?;
    if other.is_some_and(|o| !o.eq_ignore_ascii_case(&sheet)) {
        return None;
    }
    let b = CellBox::parse(&format!("{first}:{last}"))?;
    let tail = if (b.row0, b.col0) == (b.row1, b.col1) {
        String::new()
    } else {
        format!(":{}", absolute((b.row1, b.col1)))
    };
    Some(format!(
        "{}!{}{tail}",
        quote(&sheet),
        absolute((b.row0, b.col0))
    ))
}

/// The names of one `table:named-expressions` element.
fn read_block(inner: &str, scope: Option<&str>, out: &mut Vec<DefinedName>, rejected: &mut u32) {
    let items = extract_balanced_elements(inner, "table:named-range")
        .into_iter()
        .map(|(attrs, _)| (attrs, true))
        .chain(
            extract_balanced_elements(inner, "table:named-expression")
                .into_iter()
                .map(|(attrs, _)| (attrs, false)),
        );
    for (attrs, is_range) in items {
        let attr = |name: &str| extract_attr_from_fragment(attrs, name).map(XmlSanitizer::unescape);
        let Some(name) = attr("table:name") else {
            continue;
        };
        let (refers_to, hidden) = if is_range {
            (
                attr("table:cell-range-address").and_then(|a| range_from_ods(&a)),
                attr("table:range-usable-as")
                    .is_some_and(|u| u.split_whitespace().any(|t| t == "hidden")),
            )
        } else {
            (
                attr("table:expression")
                    .map(|e| OdfCodec::normalize_ods_formula(&e)[1..].trim().to_string()),
                attr("loext:hidden").as_deref() == Some("true"),
            )
        };
        let comment = attr("eo:comment").unwrap_or_default();
        let usable = refers_to
            .as_ref()
            .is_some_and(|r| r.chars().count() <= MAX_REFERS_TO_CHARS)
            && comment.chars().count() <= MAX_NAME_COMMENT_CHARS
            && defined_name_problem(&name).is_none()
            && out.len() < MAX_NAMES
            && !out.iter().any(|d| {
                fold_name(&d.name) == fold_name(&name)
                    && d.scope.as_deref().map(str::to_lowercase) == scope.map(str::to_lowercase)
            });
        let Some(refers_to) = refers_to.filter(|_| usable) else {
            *rejected = rejected.saturating_add(1);
            continue;
        };
        out.push(DefinedName {
            name,
            scope: scope.map(str::to_string),
            refers_to,
            comment,
            hidden,
        });
    }
}

/// Reads the names of the workbook and of its tables into the styling of the first sheet.
pub(crate) fn read_names(content: &str, stylings: &mut [(String, SheetStyling)]) {
    if !content.contains("table:named-expressions") {
        return;
    }
    let mut names = Vec::new();
    let mut rejected = 0u32;
    for (attrs, inner) in extract_balanced_elements(content, "table:table") {
        let Some(table) =
            extract_attr_from_fragment(attrs, "table:name").map(XmlSanitizer::unescape)
        else {
            continue;
        };
        let Some((sheet, _)) = stylings.iter().find(|(n, _)| *n == table) else {
            continue;
        };
        for (_, block) in extract_balanced_elements(inner, "table:named-expressions") {
            read_block(block, Some(&sheet.clone()), &mut names, &mut rejected);
        }
    }
    // The workbook's names follow the last table (ODF 1.2 puts them after the tables).
    let tail = content
        .rfind("</table:table>")
        .map_or(content, |at| &content[at..]);
    for (_, block) in extract_balanced_elements(tail, "table:named-expressions") {
        read_block(block, None, &mut names, &mut rejected);
    }
    if let Some((_, first)) = stylings.first_mut() {
        first.defined_names = names;
        first.names_rejected = rejected;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, scope: Option<&str>, refers_to: &str) -> DefinedName {
        DefinedName {
            name: name.to_string(),
            scope: scope.map(str::to_string),
            refers_to: refers_to.to_string(),
            ..DefinedName::default()
        }
    }

    #[test]
    fn a_plain_range_is_a_named_range_and_anything_else_a_named_expression() {
        let names = [
            def("Prices", None, "Sheet1!$A$1:$A$3"),
            def("One", None, "'My Data'!$B$2"),
            def("Calc", None, "SUM(Sheet1!$A$1:$A$3)*2"),
            def("Hidden", None, "Sheet1!$C$1"),
        ];
        let xml = names_xml(&names, None, "Sheet1");
        assert!(
            xml.contains("<table:named-range table:name=\"Prices\""),
            "{xml}"
        );
        assert!(
            xml.contains("table:cell-range-address=\"$Sheet1.$A$1:.$A$3\""),
            "{xml}"
        );
        assert!(
            xml.contains("table:cell-range-address=\"$&apos;My Data&apos;.$B$2\""),
            "{xml}"
        );
        assert!(
            xml.contains("<table:named-expression table:name=\"Calc\""),
            "{xml}"
        );
        assert!(xml.contains("table:expression=\"of:="), "{xml}");
    }

    #[test]
    fn ranges_and_formulas_read_back_as_written() {
        let mut hidden = def("Hid", None, "Sheet1!$C$1");
        hidden.hidden = true;
        let mut commented = def("Calc", None, "SUM(Sheet1!$A$1:$A$3)*2+\"a<b\"");
        commented.comment = "x & y".to_string();
        let names = vec![
            def("Prices", None, "Sheet1!$A$1:$A$3"),
            def("One", None, "'My Data'!$B$2"),
            commented,
            hidden,
            def("Local", Some("My Data"), "'My Data'!$A$1:$C$9"),
        ];
        let content = format!(
            "<office:spreadsheet><table:table table:name=\"Sheet1\"><table:table-row/></table:table>\
             <table:table table:name=\"My Data\"><table:table-row/>{}</table:table>{}</office:spreadsheet>",
            names_xml(&names, Some("My Data"), "Sheet1"),
            names_xml(&names, None, "Sheet1"),
        );
        let mut stylings = vec![
            ("Sheet1".to_string(), SheetStyling::default()),
            ("My Data".to_string(), SheetStyling::default()),
        ];
        read_names(&content, &mut stylings);
        let mut read = stylings[0].1.defined_names.clone();
        let mut want = names;
        read.sort_by(|a, b| a.name.cmp(&b.name));
        want.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(read, want);
        assert_eq!(stylings[0].1.names_rejected, 0);
        assert!(stylings[1].1.defined_names.is_empty());
    }

    #[test]
    fn names_a_spreadsheet_program_wrote_are_read() {
        let content = "<office:spreadsheet><table:table table:name=\"Sheet1\"/>\
            <table:named-expressions>\
            <table:named-range table:name=\"Pr\" table:base-cell-address=\"$Sheet1.$A$1\" table:cell-range-address=\"$Sheet1.$A$1:.$B$3\"/>\
            <table:named-range table:name=\"Span\" table:cell-range-address=\"$Sheet1.$A$1:$Sheet2.$B$3\"/>\
            <table:named-expression table:name=\"Dbl\" table:base-cell-address=\"$Sheet1.$A$1\" table:expression=\"[$Sheet1.$A$1]*2\"/>\
            </table:named-expressions></office:spreadsheet>";
        let mut stylings = vec![("Sheet1".to_string(), SheetStyling::default())];
        read_names(content, &mut stylings);
        let read = &stylings[0].1;
        assert_eq!(read.defined_names.len(), 2);
        assert_eq!(read.defined_names[0].refers_to, "Sheet1!$A$1:$B$3");
        assert_eq!(read.defined_names[1].refers_to, "Sheet1!$A$1*2");
        assert_eq!(read.names_rejected, 1);
    }
}

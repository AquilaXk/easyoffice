//! The defined names of a workbook in XLSX (PD-214): `<definedNames><definedName name localSheetId hidden
//! comment>formula</definedName>` in `xl/workbook.xml` (ECMA-376 18.2.5 and 18.2.6). A name with a
//! `localSheetId` belongs to the sheet at that position, one without belongs to the workbook. The built-in
//! names `_xlnm.*` are the print areas and titles of [`super::xlsx_page`]; the others are left out, as are
//! the names of macros (`function`, `vbProcedure`, `xlm`).
//!
//! A name that breaks the syntax of a name, repeats another of the same scope or is past [`MAX_NAMES`] is
//! not read; the count of those goes to the styling of the first sheet.

use std::collections::HashSet;
use std::fmt::Write;

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_balanced_elements};
use crate::sheet_names::{
    defined_name_problem, fold_name, DefinedName, MAX_NAMES, MAX_NAME_COMMENT_CHARS,
    MAX_REFERS_TO_CHARS,
};
use crate::{SheetStyling, XmlSanitizer};

/// Whether a name of the file is one a reader gives to [`DefinedName`] (not a built-in or a macro).
fn is_plain(attrs: &str, name: &str) -> bool {
    !name.starts_with("_xlnm.")
        && ["function", "vbProcedure", "xlm"]
            .iter()
            .all(|flag| !matches!(extract_attr_from_fragment(attrs, flag), Some("1" | "true")))
}

/// The defined names of the workbook part and how many names were left out.
pub(crate) fn read_defined_names(
    workbook_xml: &str,
    sheet_names: &[String],
) -> (Vec<DefinedName>, u32) {
    let mut out: Vec<DefinedName> = Vec::new();
    let mut seen: HashSet<(String, Option<usize>)> = HashSet::new();
    let mut rejected = 0u32;
    for (attrs, inner) in extract_balanced_elements(workbook_xml, "definedName") {
        let Some(name) = extract_attr_from_fragment(attrs, "name").map(XmlSanitizer::unescape)
        else {
            continue;
        };
        if !is_plain(attrs, &name) {
            continue;
        }
        let local = extract_attr_from_fragment(attrs, "localSheetId").map(|v| v.parse::<usize>());
        let scope = match local {
            None => None,
            Some(Ok(i)) if i < sheet_names.len() => Some(i),
            Some(_) => {
                rejected = rejected.saturating_add(1);
                continue;
            }
        };
        let refers_to = XmlSanitizer::unescape(inner);
        let comment = extract_attr_from_fragment(attrs, "comment")
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();
        let usable = out.len() < MAX_NAMES
            && defined_name_problem(&name).is_none()
            && refers_to.chars().count() <= MAX_REFERS_TO_CHARS
            && comment.chars().count() <= MAX_NAME_COMMENT_CHARS
            && seen.insert((fold_name(&name), scope));
        if !usable {
            rejected = rejected.saturating_add(1);
            continue;
        }
        out.push(DefinedName {
            name,
            scope: scope.map(|i| sheet_names[i].clone()),
            refers_to: refers_to.trim().to_string(),
            comment,
            hidden: matches!(
                extract_attr_from_fragment(attrs, "hidden"),
                Some("1" | "true")
            ),
        });
    }
    (out, rejected)
}

fn push_name(out: &mut String, def: &DefinedName, local: Option<usize>) {
    let _ = write!(
        out,
        "<definedName name=\"{}\"",
        XmlSanitizer::escape(&def.name)
    );
    if !def.comment.is_empty() {
        let _ = write!(out, " comment=\"{}\"", XmlSanitizer::escape(&def.comment));
    }
    if let Some(i) = local {
        let _ = write!(out, " localSheetId=\"{i}\"");
    }
    if def.hidden {
        out.push_str(" hidden=\"1\"");
    }
    let _ = write!(
        out,
        ">{}</definedName>",
        XmlSanitizer::escape(&def.refers_to)
    );
}

/// `<definedNames>` for the print names of the sheets and the defined names of the workbook (the first
/// sheet's styling carries them), empty without any.
pub(crate) fn defined_names_xml(sheet_names: &[String], stylings: &[SheetStyling]) -> String {
    let mut inner = super::xlsx_page::print_names_inner(sheet_names, stylings);
    for def in stylings.first().map_or(&[][..], |s| &s.defined_names[..]) {
        let local = def.scope.as_deref().and_then(|scope| {
            sheet_names.iter().position(|n| {
                n.eq_ignore_ascii_case(scope) || n.to_lowercase() == scope.to_lowercase()
            })
        });
        if def.scope.is_some() && local.is_none() {
            continue;
        }
        push_name(&mut inner, def, local);
    }
    if inner.is_empty() {
        inner
    } else {
        format!("<definedNames>{inner}</definedNames>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheets() -> Vec<String> {
        vec!["Sheet1".to_string(), "My Data".to_string()]
    }

    fn def(name: &str, scope: Option<&str>, refers_to: &str) -> DefinedName {
        DefinedName {
            name: name.to_string(),
            scope: scope.map(str::to_string),
            refers_to: refers_to.to_string(),
            ..DefinedName::default()
        }
    }

    #[test]
    fn names_round_trip_with_scope_hidden_and_comment() {
        let names = vec![
            def("Prices", None, "Sheet1!$A$1:$A$3"),
            DefinedName {
                hidden: true,
                comment: "a < b & \"c\"".to_string(),
                ..def("Local", Some("My Data"), "'My Data'!$B$2")
            },
            def("Calc", None, "SUM(Prices)*2&\"<>\""),
        ];
        let styling = SheetStyling {
            defined_names: names.clone(),
            ..SheetStyling::default()
        };
        let xml = defined_names_xml(&sheets(), &[styling, SheetStyling::default()]);
        assert!(xml.starts_with("<definedNames>"), "{xml}");
        assert!(xml.contains("localSheetId=\"1\" hidden=\"1\""), "{xml}");
        let (read, rejected) = read_defined_names(&xml, &sheets());
        assert_eq!(read, names);
        assert_eq!(rejected, 0);
    }

    #[test]
    fn print_names_and_defined_names_share_one_element() {
        let mut first = SheetStyling::default();
        first.page.print_area = vec![crate::CellBox::parse("A1:B2").unwrap()];
        first.defined_names = vec![def("Prices", None, "Sheet1!$A$1")];
        let xml = defined_names_xml(&sheets(), &[first, SheetStyling::default()]);
        assert_eq!(xml.matches("<definedNames>").count(), 1);
        assert!(xml.contains("_xlnm.Print_Area") && xml.contains("name=\"Prices\""));
        let (read, _) = read_defined_names(&xml, &sheets());
        assert_eq!(read.len(), 1, "the built-in name is not a defined name");
    }

    #[test]
    fn a_file_with_odd_names_is_read_without_failing() {
        let xml = "<definedNames>\
            <definedName name=\"Good\">Sheet1!$A$1</definedName>\
            <definedName name=\"good\">Sheet1!$A$2</definedName>\
            <definedName name=\"A1\">Sheet1!$A$1</definedName>\
            <definedName name=\"two words\">1</definedName>\
            <definedName name=\"Macro\" function=\"1\">1</definedName>\
            <definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"0\" hidden=\"1\">Sheet1!$A$1:$B$2</definedName>\
            <definedName name=\"Far\" localSheetId=\"9\">1</definedName>\
            <definedName name=\"Union\">Sheet1!$A$1,Sheet1!$C$1</definedName>\
            <definedName name=\"Empty\"/>\
            </definedNames>";
        let (read, rejected) = read_defined_names(xml, &sheets());
        let names: Vec<&str> = read.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Good", "Union", "Empty"]);
        assert_eq!(
            rejected, 4,
            "a repeated name, two that break the rules and a scope that is no sheet"
        );
        assert_eq!(read[1].refers_to, "Sheet1!$A$1,Sheet1!$C$1");
    }

    #[test]
    fn names_past_the_limit_are_dropped_and_counted() {
        let mut xml = String::from("<definedNames>");
        for i in 0..MAX_NAMES + 5 {
            let _ = write!(xml, "<definedName name=\"n_{i}\">1</definedName>");
        }
        xml.push_str("</definedNames>");
        let (read, rejected) = read_defined_names(&xml, &sheets());
        assert_eq!(read.len(), MAX_NAMES);
        assert_eq!(rejected, 5);
    }
}

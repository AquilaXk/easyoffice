//! Protection in an ODS file (PD-216): `table:protected`, `table:protection-key` and
//! `table:protection-key-digest-algorithm` of a table, `table:structure-protected` and the same key
//! attributes of `office:spreadsheet` for the workbook, and the allowed actions of a table in the child
//! `loext:table-protection`. The cell side (`style:cell-protect`) is in `ods_styles`. The model is
//! `sheet_protect`.
//!
//! 동작 참고: the behaviour of a widely used open-source office suite (MPL-2.0; behaviour only, no code is
//! copied): the key is the base64 of a digest of the UTF-8 password (SHA-256 under the `xmldsig#sha256` URI
//! when written, SHA-1 under `xmldsig#sha1` or no URI when read, and a UTF-16LE SHA-1 from older files), an
//! allowed action is a `true` attribute of `loext:table-protection` and an absent attribute is a locked
//! action, a table protected without that element allows selecting cells only.
//!
//! Modifications: ODS has no flags for formatting, sorting, hyperlinks, objects or scenarios, so they read as
//! locked and are not written. A digest that is not an ODS key (the agile or legacy digest of an XLSX file read
//! from disk; a password set in the editor also carries the key) cannot be written: the table is written protected without a password, which is stated in PD-216. A key
//! with an algorithm this program does not run is kept and never opens.

use std::fmt::Write;

use crate::rtf_xml_helpers::extract_attr_from_fragment;
use crate::sheet_protect::{
    credential_within_bounds, Credential, OdfKey, SheetProtection, WorkbookProtection, AUTO_FILTER,
    DELETE_COLUMNS, DELETE_ROWS, FLAG_NAMES, INSERT_COLUMNS, INSERT_ROWS, OBJECTS, PIVOT_TABLES,
    SCENARIOS, SELECT_LOCKED_CELLS, SELECT_UNLOCKED_CELLS,
};
use crate::{EasyOfficeError, SheetStyling, XmlSanitizer};

/// The flags `loext:table-protection` can carry, with their attribute names (the attribute says allowed).
const ALLOW_ATTRS: [(&str, u16); 8] = [
    ("loext:select-protected-cells", SELECT_LOCKED_CELLS),
    ("loext:select-unprotected-cells", SELECT_UNLOCKED_CELLS),
    ("loext:insert-columns", INSERT_COLUMNS),
    ("loext:insert-rows", INSERT_ROWS),
    ("loext:delete-columns", DELETE_COLUMNS),
    ("loext:delete-rows", DELETE_ROWS),
    ("loext:use-autofilter", AUTO_FILTER),
    ("loext:use-pivot", PIVOT_TABLES),
];

/// Every flag bit: an action ODS has no attribute for stays locked.
const ODS_LOCKED: u16 = {
    let mut deny = 0u16;
    let mut i = 0;
    while i < FLAG_NAMES.len() {
        deny |= FLAG_NAMES[i].1;
        i += 1;
    }
    deny
};

fn too_long() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(
        "ODS 보호 정보의 문자열이 허용 길이를 초과했습니다.".to_string(),
    )
}

/// The key the attributes of a start tag describe.
fn read_key(tag: &str) -> Result<Credential, EasyOfficeError> {
    let Some(key) = extract_attr_from_fragment(tag, "table:protection-key") else {
        return Ok(Credential::None);
    };
    let credential = Credential::Odf(OdfKey {
        key: XmlSanitizer::unescape(key),
        algorithm: extract_attr_from_fragment(tag, "table:protection-key-digest-algorithm")
            .map(XmlSanitizer::unescape),
    });
    if credential_within_bounds(&credential) {
        Ok(credential)
    } else {
        Err(too_long())
    }
}

fn write_key(xml: &mut String, credential: &Credential) {
    if let Some(odf) = credential.odf_part() {
        let _ = write!(
            xml,
            " table:protection-key=\"{}\"",
            XmlSanitizer::escape(&odf.key)
        );
        if let Some(algorithm) = &odf.algorithm {
            let _ = write!(
                xml,
                " table:protection-key-digest-algorithm=\"{}\"",
                XmlSanitizer::escape(algorithm)
            );
        }
    }
}

/// An attribute that says `true`. `closed` reads a value that is not a boolean as `true` too (an attribute that
/// protects), `open` reads it as `false` (an action that is allowed).
fn is_true(tag: &str, name: &str) -> bool {
    extract_attr_from_fragment(tag, name) == Some("true")
}

fn is_closed(tag: &str, name: &str) -> bool {
    extract_attr_from_fragment(tag, name).is_some_and(|v| v.trim() != "false")
}

/// The attributes of `table:table` that protect it (empty for a table that is not protected).
pub(crate) fn table_attrs(protect: &SheetProtection) -> String {
    let mut xml = String::new();
    if protect.on {
        xml.push_str(" table:protected=\"true\"");
        write_key(&mut xml, &protect.credential);
    }
    xml
}

/// `<loext:table-protection .../>`, the first child of a protected table (empty otherwise).
pub(crate) fn table_protection_xml(protect: &SheetProtection) -> String {
    if !protect.on {
        return String::new();
    }
    let mut xml = String::from("<loext:table-protection");
    for (name, bit) in ALLOW_ATTRS {
        let _ = write!(xml, " {name}=\"{}\"", protect.deny & bit == 0);
    }
    xml.push_str("/>");
    xml
}

/// The attributes of `office:spreadsheet` that protect the structure.
pub(crate) fn body_attrs(protect: &WorkbookProtection) -> String {
    let mut xml = String::new();
    if protect.lock_structure {
        xml.push_str(" table:structure-protected=\"true\"");
        write_key(&mut xml, &protect.credential);
    }
    xml
}

/// The start tag text of the first element `name` (`<office:spreadsheet ...>`), without the angle brackets.
fn start_tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(at) = xml[from..].find(&open) {
        let begin = from + at + open.len();
        let next = xml.as_bytes().get(begin).copied();
        if matches!(next, Some(b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r')) {
            let end = begin + xml[begin..].find('>')?;
            return Some(&xml[begin..end]);
        }
        from = begin;
    }
    None
}

/// Reads the protection of the workbook and of every table into `stylings`, in table order.
pub(crate) fn read_protection(
    content_xml: &str,
    stylings: &mut [(String, SheetStyling)],
) -> Result<(), EasyOfficeError> {
    if !content_xml.contains("protected=") {
        return Ok(());
    }
    if let (Some(tag), Some((_, first))) = (
        start_tag(content_xml, "office:spreadsheet"),
        stylings.first_mut(),
    ) {
        if is_closed(tag, "table:structure-protected") {
            first.workbook_protect = Some(WorkbookProtection {
                lock_structure: true,
                credential: read_key(tag)?,
                ..WorkbookProtection::default()
            });
        }
    }
    let tables = crate::rtf_xml_helpers::extract_balanced_elements(content_xml, "table:table");
    for (i, (_, styling)) in stylings.iter_mut().enumerate() {
        let Some((attrs, inner)) = tables.get(i) else {
            break;
        };
        if !is_closed(attrs, "table:protected") {
            continue;
        }
        let mut deny = ODS_LOCKED & !(SELECT_LOCKED_CELLS | SELECT_UNLOCKED_CELLS);
        if let Some(at) = inner.find("<loext:table-protection") {
            let tag = start_tag(&inner[at..], "loext:table-protection").unwrap_or("");
            deny = ODS_LOCKED;
            for (name, bit) in ALLOW_ATTRS {
                if is_true(tag, name) {
                    deny &= !bit;
                }
            }
        }
        // The objects and scenarios of a table stay locked with it.
        deny |= OBJECTS | SCENARIOS;
        styling.protect = Some(SheetProtection {
            on: true,
            deny,
            credential: read_key(attrs)?,
            ranges: Vec::new(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PasswordCheck, FORMAT_CELLS};

    fn read(xml: &str, tables: usize) -> Vec<(String, SheetStyling)> {
        let mut stylings: Vec<(String, SheetStyling)> = (0..tables)
            .map(|i| (format!("S{i}"), SheetStyling::default()))
            .collect();
        read_protection(xml, &mut stylings).unwrap();
        stylings
    }

    #[test]
    fn a_protected_table_with_options_and_a_key() {
        let key = Credential::new_odf("pw").unwrap();
        let mut p = SheetProtection::locked_with(key);
        p.deny &= !(INSERT_ROWS | SELECT_LOCKED_CELLS | SELECT_UNLOCKED_CELLS);
        let xml = format!(
            "<office:spreadsheet><table:table table:name=\"A\"{}>{}<table:table-row/></table:table><table:table table:name=\"B\"></table:table></office:spreadsheet>",
            table_attrs(&p),
            table_protection_xml(&p)
        );
        assert!(
            xml.contains("loext:insert-rows=\"true\"")
                && xml.contains("loext:delete-rows=\"false\"")
        );
        let read = read(&xml, 2);
        let got = read[0].1.protect.as_ref().unwrap();
        assert!(got.on && got.allows(INSERT_ROWS) && !got.allows(DELETE_ROWS));
        assert!(got.allows(SELECT_LOCKED_CELLS));
        // Formatting has no flag in ODS: it stays locked.
        assert!(!got.allows(FORMAT_CELLS));
        assert_eq!(got.credential.verify("pw"), PasswordCheck::Accepted);
        assert_eq!(got.credential.verify("pwx"), PasswordCheck::Rejected);
        assert!(read[1].1.protect.is_none());
    }

    #[test]
    fn a_table_protected_without_the_options_element_allows_selecting_only() {
        let xml = "<office:spreadsheet><table:table table:name=\"A\" table:protected=\"true\"></table:table></office:spreadsheet>";
        let read = read(xml, 1);
        let got = read[0].1.protect.as_ref().unwrap();
        assert!(got.allows(SELECT_LOCKED_CELLS) && got.allows(SELECT_UNLOCKED_CELLS));
        assert!(!got.allows(INSERT_ROWS));
        assert!(got.credential.is_none());
    }

    #[test]
    fn a_structure_key_and_hostile_keys() {
        let xml = "<office:spreadsheet table:structure-protected=\"true\" table:protection-key=\"AAAA\" table:protection-key-digest-algorithm=\"http://example.invalid/x\"><table:table table:name=\"A\"></table:table></office:spreadsheet>";
        let read = read(xml, 1);
        let w = read[0].1.workbook_protect.as_ref().unwrap();
        assert!(w.lock_structure);
        assert!(matches!(
            w.credential.verify("pw"),
            PasswordCheck::Unsupported(_)
        ));
        assert_eq!(body_attrs(w), " table:structure-protected=\"true\" table:protection-key=\"AAAA\" table:protection-key-digest-algorithm=\"http://example.invalid/x\"");
        let long = "A".repeat(5_000);
        let xml = format!("<office:spreadsheet><table:table table:name=\"A\" table:protected=\"true\" table:protection-key=\"{long}\"></table:table></office:spreadsheet>");
        let mut stylings = vec![("A".to_string(), SheetStyling::default())];
        assert!(read_protection(&xml, &mut stylings).is_err());
    }
}

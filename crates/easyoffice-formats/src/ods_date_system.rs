//! The date system of an ODS workbook (PD-263): `table:calculation-settings` holds `table:null-date`, the day whose serial is 0
//! (OASIS ODF 1.2 9.4.7 and 9.4.8). The default is 1899-12-30; 1904-01-01 is the 1904 date system of the other spreadsheet
//! formats. Any other null date is a count this app does not have, so the workbook is read as the default.

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_tags_or_self_closing_with_attrs};
use crate::SheetStyling;

const NULL_DATE_1904: &str = "1904-01-01";

/// `table:calculation-settings` for a workbook in the 1904 system; empty for the default one.
pub(crate) fn calculation_settings_xml(stylings: &[SheetStyling]) -> String {
    if stylings.first().is_some_and(|st| st.date1904) {
        format!(
            "<table:calculation-settings><table:null-date table:value-type=\"date\" table:date-value=\"{NULL_DATE_1904}\"/></table:calculation-settings>"
        )
    } else {
        String::new()
    }
}

/// Sets `date1904` on the first sheet when the workbook's null date is 1904-01-01.
pub(crate) fn read_date_system(content_xml: &str, stylings: &mut [(String, SheetStyling)]) {
    let is_1904 = extract_tags_or_self_closing_with_attrs(content_xml, "table:null-date")
        .into_iter()
        .next()
        .and_then(|(attrs, _)| extract_attr_from_fragment(attrs, "table:date-value"))
        .is_some_and(|date| date.trim() == NULL_DATE_1904);
    if let Some((_, first)) = stylings.first_mut() {
        first.date1904 = is_1904;
    }
}

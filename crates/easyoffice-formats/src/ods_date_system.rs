//! The date system and calculation settings of an ODS workbook (PD-263, PD-309):
//! `table:calculation-settings` holds `table:null-date` (default 1899-12-30, or 1904-01-01) and
//! `table:iteration` (OASIS ODF 1.2 9.4.6-9.4.9).

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_tags_or_self_closing_with_attrs};
use crate::sheet_styling::sheet_calc_options::{ods_iteration_xml, read_ods_iteration};
use crate::SheetStyling;

const NULL_DATE_1904: &str = "1904-01-01";

/// `table:calculation-settings` for a workbook in the 1904 system and/or iterative calculation; empty for defaults.
pub(crate) fn calculation_settings_xml(stylings: &[SheetStyling]) -> String {
    let Some(first) = stylings.first() else {
        return String::new();
    };
    let iter_xml = ods_iteration_xml(&first.calc_options);
    if !first.date1904 && iter_xml.is_empty() {
        return String::new();
    }
    let null_date = if first.date1904 {
        format!(r#"<table:null-date table:value-type="date" table:date-value="{NULL_DATE_1904}"/>"#)
    } else {
        String::new()
    };
    format!("<table:calculation-settings>{null_date}{iter_xml}</table:calculation-settings>")
}

/// Whether the workbook's null date is 1904-01-01.
pub(crate) fn is_date1904(content_xml: &str) -> bool {
    extract_tags_or_self_closing_with_attrs(content_xml, "table:null-date")
        .into_iter()
        .next()
        .and_then(|(attrs, _)| extract_attr_from_fragment(attrs, "table:date-value"))
        .is_some_and(|date| date.trim() == NULL_DATE_1904)
}

/// Sets `date1904` and iterative calculation settings on the first sheet from `content.xml`.
pub(crate) fn read_date_system(content_xml: &str, stylings: &mut [(String, SheetStyling)]) {
    if let Some((_, first)) = stylings.first_mut() {
        first.date1904 = is_date1904(content_xml);
        read_ods_iteration(content_xml, &mut first.calc_options);
    }
}

//! Parts of the XLSX package: the content types and the part that marks array formulas as dynamic (PD-217).

/// The part that marks the array formulas of a sheet as dynamic: cell metadata type 1 of `XLDAPR`
/// (`cm="1"` on the anchor) with the dynamic array properties (`fDynamic`).
pub(crate) const DYNAMIC_ARRAY_METADATA: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
    "<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" ",
    "xmlns:xda=\"http://schemas.micro",
    "soft.com/office/spreadsheetml/2017/dynamicarray\">",
    "<metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" ",
    "pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" ",
    "clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/></metadataTypes>",
    "<futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\">",
    "<xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata>",
    "<cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata>",
    "</metadata>"
);
pub(crate) const DYNAMIC_ARRAY_OVERRIDE: &str = concat!(
    "<Override PartName=\"/xl/metadata.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml\"/>",
    "</Types>"
);
pub(crate) const DYNAMIC_ARRAY_RELATIONSHIP: &str = concat!(
    "<Relationship Id=\"rIdMeta\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata\" Target=\"metadata.xml\"/>",
    "</Relationships>"
);

pub(crate) fn build_content_types(sheet_count: usize) -> String {
    let mut content_types = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
    ));
    for idx in 1..=sheet_count {
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{idx}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
    }
    content_types.push_str(concat!(
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>",
        "</Types>"
    ));
    content_types
}

use crate::XlsxCachedValue;

/// What one cell holds when it is written: its stored text, the value its formula gave, and the area
/// of the array it spills (PD-217).
pub(crate) struct CellContent<'a> {
    pub(crate) raw: &'a str,
    pub(crate) cached: Option<&'a XlsxCachedValue>,
    pub(crate) spill: Option<&'a crate::SpillRef>,
    /// A cell an array filled: its text is a value, even when it starts with `=`.
    pub(crate) filled: bool,
}

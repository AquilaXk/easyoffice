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

/// Inserts `<pivotCaches>...</pivotCaches>` in `xl/workbook.xml` right before `<extLst` (if
/// present) or before `</workbook>`, preserving ECMA-376 `CT_Workbook` element ordering.
pub(crate) fn insert_workbook_pivot_caches(workbook_xml: &mut String, pivot_caches_xml: &str) {
    if pivot_caches_xml.is_empty() {
        return;
    }
    let pos = workbook_xml
        .find("<extLst")
        .or_else(|| workbook_xml.rfind("</workbook>"))
        .unwrap_or(workbook_xml.len());
    workbook_xml.insert_str(pos, pivot_caches_xml);
}

/// Concatenates non-empty worksheet `<Relationship .../>` XML fragments.
pub(crate) fn combine_sheet_rels(fragments: &[&str]) -> String {
    let total: usize = fragments.iter().map(|s| s.len()).sum();
    let mut out = String::with_capacity(total);
    for frag in fragments {
        out.push_str(frag);
    }
    out
}

/// Resolves an OOXML relationship `target` relative to `dir`, rejecting `..` path traversal.
pub(crate) fn resolve_zip_target(dir: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let joined;
    let path = if let Some(absolute) = target.strip_prefix('/') {
        absolute
    } else {
        joined = if dir.is_empty() {
            target.to_string()
        } else {
            format!("{dir}/{target}")
        };
        &joined
    };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// What one cell holds when it is written: its stored text, the value its formula gave, and the area
/// of the array it spills (PD-217).
pub(crate) struct CellContent<'a> {
    pub(crate) raw: &'a str,
    pub(crate) kind: crate::CellValueKind,
    pub(crate) cached: Option<&'a XlsxCachedValue>,
    pub(crate) spill: Option<&'a crate::SpillRef>,
    /// A cell an array filled: its text is a value, even when it starts with `=`.
    pub(crate) filled: bool,
    /// A table's header cell: written as an inline string whatever its text (PD-224).
    pub(crate) text_only: bool,
}

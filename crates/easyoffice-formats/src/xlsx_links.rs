//! Hyperlinks in SpreadsheetML (PD-218): the `hyperlinks` element of a worksheet part (ECMA-376
//! 18.3.1.47, 18.3.1.48) and the sheet's relationships part, where an address outside the workbook
//! lives as a relationship of type `hyperlink` with `TargetMode="External"` (ECMA-376 Part 2, 9.3).
//! The model is `sheet_links`.
//!
//! `<hyperlink ref="B2" r:id="rId1" tooltip="..."/>` links the cells of `ref` to the relationship's
//! address; `location="Sheet2!A1"` is a jump inside the workbook. A link that has both is the address
//! with `#location` as its fragment. Every address goes through the policy of `sheet_links`: a file's
//! `javascript:`, `file:` or relative address is turned away and counted, never kept.

use std::collections::HashMap;
use std::fmt::Write;

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, format_col_letters,
};
use crate::sheet_links::{LinkAllowance, LinkCollector, SheetLinks, MAX_LINKS_PER_SHEET};
use crate::xml_stream::{XmlEvent, XmlReader};
use crate::{CellBox, XmlSanitizer};

const HYPERLINK_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const REL_NAMESPACE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The addresses of the `hyperlink` relationships of a sheet's relationships part, by relationship id.
pub(crate) fn hyperlink_relationships(rels_xml: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (attrs, _) in extract_balanced_elements(rels_xml, "Relationship") {
        let is_link = extract_attr_from_fragment(attrs, "Type")
            .is_some_and(|kind| kind == HYPERLINK_RELATIONSHIP);
        if let (true, Some(id), Some(target)) = (
            is_link,
            extract_attr_from_fragment(attrs, "Id"),
            extract_attr_from_fragment(attrs, "Target"),
        ) {
            out.insert(id.to_string(), XmlSanitizer::unescape(target));
        }
    }
    out
}

/// The hyperlink relationships of the sheet part `part_path`, by id (empty without a relationships part).
pub(crate) fn sheet_relationships(
    entries: &std::collections::HashMap<&str, &[u8]>,
    part_path: &str,
) -> std::collections::HashMap<String, String> {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    entries
        .get(format!("{dir}/_rels/{file}.rels").as_str())
        .and_then(|data| std::str::from_utf8(data).ok())
        .map(hyperlink_relationships)
        .unwrap_or_default()
}

/// The text of the `hyperlinks` element of a worksheet part, or `None` when it has none.
fn hyperlinks_block(sheet_xml: &str) -> Option<&str> {
    let start = sheet_xml.find("<hyperlinks")?;
    let rest = &sheet_xml[start..];
    let end = rest
        .find("</hyperlinks>")
        .map_or(rest.len(), |at| at + "</hyperlinks>".len());
    Some(&rest[..end])
}

/// The links of a worksheet part. `relationships` are the sheet's hyperlink relationships. A file that
/// lists more than twice [`MAX_LINKS_PER_SHEET`] elements is read no further.
pub(crate) fn read_links(
    sheet_xml: &str,
    relationships: &HashMap<String, String>,
    allowance: &mut LinkAllowance,
) -> LinkCollector {
    let mut got = LinkCollector::default();
    let Some(block) = hyperlinks_block(sheet_xml) else {
        return got;
    };
    let mut reader = XmlReader::unbounded(block);
    let mut seen = 0usize;
    while let Ok(Some(event)) = reader.next_event() {
        let XmlEvent::Start {
            name: "hyperlink",
            attrs,
        } = event
        else {
            continue;
        };
        seen += 1;
        if seen > 2 * MAX_LINKS_PER_SHEET {
            got.rejected = got.rejected.saturating_add(1);
            break;
        }
        let Some(area) = attrs.get("ref").and_then(|text| CellBox::parse(&text)) else {
            got.rejected = got.rejected.saturating_add(1);
            continue;
        };
        let location = attrs.get("location").filter(|text| !text.is_empty());
        let address = attrs
            .get("r:id")
            .and_then(|id| relationships.get(id.as_ref()));
        let target = match (address, location) {
            (Some(base), Some(fragment)) => Some(format!("{base}#{fragment}")),
            (Some(base), None) => Some(base.clone()),
            (None, Some(location)) => Some(format!("#{location}")),
            (None, None) => None,
        };
        let tip = attrs.get("tooltip").unwrap_or_default();
        match target {
            Some(target) => got.add_area(allowance, area, &target, &tip),
            None => got.rejected = got.rejected.saturating_add(1),
        }
    }
    got
}

/// The `hyperlinks` element of a worksheet part and the sheet's relationships part (`None` when no link
/// leaves the workbook). Both are empty strings and `None` for a sheet without links.
pub(crate) fn links_xml(links: &SheetLinks) -> (String, Option<String>) {
    if links.is_empty() {
        return (String::new(), None);
    }
    let mut xml = format!("<hyperlinks xmlns:r=\"{REL_NAMESPACE}\">");
    let mut rels = String::new();
    let mut external = 0usize;
    for (&(row, col), link) in links {
        let _ = write!(
            xml,
            "<hyperlink ref=\"{}{}\"",
            format_col_letters(col),
            row + 1
        );
        match link.target.strip_prefix('#') {
            Some(location) => {
                let _ = write!(xml, " location=\"{}\"", XmlSanitizer::escape(location));
            }
            None => {
                external += 1;
                let _ = write!(xml, " r:id=\"rId{external}\"");
                let _ = write!(
                    rels,
                    "<Relationship Id=\"rId{external}\" Type=\"{HYPERLINK_RELATIONSHIP}\" Target=\"{}\" TargetMode=\"External\"/>",
                    XmlSanitizer::escape(&link.target)
                );
            }
        }
        if !link.tip.is_empty() {
            let _ = write!(xml, " tooltip=\"{}\"", XmlSanitizer::escape(&link.tip));
        }
        xml.push_str("/>");
    }
    xml.push_str("</hyperlinks>");
    let rels = (external > 0).then(|| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"
        )
    });
    (xml, rels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_links::CellLink;

    fn read(xml: &str, rels: &[(&str, &str)]) -> LinkCollector {
        let rels = rels
            .iter()
            .map(|(id, target)| (id.to_string(), target.to_string()))
            .collect();
        read_links(xml, &rels, &mut LinkAllowance::default())
    }

    #[test]
    fn external_internal_and_both_forms_are_read() {
        let got = read(
            r#"<worksheet><sheetData/><hyperlinks>
            <hyperlink ref="A1" r:id="rId1" tooltip="열기 &amp; 이동"/>
            <hyperlink ref="B2" location="Sheet2!A1"/>
            <hyperlink ref="C3" r:id="rId2" location="top"/>
            <hyperlink ref="D4:D5" r:id="rId1"/>
            </hyperlinks></worksheet>"#,
            &[
                ("rId1", "https://example.com/a?b=1&c=2"),
                ("rId2", "https://example.com/p"),
            ],
        );
        assert_eq!(got.rejected, 0);
        assert_eq!(got.links[&(0, 0)].target, "https://example.com/a?b=1&c=2");
        assert_eq!(got.links[&(0, 0)].tip, "열기 & 이동");
        assert_eq!(got.links[&(1, 1)].target, "#Sheet2!A1");
        assert_eq!(got.links[&(2, 2)].target, "https://example.com/p#top");
        assert!(got.links.contains_key(&(3, 3)) && got.links.contains_key(&(4, 3)));
    }

    #[test]
    fn unsafe_unknown_and_malformed_links_are_turned_away_and_counted() {
        let got = read(
            r#"<hyperlinks>
            <hyperlink ref="A1" r:id="rId1"/>
            <hyperlink ref="A2" r:id="rId2"/>
            <hyperlink ref="A3" r:id="rId3"/>
            <hyperlink ref="A4" r:id="rId404"/>
            <hyperlink ref="not a ref" location="Sheet1!A1"/>
            <hyperlink ref="A6"/>
            <hyperlink ref="A7" r:id="rId4"/>
            </hyperlinks>"#,
            &[
                ("rId1", "javascript:alert(1)"),
                ("rId2", "file:///etc/passwd"),
                ("rId3", "../other.xlsx"),
                ("rId4", "https://ok.example/"),
            ],
        );
        assert_eq!(got.links.len(), 1);
        assert_eq!(got.links[&(6, 0)].target, "https://ok.example/");
        assert_eq!(got.rejected, 6);
    }

    #[test]
    fn a_sheet_without_links_reads_none() {
        assert!(read("<worksheet><sheetData/></worksheet>", &[])
            .links
            .is_empty());
    }

    #[test]
    fn written_links_read_back() {
        let mut links = SheetLinks::new();
        links.insert(
            (0, 0),
            CellLink::new("https://example.com/a?b=1&c=2", "한 &\"줄\"").unwrap(),
        );
        links.insert((1, 27), CellLink::new("#'Sheet 2'!A1:B3", "").unwrap());
        links.insert((2, 0), CellLink::new("mailto:me@example.com", "").unwrap());
        let (xml, rels) = links_xml(&links);
        let rels = rels.unwrap();
        assert!(
            xml.contains("ref=\"AB2\"") && xml.contains("location=\"&apos;Sheet 2&apos;!A1:B3\"")
        );
        assert_eq!(rels.matches("TargetMode=\"External\"").count(), 2);
        crate::XmlSanitizer::validate_safe_xml(&xml).unwrap();
        crate::XmlSanitizer::validate_safe_xml(&rels).unwrap();
        let back = read_links(
            &xml,
            &hyperlink_relationships(&rels),
            &mut LinkAllowance::default(),
        );
        assert_eq!(back.rejected, 0);
        assert_eq!(back.links, links);
    }

    #[test]
    fn no_links_write_nothing() {
        assert_eq!(links_xml(&SheetLinks::new()), (String::new(), None));
        let mut links = SheetLinks::new();
        links.insert((0, 0), CellLink::new("#Total", "").unwrap());
        assert!(links_xml(&links).1.is_none());
    }
}

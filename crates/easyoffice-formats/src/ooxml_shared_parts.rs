//! Shared ECMA-376 package parts: core properties, DrawingML theme, and table scaffolding.

use crate::XmlSanitizer;

/// Content type of the OPC core-properties part.
pub(crate) const CORE_PROPERTIES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.core-properties+xml";
/// Relationship type from the package root to the core-properties part.
pub(crate) const CORE_PROPERTIES_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

/// Text area width (twips) of an A4 page with 1-inch side margins.
const DOCX_TEXT_WIDTH_TWIPS: usize = 9026;

/// `docProps/core.xml` carrying the document title (ECMA-376 Part 2 §11).
pub(crate) fn core_properties_xml(title: &str) -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" ",
            "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" ",
            "xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
            "<dc:title>{}</dc:title><dc:creator>EasyOffice</dc:creator>",
            "</cp:coreProperties>"
        ),
        XmlSanitizer::escape(title)
    )
}

/// Writes `w:tblPr` and `w:tblGrid`, which must precede the first `w:tr` (ECMA-376 §17.4.38).
pub(crate) fn push_docx_table_properties(out: &mut String, rows: &[Vec<String>]) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    out.push_str(concat!(
        "<w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders>",
        "<w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
        "<w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
        "<w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
        "<w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
        "<w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
        "<w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
        "</w:tblBorders><w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" ",
        "w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/></w:tblPr>"
    ));
    let width = DOCX_TEXT_WIDTH_TWIPS / cols;
    out.push_str("<w:tblGrid>");
    for _ in 0..cols {
        out.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
    }
    out.push_str("</w:tblGrid>");
}

/// `p:clrMap` mapping the theme's color slots (required on slide and notes masters).
pub(crate) const PML_CLR_MAP: &str = concat!(
    "<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" ",
    "accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>"
);

/// Empty group-shape header every `p:spTree` must start with.
pub(crate) const PML_SP_TREE_HEADER: &str = concat!(
    "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>",
    "<p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/>",
    "<a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>"
);

fn solid_fill_style(scheme: &str) -> String {
    format!("<a:solidFill><a:schemeClr val=\"{scheme}\"/></a:solidFill>")
}

/// Minimal complete DrawingML theme (`a:theme`) with color, font, and format schemes.
pub(crate) fn drawingml_theme_xml(name: &str) -> String {
    let mut xml = String::with_capacity(4096);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>");
    xml.push_str(&format!(
        "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"{}\"><a:themeElements>",
        XmlSanitizer::escape(name)
    ));
    xml.push_str(concat!(
        "<a:clrScheme name=\"EasyOffice\">",
        "<a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1>",
        "<a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>",
        "<a:dk2><a:srgbClr val=\"1C1F2E\"/></a:dk2>",
        "<a:lt2><a:srgbClr val=\"EEF0F6\"/></a:lt2>",
        "<a:accent1><a:srgbClr val=\"4F5BD5\"/></a:accent1>",
        "<a:accent2><a:srgbClr val=\"7A5AF8\"/></a:accent2>",
        "<a:accent3><a:srgbClr val=\"2E9E6B\"/></a:accent3>",
        "<a:accent4><a:srgbClr val=\"D98E04\"/></a:accent4>",
        "<a:accent5><a:srgbClr val=\"C2410C\"/></a:accent5>",
        "<a:accent6><a:srgbClr val=\"0E7490\"/></a:accent6>",
        "<a:hlink><a:srgbClr val=\"2563EB\"/></a:hlink>",
        "<a:folHlink><a:srgbClr val=\"7C3AED\"/></a:folHlink>",
        "</a:clrScheme>",
        "<a:fontScheme name=\"EasyOffice\">",
        "<a:majorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>",
        "<a:font script=\"Hang\" typeface=\"맑은 고딕\"/></a:majorFont>",
        "<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>",
        "<a:font script=\"Hang\" typeface=\"맑은 고딕\"/></a:minorFont>",
        "</a:fontScheme>",
        "<a:fmtScheme name=\"EasyOffice\"><a:fillStyleLst>"
    ));
    let fill = solid_fill_style("phClr");
    for _ in 0..3 {
        xml.push_str(&fill);
    }
    xml.push_str("</a:fillStyleLst><a:lnStyleLst>");
    for w in [6350, 12700, 19050] {
        xml.push_str(&format!(
            "<a:ln w=\"{w}\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\">{fill}<a:prstDash val=\"solid\"/><a:miter lim=\"800000\"/></a:ln>"
        ));
    }
    xml.push_str("</a:lnStyleLst><a:effectStyleLst>");
    for _ in 0..3 {
        xml.push_str("<a:effectStyle><a:effectLst/></a:effectStyle>");
    }
    xml.push_str("</a:effectStyleLst><a:bgFillStyleLst>");
    for _ in 0..3 {
        xml.push_str(&fill);
    }
    xml.push_str("</a:bgFillStyleLst></a:fmtScheme></a:themeElements>");
    xml.push_str("<a:objectDefaults/><a:extraClrSchemeLst/></a:theme>");
    xml
}

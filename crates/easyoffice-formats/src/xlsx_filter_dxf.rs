//! The colours of the differential formats (`dxf`) of `styles.xml` that a colour filter names
//! (ECMA-376 18.3.2.3 `colorFilter@dxfId`, 18.8.14 `dxf`): the fill colour (`fill/patternFill`, the
//! solid colour is `bgColor`) and the text colour (`font/color`) of each entry.
//!
//! A colour the file gives as an RGB value, a theme slot with a tint or a palette number becomes
//! `0xRRGGBB` (`xlsx_color`); `auto`, a `patternType` of `none`, or no such element is "no colour of
//! its own"; a colour the reader cannot resolve makes the filter that names it dropped. The writer
//! adds the entries a workbook's colour filters need to the same `dxfs` list the conditional formats
//! use (`xlsx_rules::DxfTable`).

use crate::rtf_xml_helpers::{extract_tag_bodies, extract_tags_or_self_closing_with_attrs};
use crate::sheet_styling::xlsx_color::{ColorScheme, XlsxColor};

/// One colour of a differential format.
pub(crate) type DxfColor = XlsxColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DxfColors {
    pub(crate) fill: DxfColor,
    pub(crate) font: DxfColor,
}

fn first_color(body: &str, tags: &[&str], colors: &ColorScheme) -> DxfColor {
    tags.iter()
        .find_map(|tag| {
            extract_tags_or_self_closing_with_attrs(body, tag)
                .first()
                .map(|(attrs, _)| colors.resolve(attrs))
        })
        .unwrap_or(DxfColor::Auto)
}

/// `dxf` entries one reader looks at (the bound the colour filters had): a stylesheet that lists more is read up to it.
pub(crate) const MAX_DXFS: usize = 4_096;

/// The bodies of the `dxf` entries of `styles_xml`, in file order, an empty `<dxf/>` as `""` (it still takes an id: a
/// `dxfId` counts every entry), read once for every reader of the list and up to [`MAX_DXFS`] entries.
pub(crate) fn dxf_bodies(styles_xml: &str) -> Vec<&str> {
    let Some(block) = extract_tag_bodies(styles_xml, "dxfs").into_iter().next() else {
        return Vec::new();
    };
    let bytes = block.as_bytes();
    let mut out = Vec::new();
    let mut at = 0usize;
    while out.len() < MAX_DXFS {
        let Some(found) = block[at..].find("<dxf") else {
            break;
        };
        let name_end = at + found + 4;
        // `<dxf` must be the whole name (not `<dxfs`).
        if !matches!(
            bytes.get(name_end),
            Some(b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/')
        ) {
            at = name_end;
            continue;
        }
        let Some(close) = block[name_end..].find('>') else {
            break;
        };
        let tag_end = name_end + close;
        if bytes[tag_end - 1] == b'/' {
            out.push("");
            at = tag_end + 1;
            continue;
        }
        let Some(end) = block[tag_end + 1..].find("</dxf>") else {
            break;
        };
        out.push(&block[tag_end + 1..tag_end + 1 + end]);
        at = tag_end + 1 + end + "</dxf>".len();
    }
    out
}

/// The colours of the `dxf` entries `dxfs` ([`dxf_bodies`]), in file order.
pub(crate) fn parse_dxf_colors(dxfs: &[&str], colors: &ColorScheme) -> Vec<DxfColors> {
    dxfs.iter()
        .map(|dxf| {
            let fill = extract_tag_bodies(dxf, "fill").into_iter().next();
            let none = fill.is_some_and(|f| f.contains("patternType=\"none\""));
            DxfColors {
                fill: match fill {
                    Some(body) if !none => first_color(body, &["bgColor", "fgColor"], colors),
                    _ => DxfColor::Auto,
                },
                font: extract_tag_bodies(dxf, "font")
                    .into_iter()
                    .next()
                    .map_or(DxfColor::Auto, |body| first_color(body, &["color"], colors)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_styling::xlsx_view::{apply_tint, DEFAULT_THEME};

    #[test]
    fn an_empty_dxf_takes_an_id_and_the_list_stops_at_the_bound() {
        let xml = "<styleSheet><dxfs count=\"4\"><dxf><font><b/></font></dxf><dxf/><dxf ></dxf><dxf><fill/></dxf></dxfs></styleSheet>";
        assert_eq!(dxf_bodies(xml), ["<font><b/></font>", "", "", "<fill/>"]);
        assert!(dxf_bodies("<styleSheet><dxfs count=\"0\"/></styleSheet>").is_empty());
        let flood = format!(
            "<styleSheet><dxfs>{}</dxfs></styleSheet>",
            "<dxf><b/></dxf>".repeat(MAX_DXFS + 10)
        );
        assert_eq!(dxf_bodies(&flood).len(), MAX_DXFS);
    }

    #[test]
    fn fill_and_font_colours_of_a_dxf() {
        let xml = concat!(
            "<styleSheet><dxfs count=\"5\">",
            "<dxf><fill><patternFill><bgColor rgb=\"FFFFC7CE\"/></patternFill></fill></dxf>",
            "<dxf><font><color rgb=\"FF9C0006\"/></font></dxf>",
            "<dxf><fill><patternFill patternType=\"none\"><bgColor auto=\"1\"/></patternFill></fill></dxf>",
            "<dxf><fill><patternFill><bgColor theme=\"5\" tint=\"0.5\"/></patternFill></fill></dxf>",
            "<dxf><font><color indexed=\"10\"/></font></dxf>",
            "</dxfs></styleSheet>"
        );
        let dxfs = parse_dxf_colors(&dxf_bodies(xml), &ColorScheme::default());
        assert_eq!(dxfs.len(), 5);
        assert_eq!(dxfs[0].fill, DxfColor::Rgb(0xFFC7CE));
        assert_eq!(dxfs[0].font, DxfColor::Auto);
        assert_eq!(dxfs[1].font, DxfColor::Rgb(0x9C0006));
        assert_eq!(dxfs[2].fill, DxfColor::Auto);
        assert_eq!(
            dxfs[3].fill,
            DxfColor::Rgb(apply_tint(DEFAULT_THEME[5], 0.5))
        );
        assert_eq!(
            dxfs[4].font,
            DxfColor::Rgb(0xFF0000),
            "a palette number resolves"
        );
        assert!(parse_dxf_colors(&dxf_bodies("<styleSheet/>"), &ColorScheme::default()).is_empty());
    }
}

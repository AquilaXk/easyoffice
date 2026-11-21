//! The `border` of a differential format (`dxf`, ECMA-376 18.8.14 and 18.8.4): the four sides a
//! conditional format can set, read from and written into the `dxfs` list of `styles.xml`.
//!
//! 구조 참조 (reference implementation, structure only): IronCalc `xlsx/src/import/styles.rs`
//! `parse_dxf` lines 485-640 and `base/src/types.rs` `Dxf` lines 560-567, MIT OR Apache-2.0,
//! Copyright (c) IronCalc contributors, commit f93662006b8fb1c401dc870bbda26c5d60ac6037;
//! umya-spreadsheet `src/structs/differential_format.rs`, MIT, Copyright (c) 2020 MathNya, commit
//! 4c8c4a4027200c146eb9e17d68cae93453f8e046. Both keep the `dxf` border in the same type as a cell
//! border, so the model here is `CellBorders` too. No code is copied: the edge reader and the edge
//! writer are the ones of the cell borders (`xlsx_styles`).
//!
//! A side the `dxf` does not name stays as the cell has it. `diagonal`, `vertical` and `horizontal`
//! (the inner lines of a table style) do not apply to a rule and are not read. A line kind the
//! editor cannot draw folds onto the nearest one (`hair` reads as thin, `dashDot` as dashed);
//! the writer gives the kinds the editor has back. The colour is `rgb`, a theme slot with its tint or
//! an `indexed` entry; automatic and unreadable colours read as the default (black).

use crate::rtf_xml_helpers::extract_tag_bodies;
use crate::sheet_styling::sheet_look::{BorderEdge, CellBorders, BORDER_DEFAULT_RGB};
use crate::sheet_styling::xlsx_color::ColorScheme;
use crate::sheet_styling::xlsx_styles::parse_border_edge;

/// One side of a border as SpreadsheetML writes it (`left`, `right`, `top`, `bottom`); `None` for no edge.
pub(crate) fn edge_xml(tag: &str, edge: BorderEdge) -> Option<String> {
    let style = edge.line().xlsx()?;
    Some(format!(
        "<{tag} style=\"{style}\"><color rgb=\"FF{:06X}\"/></{tag}>",
        edge.rgb()
    ))
}

/// The `<border>` element of a differential format, "" when no side is set. A side with no edge is
/// left out (an empty side would read as "no line" where the cell has one).
pub(crate) fn dxf_border_xml(borders: CellBorders) -> String {
    let sides = [
        ("left", borders.left),
        ("right", borders.right),
        ("top", borders.top),
        ("bottom", borders.bottom),
    ];
    if borders.is_none() {
        return String::new();
    }
    let mut xml = String::from("<border>");
    for (tag, edge) in sides {
        if let Some(side) = edge_xml(tag, edge) {
            xml.push_str(&side);
        }
    }
    xml.push_str("</border>");
    xml
}

/// The sides of the `border` of the `dxf` body `dxf`; none when it has no border element.
pub(crate) fn parse_dxf_border(dxf: &str, colors: &ColorScheme) -> CellBorders {
    let Some(body) = extract_tag_bodies(dxf, "border").into_iter().next() else {
        return CellBorders::default();
    };
    let color = |attrs: &str| colors.resolve(attrs).rgb().unwrap_or(BORDER_DEFAULT_RGB);
    CellBorders {
        top: parse_border_edge(body, "top", color),
        right: parse_border_edge(body, "right", color),
        bottom: parse_border_edge(body, "bottom", color),
        left: parse_border_edge(body, "left", color),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_styling::sheet_look::BorderLine;
    use crate::sheet_styling::xlsx_view::{apply_tint, DEFAULT_THEME};

    fn read(dxf: &str) -> CellBorders {
        parse_dxf_border(dxf, &ColorScheme::default())
    }

    fn edge(line: BorderLine, rgb: u32) -> BorderEdge {
        BorderEdge::new(line, rgb)
    }

    // Entries of the `dxfs` list of workbooks a spreadsheet program wrote (test files `issue_281.xlsx` and `issue_216.xlsx`
    // of the library in the header, MIT; listed in THIRD_PARTY_NOTICES.md): theme colours with a tint, an automatic colour, sides left empty, and a `horizontal` line.
    #[test]
    fn dxf_borders_a_spreadsheet_program_wrote_are_read() {
        let top_bottom = read(concat!(
            "<border><top style=\"thin\"><color theme=\"6\" tint=\"0.79998168889431442\"/></top>",
            "<bottom style=\"thin\"><color theme=\"6\" tint=\"0.79998168889431442\"/></bottom></border>"
        ));
        let tinted = apply_tint(DEFAULT_THEME[6], 0.799_981_688_894_314_4);
        assert_eq!(top_bottom.top, edge(BorderLine::Thin, tinted));
        assert_eq!(top_bottom.bottom, edge(BorderLine::Thin, tinted));
        assert!(top_bottom.left.is_none() && top_bottom.right.is_none());

        let auto = read(
            "<fill><patternFill><bgColor theme=\"0\"/></patternFill></fill><border><left/><right/><top/><bottom style=\"thin\"><color auto=\"1\"/></bottom></border>",
        );
        assert_eq!(auto.bottom, edge(BorderLine::Thin, BORDER_DEFAULT_RGB));
        assert!(auto.top.is_none() && auto.left.is_none() && auto.right.is_none());

        // The inner line of a table style does not apply to a rule.
        let inner =
            read("<border><horizontal style=\"thin\"><color theme=\"6\"/></horizontal></border>");
        assert!(inner.is_none());
    }

    #[test]
    fn line_kinds_fold_onto_the_ones_the_editor_draws() {
        for (word, line) in [
            ("hair", BorderLine::Thin),
            ("medium", BorderLine::Thick),
            ("dashDot", BorderLine::Dashed),
            ("mediumDashDotDot", BorderLine::Dashed),
            ("dotted", BorderLine::Dotted),
            ("double", BorderLine::Double),
        ] {
            let b = read(&format!(
                "<border><left style=\"{word}\"><color rgb=\"FF112233\"/></left></border>"
            ));
            assert_eq!(b.left, edge(line, 0x112233), "{word}");
        }
    }

    #[test]
    fn a_written_border_reads_back_and_leaves_unset_sides_out() {
        let borders = CellBorders {
            top: edge(BorderLine::Thin, 0xFF0000),
            left: edge(BorderLine::Double, 0x0000FF),
            ..CellBorders::NONE
        };
        let xml = dxf_border_xml(borders);
        assert_eq!(
            xml,
            concat!(
                "<border><left style=\"double\"><color rgb=\"FF0000FF\"/></left>",
                "<top style=\"thin\"><color rgb=\"FFFF0000\"/></top></border>"
            )
        );
        assert_eq!(read(&xml), borders);
        assert_eq!(dxf_border_xml(CellBorders::NONE), "");
    }

    #[test]
    fn values_that_are_unknown_or_over_a_bound_are_not_edges() {
        let long = "x".repeat(100_000);
        for dxf in [
            "<border><top style=\"wavy\"><color rgb=\"FFFF0000\"/></top></border>".to_string(),
            "<border><top style=\"\"><color rgb=\"FFFF0000\"/></top></border>".to_string(),
            format!("<border><top style=\"thin{long}\"><color rgb=\"FFFF0000\"/></top></border>"),
            "<border><top><color rgb=\"FFFF0000\"/></top></border>".to_string(),
            "<border/>".to_string(),
            String::new(),
        ] {
            assert!(read(&dxf).is_none(), "{}", &dxf[..dxf.len().min(60)]);
        }
        // A colour that cannot be read, or that is over its bound, is the automatic one, never a
        // partial value: the edge keeps its line and takes the default colour.
        for color in [
            "rgb=\"ZZZZZZZZ\"".to_string(),
            "rgb=\"FF12\"".to_string(),
            "theme=\"99\"".to_string(),
            "theme=\"1\" tint=\"7\"".to_string(),
            "indexed=\"-3\"".to_string(),
            format!("rgb=\"FFFF0000\" {}=\"{long}\"", "pad"),
        ] {
            let b = read(&format!(
                "<border><top style=\"thin\"><color {color}/></top></border>"
            ));
            assert_eq!(
                b.top,
                edge(BorderLine::Thin, BORDER_DEFAULT_RGB),
                "{color:.40}"
            );
        }
    }

    #[test]
    fn only_the_first_border_of_a_dxf_counts() {
        let b = read(concat!(
            "<border><top style=\"thin\"><color rgb=\"FFFF0000\"/></top></border>",
            "<border><top style=\"double\"><color rgb=\"FF00FF00\"/></top></border>"
        ));
        assert_eq!(b.top, edge(BorderLine::Thin, 0xFF0000));
    }
}

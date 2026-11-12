//! Alignment, wrapping and borders survive XLSX/ODS save -> reopen, are shared across cells,
//! and stay bounded on hostile stylesheets (PD-207).

use easyoffice_formats::{
    read_sheet_styling, BorderEdge, BorderLine, CellBorders, CellStyle, DocumentFormat, HAlign,
    OdfCodec, OdsUnpacked, OoxmlCodec, SheetStyling, VAlign, ZipArchiveCodec, ZipSecurityPolicy,
};

const BOTH: [DocumentFormat; 2] = [DocumentFormat::Xlsx, DocumentFormat::Ods];

fn sheet() -> Vec<OdsUnpacked> {
    vec![(
        "Sheet1".to_string(),
        vec![(0, 0, "가".to_string()), (1, 1, "나".to_string())],
    )]
}

fn pack(format: DocumentFormat, styling: &SheetStyling) -> Vec<u8> {
    let styles = std::slice::from_ref(styling);
    match format {
        DocumentFormat::Xlsx => OoxmlCodec::pack_xlsx_multi_styled(&sheet(), &[], styles).unwrap(),
        _ => OdfCodec::pack_ods_multi_styled(&sheet(), styles).unwrap(),
    }
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn with(cells: Vec<((u32, u32), CellStyle)>) -> SheetStyling {
    SheetStyling {
        cells: cells.into_iter().collect(),
        ..Default::default()
    }
}

fn edge(line: BorderLine, rgb: u32) -> BorderEdge {
    BorderEdge::new(line, rgb)
}

fn round_trip(format: DocumentFormat, styling: &SheetStyling) -> SheetStyling {
    let bytes = pack(format, styling);
    read_sheet_styling(&bytes, format).unwrap().remove(0).1
}

#[test]
fn all_nine_alignments_round_trip() {
    for format in BOTH {
        let mut cells = Vec::new();
        for (r, h) in [HAlign::Left, HAlign::Center, HAlign::Right]
            .into_iter()
            .enumerate()
        {
            for (c, v) in [VAlign::Top, VAlign::Center, VAlign::Bottom]
                .into_iter()
                .enumerate()
            {
                cells.push((
                    (r as u32, c as u32),
                    CellStyle {
                        h_align: h,
                        v_align: v,
                        ..Default::default()
                    },
                ));
            }
        }
        let st = with(cells);
        assert_eq!(round_trip(format, &st).cells, st.cells, "{format:?}");
    }
}

#[test]
fn wrap_alone_and_default_alignment_round_trip() {
    for format in BOTH {
        let st = with(vec![
            (
                (0, 0),
                CellStyle {
                    wrap: true,
                    ..Default::default()
                },
            ),
            (
                (1, 1),
                CellStyle {
                    bold: true,
                    ..Default::default()
                },
            ),
        ]);
        let back = round_trip(format, &st);
        assert!(back.cells[&(0, 0)].wrap, "{format:?}");
        assert_eq!(back.cells, st.cells, "{format:?}");
    }
}

#[test]
fn every_border_line_and_side_round_trips() {
    let lines = [
        BorderLine::Thin,
        BorderLine::Thick,
        BorderLine::Dashed,
        BorderLine::Dotted,
        BorderLine::Double,
    ];
    for format in BOTH {
        let cells = lines
            .iter()
            .enumerate()
            .map(|(i, &line)| {
                let e = edge(line, 0x0010_2030 + i as u32);
                let borders = CellBorders {
                    top: e,
                    right: BorderEdge::NONE,
                    bottom: e,
                    left: edge(BorderLine::Thin, 0xFF0000),
                };
                (
                    (i as u32, 0),
                    CellStyle {
                        borders,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let st = with(cells);
        assert_eq!(round_trip(format, &st).cells, st.cells, "{format:?}");
    }
}

#[test]
fn xlsx_shares_one_border_and_one_xf_among_equal_cells() {
    let style = CellStyle {
        h_align: HAlign::Center,
        wrap: true,
        borders: CellBorders {
            top: edge(BorderLine::Thin, 0),
            right: edge(BorderLine::Thin, 0),
            bottom: edge(BorderLine::Thin, 0),
            left: edge(BorderLine::Thin, 0),
        },
        ..Default::default()
    };
    let cells = (0..50).map(|r| ((r, 0), style)).collect();
    let bytes = pack(DocumentFormat::Xlsx, &with(cells));
    let styles = part(&bytes, "xl/styles.xml");
    assert!(styles.contains("<borders count=\"2\">"), "{styles}");
    assert!(styles.contains("<cellXfs count=\"2\">"), "{styles}");
    assert!(styles.contains("<alignment horizontal=\"center\" wrapText=\"1\"/>"));
    assert!(styles.contains("borderId=\"1\"") && styles.contains("applyBorder=\"1\""));
}

#[test]
fn xlsx_reader_folds_foreign_border_kinds_and_colors() {
    let style = CellStyle {
        borders: CellBorders {
            top: edge(BorderLine::Thin, 0),
            ..Default::default()
        },
        ..Default::default()
    };
    let st = with(vec![((0, 0), style)]);
    let bytes = pack(DocumentFormat::Xlsx, &st);
    let styles = part(&bytes, "xl/styles.xml");
    let foreign = styles
        .replace(
            "<top style=\"thin\"><color rgb=\"FF000000\"/></top>",
            "<top style=\"mediumDashDot\"><color theme=\"1\"/></top>",
        )
        .replace(
            "<left/>",
            "<left style=\"hair\"><color rgb=\"FF336699\"/></left>",
        );
    assert_ne!(foreign, styles);
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let swapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/styles.xml" {
                (e.path, foreign.clone().into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let rebuilt = ZipArchiveCodec::pack_compressed(&swapped).unwrap();
    let back = read_sheet_styling(&rebuilt, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    let borders = back.cells[&(0, 0)].borders;
    assert_eq!(borders.top, edge(BorderLine::Dashed, 0));
    assert_eq!(borders.left, edge(BorderLine::Thin, 0x336699));
}

#[test]
fn xlsx_reader_ignores_border_definitions_beyond_the_limit() {
    let count = 65_536 + 10;
    let mut borders = String::new();
    for _ in 0..count {
        borders.push_str("<border><left style=\"thin\"/><right/><top/><bottom/></border>");
    }
    let style = CellStyle {
        bold: true,
        ..Default::default()
    };
    let bytes = pack(DocumentFormat::Xlsx, &with(vec![((0, 0), style)]));
    let styles = part(&bytes, "xl/styles.xml");
    let start = styles.find("<borders").unwrap();
    let end = styles.find("</borders>").unwrap() + "</borders>".len();
    let hostile = format!(
        "{}<borders count=\"{count}\">{borders}</borders>{}",
        &styles[..start],
        &styles[end..]
    )
    .replace(
        "borderId=\"0\" xfId=\"0\" applyFont",
        "borderId=\"65540\" xfId=\"0\" applyFont",
    );
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let swapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/styles.xml" {
                (e.path, hostile.clone().into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let rebuilt = ZipArchiveCodec::pack_compressed(&swapped).unwrap();
    let back = read_sheet_styling(&rebuilt, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    assert!(back.cells[&(0, 0)].borders.is_none());
    assert!(back.cells[&(0, 0)].bold);
}

#[test]
fn ods_reads_shorthand_border_with_side_override_and_bad_values() {
    let st = with(vec![(
        (0, 0),
        CellStyle {
            bold: true,
            ..Default::default()
        },
    )]);
    let bytes = pack(DocumentFormat::Ods, &st);
    let content = part(&bytes, "content.xml");
    let hostile = content.replace(
        "<style:text-properties",
        "<style:table-cell-properties fo:border=\"0.74pt solid #ff0000\" fo:border-left=\"none\" style:vertical-align=\"sideways\" fo:wrap-option=\"no-wrap\"/><style:paragraph-properties fo:text-align=\"end\"/><style:text-properties",
    );
    assert_ne!(hostile, content);
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let swapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "content.xml" {
                (e.path, hostile.clone().into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let rebuilt = ZipArchiveCodec::pack_compressed(&swapped).unwrap();
    let style = read_sheet_styling(&rebuilt, DocumentFormat::Ods)
        .unwrap()
        .remove(0)
        .1
        .cells[&(0, 0)];
    assert_eq!(style.borders.top, edge(BorderLine::Thin, 0xFF0000));
    assert_eq!(style.borders.right, edge(BorderLine::Thin, 0xFF0000));
    assert_eq!(style.borders.left, BorderEdge::NONE);
    assert_eq!(style.h_align, HAlign::Right);
    assert_eq!(style.v_align, VAlign::Bottom);
    assert!(!style.wrap);
}

#[test]
fn look_does_not_change_the_font_or_number_format_parts() {
    let plain_font = CellStyle {
        bold: true,
        ..Default::default()
    };
    let looked = CellStyle {
        bold: true,
        h_align: HAlign::Right,
        borders: CellBorders {
            top: edge(BorderLine::Double, 0x123456),
            ..Default::default()
        },
        ..Default::default()
    };
    let st = with(vec![((0, 0), plain_font), ((1, 1), looked)]);
    let bytes = pack(DocumentFormat::Xlsx, &st);
    let styles = part(&bytes, "xl/styles.xml");
    // Both cells share one bold font; only the xf differs.
    assert!(styles.contains("<fonts count=\"2\">"), "{styles}");
    assert!(styles.contains("<cellXfs count=\"3\">"), "{styles}");
}

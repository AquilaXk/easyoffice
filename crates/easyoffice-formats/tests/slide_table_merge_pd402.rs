//! PD-402: merged table cells (`gridSpan`, `rowSpan`, `hMerge`, `vMerge`) survive a PPTX and an ODP round trip, and
//! hostile spans in a file never index outside the grid.

use easyoffice_formats::{
    ElementFx, OdfCodec, OoxmlCodec, RichSlide, RichSlideDeck, SlideBox, SlideElement, SlideExt,
    SlideMasterLayout, TableSpan, ZipArchiveCodec, ZipSecurityPolicy,
};

fn span(row: u16, col: u16, row_span: u16, col_span: u16) -> TableSpan {
    TableSpan {
        row,
        col,
        row_span,
        col_span,
    }
}

fn deck(rows: Vec<Vec<String>>, spans: Vec<TableSpan>) -> RichSlideDeck {
    let mut fx = ElementFx::new("tbl");
    fx.spans = spans;
    RichSlideDeck {
        slides: vec![RichSlide {
            layout: SlideMasterLayout::Blank,
            elements: vec![SlideElement::Table {
                id: "tbl".into(),
                rect: SlideBox {
                    x: 40.0,
                    y: 60.0,
                    w: 400.0,
                    h: 200.0,
                },
                rows,
                header_row: true,
                fill: None,
            }],
            ext: SlideExt {
                fx: vec![fx],
                ..SlideExt::default()
            },
            ..RichSlide::default()
        }],
        ..RichSlideDeck::default()
    }
}

fn grid() -> Vec<Vec<String>> {
    (0..3)
        .map(|r| (0..4).map(|c| format!("r{r}c{c}")).collect())
        .collect()
}

fn spans_of(deck: &RichSlideDeck) -> Vec<TableSpan> {
    deck.slides[0]
        .ext
        .fx_of("tbl")
        .map_or_else(Vec::new, |f| f.spans.clone())
}

fn slide_xml(pptx: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(pptx, &ZipSecurityPolicy::default()).unwrap();
    let e = entries
        .iter()
        .find(|e| e.path == "ppt/slides/slide1.xml")
        .unwrap();
    String::from_utf8(e.data.clone()).unwrap()
}

#[test]
fn merges_are_written_with_the_origin_and_the_covered_marks_and_read_back() {
    // The PptxGenJS example: A0 rowspan 3, B1 rowspan 2, C1 colspan 2.
    let spans = vec![span(0, 0, 3, 1), span(1, 1, 2, 1), span(1, 2, 1, 2)];
    let src = deck(grid(), spans.clone());
    let bytes = OoxmlCodec::pack_pptx_rich(&src, false).unwrap();
    let xml = slide_xml(&bytes);
    assert_eq!(xml.matches("rowSpan=\"3\"").count(), 1);
    assert_eq!(xml.matches("rowSpan=\"2\"").count(), 1);
    assert_eq!(xml.matches("gridSpan=\"2\"").count(), 1);
    assert_eq!(xml.matches("vMerge=\"1\"").count(), 3, "A1 A2 B2");
    assert_eq!(xml.matches("hMerge=\"1\"").count(), 1, "D1");
    assert!(!xml.contains("r1c0"), "a covered cell loses its text");
    let back = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
    assert_eq!(spans_of(&back), spans);
    let SlideElement::Table { rows, .. } = &back.slides[0].elements[0] else {
        panic!("table")
    };
    assert_eq!(rows[0][0], "r0c0");
    assert_eq!(rows[1][1], "r1c1");
    assert_eq!(rows[1][0], "");
}

#[test]
fn spans_survive_odp() {
    let spans = vec![span(0, 0, 2, 2), span(2, 1, 1, 3)];
    let back =
        OdfCodec::unpack_odp_rich(&OdfCodec::pack_odp_rich(&deck(grid(), spans.clone())).unwrap())
            .unwrap();
    assert_eq!(spans_of(&back), spans);
}

#[test]
fn a_table_without_merges_writes_none() {
    let bytes = OoxmlCodec::pack_pptx_rich(&deck(grid(), vec![]), false).unwrap();
    let xml = slide_xml(&bytes);
    assert!(!xml.contains("gridSpan") && !xml.contains("Merge="));
}

#[test]
fn hostile_spans_are_dropped_on_write_and_on_read() {
    let hostile = vec![
        span(0, 0, 2, 2),
        span(1, 1, 2, 2),
        span(0, 3, 1, 9),
        span(9, 9, 9, 9),
        span(0, 0, 0, 0),
        span(u16::MAX, u16::MAX, u16::MAX, u16::MAX),
    ];
    let bytes = OoxmlCodec::pack_pptx_rich(&deck(grid(), hostile), false).unwrap();
    let back = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
    assert_eq!(spans_of(&back), vec![span(0, 0, 2, 2)]);

    // A file whose gridSpan runs past the grid, or whose rowSpan is huge, keeps no span.
    let xml = slide_xml(&bytes).replace("gridSpan=\"2\"", "gridSpan=\"60000\" rowSpan=\"60000\"");
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let edited: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "ppt/slides/slide1.xml" {
                (e.path, xml.clone().into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let back = OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack(&edited).unwrap()).unwrap();
    assert!(spans_of(&back).is_empty());
}

//! PD-402: the chart kinds doughnut, radar, bubble, stock and combo survive XLSX, PPTX and ODP round trips, draw into a PDF,
//! and hostile chart parts stay inside their limits.

use easyoffice_formats::{
    encode_slide_pdf, ChartAnchor, ChartKind, ChartLegendPos, ChartSeriesRef, OdfCodec,
    OdsUnpacked, OoxmlCodec, RichSlide, RichSlideDeck, SheetChart, SheetStyling, SlideBox,
    SlideChartSeries, SlideElement, SlideMasterLayout, ZipArchiveCodec, ZipSecurityPolicy,
};

const NEW_KINDS: [ChartKind; 5] = [
    ChartKind::Doughnut,
    ChartKind::Radar,
    ChartKind::Bubble,
    ChartKind::Stock,
    ChartKind::Combo,
];

fn sheets() -> Vec<OdsUnpacked> {
    let mut cells = vec![(0, 0, "x".to_string())];
    for (c, head) in ["시가", "고가", "저가", "종가"].iter().enumerate() {
        cells.push((0, c as u32 + 1, (*head).to_string()));
    }
    for r in 1..=4u32 {
        cells.push((r, 0, r.to_string()));
        for (c, v) in [10, 14, 8, 12].iter().enumerate() {
            cells.push((r, c as u32 + 1, (v + r as i32).to_string()));
        }
    }
    vec![("S".to_string(), cells)]
}

fn series_ref(name: &str, col: char) -> ChartSeriesRef {
    ChartSeriesRef {
        name: name.to_string(),
        name_ref: Some(format!("S!${col}$1")),
        categories_ref: Some("S!$A$2:$A$5".to_string()),
        values_ref: format!("S!${col}$2:${col}$5"),
    }
}

fn sheet_chart(kind: ChartKind, series: Vec<ChartSeriesRef>) -> SheetChart {
    SheetChart {
        id: format!("c-{}", kind.as_str()),
        kind,
        title: kind.as_str().to_string(),
        data_range: "A1:E5".to_string(),
        switch_row_col: false,
        legend: ChartLegendPos::Right,
        anchor: ChartAnchor::default(),
        series,
    }
}

fn chart_parts(bytes: &[u8]) -> Vec<String> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .filter(|e| e.path.starts_with("xl/charts/chart"))
        .map(|e| String::from_utf8(e.data).unwrap())
        .collect()
}

#[test]
fn the_new_kinds_round_trip_through_xlsx_with_their_plot_groups() {
    let four = vec![
        series_ref("시가", 'B'),
        series_ref("고가", 'C'),
        series_ref("저가", 'D'),
        series_ref("종가", 'E'),
    ];
    let styling = SheetStyling {
        charts: vec![
            sheet_chart(ChartKind::Doughnut, four[..1].to_vec()),
            sheet_chart(ChartKind::Radar, four[..2].to_vec()),
            sheet_chart(ChartKind::Bubble, four[..2].to_vec()),
            sheet_chart(ChartKind::Stock, four.clone()),
            sheet_chart(ChartKind::Combo, four[..3].to_vec()),
        ],
        ..SheetStyling::default()
    };
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[styling]).unwrap();
    let parts = chart_parts(&bytes);
    assert_eq!(parts.len(), 5);
    let has = |i: usize, needles: &[&str]| {
        for n in needles {
            assert!(parts[i].contains(n), "chart {i} lacks {n}: {}", parts[i]);
        }
    };
    has(
        0,
        &[
            "<c:doughnutChart>",
            "<c:holeSize val=\"50\"/>",
            "</c:doughnutChart>",
        ],
    );
    has(
        1,
        &["<c:radarChart><c:radarStyle val=\"marker\"/>", "<c:catAx>"],
    );
    has(
        2,
        &[
            "<c:bubbleChart>",
            "<c:xVal>",
            "<c:bubbleSize><c:numRef><c:f>S!$C$2:$C$5</c:f>",
            "<c:bubbleScale val=\"100\"/>",
        ],
    );
    has(
        3,
        &[
            "<c:stockChart>",
            "<c:hiLowLines/><c:upDownBars>",
            "<a:noFill/>",
        ],
    );
    has(4, &["<c:barChart>", "<c:lineChart>"]);
    assert_eq!(
        parts[2].matches("<c:ser>").count(),
        1,
        "a bubble pair is one series"
    );
    assert_eq!(parts[3].matches("<c:ser>").count(), 4);
    assert_eq!(
        parts[4].matches("<c:axId val=\"64052224\"/>").count(),
        3,
        "the bar group, the line group and the one category axis they share"
    );

    let (_, out) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    let charts = &out[0].1.charts;
    assert_eq!(charts.len(), 5);
    let kinds: Vec<_> = charts.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, NEW_KINDS);
    assert_eq!(charts[0].series.len(), 1);
    assert_eq!(charts[1].series.len(), 2);
    assert_eq!(
        charts[2].series.len(),
        2,
        "the sizes come back as the pair's second series"
    );
    assert_eq!(charts[2].series[1].values_ref, "S!$C$2:$C$5");
    assert_eq!(charts[3].series.len(), 4);
    assert_eq!(charts[4].series.len(), 3);
}

fn slide_deck(kind: ChartKind, series: Vec<SlideChartSeries>) -> RichSlideDeck {
    RichSlideDeck {
        slides: vec![RichSlide {
            layout: SlideMasterLayout::Blank,
            elements: vec![SlideElement::Chart {
                id: "ch".into(),
                rect: SlideBox {
                    x: 40.0,
                    y: 40.0,
                    w: 400.0,
                    h: 300.0,
                },
                chart_type: kind,
                title: "차트".into(),
                categories: vec!["1".into(), "2".into(), "3".into()],
                series,
                legend: ChartLegendPos::Bottom,
            }],
            ..RichSlide::default()
        }],
        ..RichSlideDeck::default()
    }
}

fn s(name: &str, values: &[f64]) -> SlideChartSeries {
    SlideChartSeries {
        name: name.into(),
        values: values.to_vec(),
    }
}

fn chart_of(deck: &RichSlideDeck) -> (ChartKind, Vec<SlideChartSeries>) {
    match &deck.slides[0].elements[0] {
        SlideElement::Chart {
            chart_type, series, ..
        } => (*chart_type, series.clone()),
        other => panic!("{other:?}"),
    }
}

fn four() -> Vec<SlideChartSeries> {
    vec![
        s("시가", &[10.0, 11.0, 12.0]),
        s("고가", &[14.0, 15.0, 16.0]),
        s("저가", &[8.0, 9.0, 7.0]),
        s("종가", &[12.0, 10.0, 13.0]),
    ]
}

#[test]
fn the_new_kinds_round_trip_through_pptx_odp_and_draw_into_a_pdf() {
    for kind in NEW_KINDS {
        let series = match kind {
            ChartKind::Doughnut => four()[..1].to_vec(),
            ChartKind::Bubble => vec![s("Y", &[3.0, 5.0, 2.0]), s("Y (크기)", &[10.0, 20.0, 5.0])],
            ChartKind::Stock => four(),
            _ => four()[..2].to_vec(),
        };
        let deck = slide_deck(kind, series.clone());
        let back = OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap())
            .unwrap();
        let (k, got) = chart_of(&back);
        assert_eq!(k, kind, "PPTX {kind:?}");
        assert_eq!(got, series, "PPTX {kind:?}");

        let odp = OdfCodec::unpack_odp_rich(&OdfCodec::pack_odp_rich(&deck).unwrap()).unwrap();
        let expected = if kind == ChartKind::Combo {
            ChartKind::Column
        } else {
            kind
        };
        assert_eq!(chart_of(&odp).0, expected, "ODP {kind:?}");

        assert!(
            !encode_slide_pdf(&deck, "차트").unwrap().is_empty(),
            "PDF {kind:?}"
        );
    }
}

#[test]
fn a_bar_group_of_many_series_stays_a_column_chart() {
    let deck = slide_deck(ChartKind::Column, four());
    let (k, got) = chart_of(
        &OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap()).unwrap(),
    );
    assert_eq!((k, got.len()), (ChartKind::Column, 4));
}

fn with_chart_xml(deck: &RichSlideDeck, edit: impl Fn(String) -> String) -> Vec<u8> {
    let bytes = OoxmlCodec::pack_pptx_rich(deck, false).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let edited: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "ppt/charts/chart1.xml" {
                (
                    e.path,
                    edit(String::from_utf8(e.data).unwrap()).into_bytes(),
                )
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack(&edited).unwrap()
}

#[test]
fn hostile_chart_parts_are_refused() {
    let bubble = slide_deck(
        ChartKind::Bubble,
        vec![s("Y", &[3.0, 5.0, 2.0]), s("Y (크기)", &[10.0, 20.0, 5.0])],
    );
    let ok = with_chart_xml(&bubble, |x| x);
    assert!(OoxmlCodec::unpack_pptx_rich(&ok).is_ok());
    // A bubble size that is not a number.
    let bad = with_chart_xml(&bubble, |x| {
        x.replace("<c:v>20</c:v>", "<c:v>NaN-ish</c:v>")
    });
    assert!(OoxmlCodec::unpack_pptx_rich(&bad).is_err());
    // A point count past the limit in the size slot.
    let many = with_chart_xml(&bubble, |x| {
        x.replacen("<c:ptCount val=\"3\"/>", "<c:ptCount val=\"99999\"/>", 3)
    });
    assert!(OoxmlCodec::unpack_pptx_rich(&many).is_err());
    // Many bubble series, each with sizes, pass the series limit.
    let one_ser_start = bubble_ser(&ok);
    let flood = with_chart_xml(&bubble, |x| {
        let rep = one_ser_start.repeat(20);
        x.replacen(&one_ser_start, &rep, 1)
    });
    assert!(OoxmlCodec::unpack_pptx_rich(&flood).is_err());
}

#[test]
fn a_bubble_series_without_sizes_is_not_paired_with_the_next_one() {
    let deck = slide_deck(
        ChartKind::Bubble,
        vec![
            s("A", &[1.0, 2.0, 3.0]),
            s("A (크기)", &[7.0, 8.0, 9.0]),
            s("B", &[4.0, 5.0, 6.0]),
            s("B (크기)", &[1.0, 2.0, 3.0]),
        ],
    );
    // Cut the `c:bubbleSize` of the first series out of the part.
    let cut = with_chart_xml(&deck, |x| {
        let start = x.find("<c:bubbleSize>").unwrap();
        let end = x.find("</c:bubbleSize>").unwrap() + "</c:bubbleSize>".len();
        format!("{}{}", &x[..start], &x[end..])
    });
    let read = OoxmlCodec::unpack_pptx_rich(&cut).unwrap();
    let (kind, series) = chart_of(&read);
    assert_eq!(kind, ChartKind::Bubble);
    assert_eq!(
        series.len(),
        4,
        "the first series keeps an empty size series of its own"
    );
    assert!(series[1].values.is_empty());
    assert_eq!(series[2].values, vec![4.0, 5.0, 6.0], "B is still B");
    assert_eq!(series[3].values, vec![1.0, 2.0, 3.0]);
    let back =
        OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&read, false).unwrap()).unwrap();
    assert_eq!(
        chart_of(&back).1,
        series,
        "writing and reading again changes nothing"
    );
}

fn bubble_ser(pptx: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(pptx, &ZipSecurityPolicy::default()).unwrap();
    let xml = String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == "ppt/charts/chart1.xml")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    let start = xml.find("<c:ser>").unwrap();
    let end = xml.find("</c:ser>").unwrap() + "</c:ser>".len();
    xml[start..end].to_string()
}

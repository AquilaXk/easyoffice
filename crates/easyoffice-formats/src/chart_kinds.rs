//! Plot groups of every [`ChartKind`] in DrawingML chart XML (`c:barChart`, `c:lineChart`, `c:pieChart`, `c:doughnutChart`,
//! `c:scatterChart`, `c:radarChart`, `c:bubbleChart`, `c:stockChart`), shared by the slide chart writer and the XLSX chart
//! writer (`PD-402`). A kind is one plot group, a combo is two (a bar group for its first series and a line group for the
//! rest) over the same axes.
//!
//! Reference implementation (3-step check): (1) the rust_xlsxwriter crate the chart writers already follow (MIT OR Apache-2.0,
//! `src/chart.rs`, see `THIRD_PARTY_NOTICES.md`) has every one of these chart types, but it only writes, from its own model, and
//! this crate keeps categories and series in the model of `sheet_chart.rs`; (2) the existing writers' header, tail and axis
//! tables are the base this extends; (3) the element order inside each group is ECMA-376 Part 1 21.2.2 (`CT_RadarChart`:
//! `radarStyle`, `varyColors`, `ser`, `axId`; `CT_BubbleChart`: `varyColors`, `ser`, `bubbleScale`, `showNegBubbles`, `axId`;
//! `CT_StockChart`: `ser`, `hiLowLines`, `upDownBars`, `axId`; `CT_DoughnutChart`: `varyColors`, `ser`, `firstSliceAng`,
//! `holeSize`), the same order rust_xlsxwriter writes. Open, high, low and close of a stock chart are its four series in that
//! order (three series are high, low and close), the layout the stock chart types of the Korean spreadsheet program read.

use crate::ooxml_presentation::{AX_ID_CAT, AX_ID_VAL};
use crate::sheet_chart::ChartKind;

/// Axes a plot group sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axes {
    /// A pie or a doughnut has none.
    None,
    /// A category axis (`pos` of it) and a value axis (`pos`).
    CatVal {
        cat_pos: &'static str,
        val_pos: &'static str,
    },
    /// Two value axes (scatter and bubble).
    ValVal,
}

/// One plot group: its element, the children before the series, the children between the series and the axis ids, and the
/// emitted series (a range of the series the writer lists; a bubble group lists its pairs).
#[derive(Debug, Clone)]
pub(crate) struct PlotGroup {
    pub tag: &'static str,
    pub header: &'static str,
    pub tail: &'static str,
    pub series: std::ops::Range<usize>,
    pub has_axes: bool,
}

pub(crate) const MAX_STOCK_SERIES: usize = 4;

const COL: &str = "<c:barDir val=\"col\"/><c:grouping val=\"clustered\"/>";
const BAR: &str = "<c:barDir val=\"bar\"/><c:grouping val=\"clustered\"/>";
const STOCK_HILO: &str = "<c:hiLowLines/>";
const STOCK_OHLC: &str =
    "<c:hiLowLines/><c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>";

/// The axes of a kind.
pub(crate) fn axes_of(kind: ChartKind) -> Axes {
    match kind {
        ChartKind::Pie | ChartKind::Doughnut => Axes::None,
        ChartKind::Scatter | ChartKind::Bubble => Axes::ValVal,
        ChartKind::Bar => Axes::CatVal {
            cat_pos: "l",
            val_pos: "b",
        },
        _ => Axes::CatVal {
            cat_pos: "b",
            val_pos: "l",
        },
    }
}

/// How many `c:ser` a kind writes for `n` series in the model: a bubble reads Y values and sizes in pairs.
pub(crate) fn emitted_series(kind: ChartKind, n: usize) -> usize {
    if kind == ChartKind::Bubble {
        n.div_ceil(2)
    } else {
        n
    }
}

/// The plot groups of `kind` for `n` model series.
pub(crate) fn plot_groups(kind: ChartKind, n: usize) -> Vec<PlotGroup> {
    let all = 0..emitted_series(kind, n);
    let one = |tag, header, tail, has_axes| {
        vec![PlotGroup {
            tag,
            header,
            tail,
            series: all.clone(),
            has_axes,
        }]
    };
    match kind {
        ChartKind::Column => one("c:barChart", COL, "", true),
        ChartKind::Bar => one("c:barChart", BAR, "", true),
        ChartKind::Line => one(
            "c:lineChart",
            "<c:grouping val=\"standard\"/>",
            "<c:marker val=\"1\"/>",
            true,
        ),
        ChartKind::Pie => one(
            "c:pieChart",
            "<c:varyColors val=\"1\"/>",
            "<c:firstSliceAng val=\"0\"/>",
            false,
        ),
        ChartKind::Doughnut => one(
            "c:doughnutChart",
            "<c:varyColors val=\"1\"/>",
            "<c:firstSliceAng val=\"0\"/><c:holeSize val=\"50\"/>",
            false,
        ),
        ChartKind::Scatter => one(
            "c:scatterChart",
            "<c:scatterStyle val=\"lineMarker\"/>",
            "",
            true,
        ),
        ChartKind::Radar => one(
            "c:radarChart",
            "<c:radarStyle val=\"marker\"/><c:varyColors val=\"0\"/>",
            "",
            true,
        ),
        ChartKind::Bubble => one(
            "c:bubbleChart",
            "<c:varyColors val=\"0\"/>",
            "<c:bubbleScale val=\"100\"/><c:showNegBubbles val=\"0\"/>",
            true,
        ),
        ChartKind::Stock => one(
            "c:stockChart",
            "",
            if n >= MAX_STOCK_SERIES {
                STOCK_OHLC
            } else {
                STOCK_HILO
            },
            true,
        ),
        ChartKind::Combo => {
            let split = n.min(1);
            vec![
                PlotGroup {
                    tag: "c:barChart",
                    header: COL,
                    tail: "",
                    series: 0..split,
                    has_axes: true,
                },
                PlotGroup {
                    tag: "c:lineChart",
                    header: "<c:grouping val=\"standard\"/>",
                    tail: "<c:marker val=\"1\"/>",
                    series: split..n,
                    has_axes: true,
                },
            ]
        }
    }
}

/// The axes of a kind's plot area: a category axis and a value axis, two value axes, or none.
pub(crate) fn axes_xml(kind: ChartKind) -> String {
    let (cat, val) = (AX_ID_CAT, AX_ID_VAL);
    let ax = |tag: &str, id: &str, cross: &str, pos: &str, grid: &str| {
        format!("<c:{tag}><c:axId val=\"{id}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{pos}\"/>{grid}<c:crossAx val=\"{cross}\"/></c:{tag}>")
    };
    match axes_of(kind) {
        Axes::None => String::new(),
        Axes::CatVal { cat_pos, val_pos } => {
            ax("catAx", cat, val, cat_pos, "")
                + &ax("valAx", val, cat, val_pos, "<c:majorGridlines/>")
        }
        Axes::ValVal => {
            ax("valAx", cat, val, "b", "") + &ax("valAx", val, cat, "l", "<c:majorGridlines/>")
        }
    }
}

/// `c:spPr` and `c:marker` of a stock series: the series draws no line and no marker, the high-low lines and the bars do.
pub(crate) const STOCK_SERIES_LOOK: &str =
    "<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr><c:marker><c:symbol val=\"none\"/></c:marker>";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_groups_over_all_its_series() {
        for kind in ChartKind::ALL {
            let groups = plot_groups(kind, 5);
            let covered: usize = groups.iter().map(|g| g.series.len()).sum();
            assert_eq!(covered, emitted_series(kind, 5), "{kind:?}");
            assert!(!groups.is_empty());
        }
        assert_eq!(emitted_series(ChartKind::Bubble, 3), 2);
        let combo = plot_groups(ChartKind::Combo, 3);
        assert_eq!(
            (combo[0].series.clone(), combo[1].series.clone()),
            (0..1, 1..3)
        );
        assert!(plot_groups(ChartKind::Stock, 4)[0]
            .tail
            .contains("upDownBars"));
        assert!(!plot_groups(ChartKind::Stock, 3)[0]
            .tail
            .contains("upDownBars"));
        assert_eq!(plot_groups(ChartKind::Combo, 0)[0].series, 0..0);
    }
}

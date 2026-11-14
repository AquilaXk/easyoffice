//! `autoFilter` of a worksheet part: reading it from the event stream and writing it back
//! (ECMA-376 18.3.2.1 `autoFilter`, 18.3.2.7 `filterColumn`, 18.3.2.8 `filters`, 18.3.2.4
//! `customFilters`). The model is `sheet_filter`; the rows a filter hides are `row hidden`, which
//! the sheet reader keeps with the user's hidden rows until a caller that has the cells tells the
//! two apart.
//!
//! Besides the value list and the custom conditions the model holds the date groups (18.3.2.4
//! `dateGroupItem`), the top-ten filter (18.3.2.10), the above and below average filters (18.3.2.5
//! `dynamicFilter`) and the colour filter (18.3.2.3 `colorFilter`, whose `dxfId` names a differential
//! format of `styles.xml`, see `xlsx_filter_dxf`).
//!
//! A filter column the model cannot hold (the other dynamic filters, icon filters, a colour the file
//! gives as a palette index, more than two custom conditions) is dropped; the rows it hid stay hidden
//! as ordinary hidden rows. A range or a list over the bounds of `sheet_filter` drops the whole filter.

use crate::sheet_filter::{
    CustomCondition, CustomOp, DateGroup, DateUnit, FilterColumn, FilterCriteria, SheetFilter,
    MAX_FILTER_COLUMNS, MAX_FILTER_DATE_GROUPS, MAX_FILTER_VALUES, MAX_TOP_ITEMS, MAX_TOP_PERCENT,
};
use crate::sheet_styling::sheet_rules::xlsx_rules::DxfTable;
use crate::sheet_styling::xlsx_filter_dxf::{DxfColor, DxfColors};
use crate::xml_stream::{Attrs, XmlEvent};
use crate::{CellBox, XmlSanitizer};
use std::fmt::Write;

/// What the column element being read holds so far.
enum Open {
    None,
    Values {
        blank: bool,
        values: Vec<String>,
        dates: Vec<DateGroup>,
    },
    Custom {
        and: bool,
        conditions: Vec<CustomCondition>,
    },
    /// A column with a criterion that needs no list: top-ten, average, colour.
    Single(FilterCriteria),
    /// A kind of filter the model cannot hold.
    Unsupported,
}

pub(crate) struct FilterScan<'a> {
    /// Inside the `autoFilter` element; every other element of the sheet is skipped at once.
    inside: bool,
    filter: Option<SheetFilter>,
    col: Option<u32>,
    open: Open,
    /// The colours of the workbook's differential formats, what a `colorFilter` names.
    dxfs: &'a [DxfColors],
}

fn flag(attrs: &Attrs<'_>, name: &str) -> bool {
    matches!(attrs.get(name).as_deref(), Some("1" | "true"))
}

/// A boolean attribute whose default is true.
fn flag_or_true(attrs: &Attrs<'_>, name: &str) -> bool {
    !matches!(attrs.get(name).as_deref(), Some("0" | "false"))
}

impl<'a> FilterScan<'a> {
    pub(crate) const fn new(dxfs: &'a [DxfColors]) -> Self {
        Self {
            inside: false,
            filter: None,
            col: None,
            open: Open::None,
            dxfs,
        }
    }

    /// The filter read, or `None` when there is none or it was over the bounds.
    pub(crate) fn finish(self) -> Option<SheetFilter> {
        self.filter.filter(SheetFilter::within_bounds)
    }

    /// The filter as read, with the bounds not applied (a table keeps its conditions whatever the
    /// range's height).
    pub(crate) fn finish_unbounded(self) -> Option<SheetFilter> {
        self.filter
    }

    pub(crate) fn feed(&mut self, event: &XmlEvent<'_>) {
        match event {
            XmlEvent::Start { name, attrs } if self.inside || *name == "autoFilter" => {
                self.start(name, attrs);
            }
            XmlEvent::End("filterColumn") => self.end_column(),
            XmlEvent::End("autoFilter") => self.inside = false,
            _ => {}
        }
    }

    fn start(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "autoFilter" if self.filter.is_none() => {
                self.inside = true;
                self.filter = attrs
                    .get("ref")
                    .and_then(|r| CellBox::parse(&r))
                    .map(SheetFilter::new);
            }
            "filterColumn" => {
                self.open = Open::None;
                self.col = None;
                let (Some(filter), Some(id)) = (
                    self.filter.as_ref(),
                    attrs.get("colId").and_then(|v| v.parse::<u32>().ok()),
                ) else {
                    return;
                };
                self.col = filter
                    .range
                    .col0
                    .checked_add(id)
                    .filter(|&c| c <= filter.range.col1);
            }
            _ if self.col.is_none() => {}
            "filters" => {
                // Only the Gregorian calendar has the groups this model names.
                let calendar = attrs.get("calendarType");
                self.open = if matches!(calendar.as_deref(), None | Some("none" | "gregorian")) {
                    Open::Values {
                        blank: flag(attrs, "blank"),
                        values: Vec::new(),
                        dates: Vec::new(),
                    }
                } else {
                    Open::Unsupported
                };
            }
            "filter" => {
                if let (Open::Values { values, .. }, Some(val)) = (&mut self.open, attrs.get("val"))
                {
                    if values.len() <= MAX_FILTER_VALUES {
                        values.push(val.into_owned());
                    }
                }
            }
            "dateGroupItem" => self.date_group(attrs),
            "top10" => self.open = top_ten(attrs),
            "dynamicFilter" => {
                self.open = match attrs.get("type").as_deref() {
                    Some("aboveAverage") => Open::Single(FilterCriteria::Average { above: true }),
                    Some("belowAverage") => Open::Single(FilterCriteria::Average { above: false }),
                    _ => Open::Unsupported,
                };
            }
            "colorFilter" => self.open = self.color_filter(attrs),
            "customFilters" => {
                self.open = Open::Custom {
                    and: flag(attrs, "and"),
                    conditions: Vec::new(),
                };
            }
            "customFilter" => self.custom_filter(attrs),
            "iconFilter" => self.open = Open::Unsupported,
            _ => {}
        }
    }

    fn date_group(&mut self, attrs: &Attrs<'_>) {
        let Open::Values { dates, .. } = &mut self.open else {
            return;
        };
        match date_group_of(attrs) {
            Some(group) if dates.len() <= MAX_FILTER_DATE_GROUPS => dates.push(group),
            Some(_) => {}
            None => self.open = Open::Unsupported,
        }
    }

    fn color_filter(&self, attrs: &Attrs<'_>) -> Open {
        let font = !flag_or_true(attrs, "cellColor");
        let colors = attrs
            .get("dxfId")
            .and_then(|id| id.parse::<usize>().ok())
            .and_then(|id| self.dxfs.get(id));
        let picked = colors.map(|c| if font { c.font } else { c.fill });
        match picked {
            Some(DxfColor::Rgb(rgb)) => Open::Single(FilterCriteria::Color {
                font,
                color: Some(rgb),
            }),
            Some(DxfColor::Auto) => Open::Single(FilterCriteria::Color { font, color: None }),
            _ => Open::Unsupported,
        }
    }

    fn custom_filter(&mut self, attrs: &Attrs<'_>) {
        let Open::Custom { conditions, .. } = &mut self.open else {
            return;
        };
        let op = match attrs.get("operator") {
            None => None,
            Some(name) => match CustomOp::from_name(&name) {
                Some(op) => Some(op),
                None => {
                    self.open = Open::Unsupported;
                    return;
                }
            },
        };
        conditions.push(CustomCondition {
            op,
            text: attrs.get("val").map(|v| v.into_owned()).unwrap_or_default(),
        });
    }

    fn end_column(&mut self) {
        let open = std::mem::replace(&mut self.open, Open::None);
        let (Some(filter), Some(col)) = (self.filter.as_mut(), self.col.take()) else {
            return;
        };
        let criteria = match open {
            Open::Values {
                blank,
                values,
                dates,
            } if blank || !values.is_empty() || !dates.is_empty() => FilterCriteria::Values {
                blank,
                values,
                dates,
            },
            Open::Single(criteria) => criteria,
            Open::Custom { and, conditions } => {
                let mut it = conditions.into_iter();
                match (it.next(), it.next(), it.next()) {
                    (Some(first), second, None) => FilterCriteria::Custom { and, first, second },
                    _ => return,
                }
            }
            _ => return,
        };
        if filter.columns.len() < MAX_FILTER_COLUMNS + 1 && filter.column(col).is_none() {
            let at = filter.columns.partition_point(|c| c.col < col);
            filter.columns.insert(at, FilterColumn { col, criteria });
        }
    }
}

/// A `dateGroupItem`: `None` for one that names a day that does not exist or a unit it gives no
/// fields for.
fn date_group_of(attrs: &Attrs<'_>) -> Option<DateGroup> {
    let number = |name: &str| attrs.get(name).and_then(|v| v.trim().parse::<u16>().ok());
    let small = |name: &str| number(name).and_then(|v| u8::try_from(v).ok());
    let unit = DateUnit::from_name(attrs.get("dateTimeGrouping")?.as_ref())?;
    let group = DateGroup {
        unit,
        year: number("year")?,
        month: if unit >= DateUnit::Month {
            small("month")?
        } else {
            0
        },
        day: if unit >= DateUnit::Day {
            small("day")?
        } else {
            0
        },
        hour: if unit >= DateUnit::Hour {
            small("hour")?
        } else {
            0
        },
        minute: if unit >= DateUnit::Minute {
            small("minute")?
        } else {
            0
        },
        second: if unit >= DateUnit::Second {
            small("second")?
        } else {
            0
        },
    };
    group.is_valid().then_some(group)
}

/// A `top10`: a whole number of items or percent, inside the bounds of the model.
fn top_ten(attrs: &Attrs<'_>) -> Open {
    let percent = flag(attrs, "percent");
    let most = if percent {
        MAX_TOP_PERCENT
    } else {
        MAX_TOP_ITEMS
    };
    attrs
        .get("val")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.fract() == 0.0 && (1.0..=f64::from(most)).contains(v))
        .map_or(Open::Unsupported, |v| {
            Open::Single(FilterCriteria::Top {
                bottom: !flag_or_true(attrs, "top"),
                percent,
                count: v as u32,
            })
        })
}

fn date_group_xml(out: &mut String, g: &DateGroup) {
    let _ = write!(out, "<dateGroupItem year=\"{}\"", g.year);
    let fields = [
        (DateUnit::Month, "month", g.month),
        (DateUnit::Day, "day", g.day),
        (DateUnit::Hour, "hour", g.hour),
        (DateUnit::Minute, "minute", g.minute),
        (DateUnit::Second, "second", g.second),
    ];
    for (unit, name, value) in fields {
        if g.unit >= unit {
            let _ = write!(out, " {name}=\"{value}\"");
        }
    }
    let _ = write!(out, " dateTimeGrouping=\"{}\"/>", g.unit.name());
}

fn custom_filter_xml(out: &mut String, c: &CustomCondition) {
    out.push_str("<customFilter");
    if let Some(op) = c.op {
        let _ = write!(out, " operator=\"{}\"", op.name());
    }
    let _ = write!(out, " val=\"{}\"/>", XmlSanitizer::escape(&c.text));
}

/// `<autoFilter>` for `filter`; a colour filter names its differential format in `dxf`.
pub(crate) fn autofilter_xml(filter: &SheetFilter, dxf: &DxfTable) -> String {
    let mut out = format!("<autoFilter ref=\"{}\"", filter.range.to_a1());
    if filter.columns.is_empty() {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    for column in &filter.columns {
        let _ = write!(
            out,
            "<filterColumn colId=\"{}\">",
            column.col - filter.range.col0
        );
        match &column.criteria {
            FilterCriteria::Values {
                blank,
                values,
                dates,
            } => {
                out.push_str("<filters");
                if *blank {
                    out.push_str(" blank=\"1\"");
                }
                if values.is_empty() && dates.is_empty() {
                    out.push_str("/>");
                } else {
                    out.push('>');
                    for value in values {
                        let _ = write!(out, "<filter val=\"{}\"/>", XmlSanitizer::escape(value));
                    }
                    for group in dates {
                        date_group_xml(&mut out, group);
                    }
                    out.push_str("</filters>");
                }
            }
            FilterCriteria::Top {
                bottom,
                percent,
                count,
            } => {
                out.push_str("<top10");
                if *bottom {
                    out.push_str(" top=\"0\"");
                }
                if *percent {
                    out.push_str(" percent=\"1\"");
                }
                let _ = write!(out, " val=\"{count}\"/>");
            }
            FilterCriteria::Average { above } => {
                let kind = if *above {
                    "aboveAverage"
                } else {
                    "belowAverage"
                };
                let _ = write!(out, "<dynamicFilter type=\"{kind}\"/>");
            }
            FilterCriteria::Color { font, color } => {
                // The workbook's differential formats were built from the same filters, so the colour has one.
                let id = dxf.color_id(*font, *color).unwrap_or(0);
                let _ = write!(out, "<colorFilter dxfId=\"{id}\"");
                if *font {
                    out.push_str(" cellColor=\"0\"");
                }
                out.push_str("/>");
            }
            FilterCriteria::Custom { and, first, second } => {
                out.push_str(if *and {
                    "<customFilters and=\"1\">"
                } else {
                    "<customFilters>"
                });
                custom_filter_xml(&mut out, first);
                if let Some(second) = second {
                    custom_filter_xml(&mut out, second);
                }
                out.push_str("</customFilters>");
            }
        }
        out.push_str("</filterColumn>");
    }
    out.push_str("</autoFilter>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_stream::XmlReader;

    fn scan_with(xml: &str, dxfs: &[DxfColors]) -> Option<SheetFilter> {
        let mut scan = FilterScan::new(dxfs);
        let mut reader = XmlReader::unbounded(xml);
        while let Some(event) = reader.next_event().unwrap() {
            scan.feed(&event);
        }
        scan.finish()
    }

    fn scan(xml: &str) -> Option<SheetFilter> {
        scan_with(xml, &[])
    }

    fn column(col: u32, criteria: FilterCriteria) -> FilterColumn {
        FilterColumn { col, criteria }
    }

    fn group(unit: DateUnit, year: u16, month: u8, day: u8) -> DateGroup {
        DateGroup {
            unit,
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }

    #[test]
    fn values_and_custom_columns_round_trip_through_the_xml() {
        let mut filter = SheetFilter::new(CellBox::parse("B2:E40").unwrap());
        filter.columns = vec![
            column(
                1,
                FilterCriteria::Values {
                    blank: true,
                    values: vec!["a&b".into(), "<x>".into(), "한글".into()],
                    dates: Vec::new(),
                },
            ),
            column(
                3,
                FilterCriteria::Custom {
                    and: true,
                    first: CustomCondition {
                        op: Some(CustomOp::GreaterThanOrEqual),
                        text: "10".into(),
                    },
                    second: Some(CustomCondition {
                        op: None,
                        text: "*\"q\"*".into(),
                    }),
                },
            ),
        ];
        let xml = autofilter_xml(&filter, &DxfTable::default());
        assert!(xml.starts_with("<autoFilter ref=\"B2:E40\"><filterColumn colId=\"0\">"));
        assert!(xml.contains("<filterColumn colId=\"2\"><customFilters and=\"1\">"));
        assert_eq!(scan(&xml), Some(filter));
    }

    #[test]
    fn date_groups_top_ten_average_and_colour_round_trip() {
        let mut filter = SheetFilter::new(CellBox::parse("A1:F9").unwrap());
        let time = DateGroup {
            unit: DateUnit::Second,
            year: 2024,
            month: 2,
            day: 29,
            hour: 23,
            minute: 59,
            second: 58,
        };
        filter.columns = vec![
            column(
                0,
                FilterCriteria::Values {
                    blank: true,
                    values: vec!["text".into()],
                    dates: vec![
                        group(DateUnit::Year, 2023, 0, 0),
                        group(DateUnit::Month, 2024, 3, 0),
                        group(DateUnit::Day, 2024, 2, 29),
                        time,
                    ],
                },
            ),
            column(
                1,
                FilterCriteria::Top {
                    bottom: false,
                    percent: false,
                    count: 10,
                },
            ),
            column(
                2,
                FilterCriteria::Top {
                    bottom: true,
                    percent: true,
                    count: 25,
                },
            ),
            column(3, FilterCriteria::Average { above: true }),
            column(4, FilterCriteria::Average { above: false }),
            column(
                5,
                FilterCriteria::Color {
                    font: false,
                    color: Some(0xFFC7CE),
                },
            ),
        ];
        let colors = [(false, Some(0xFFC7CE))];
        let dxf = DxfTable::default().with_colors(colors);
        let xml = autofilter_xml(&filter, &dxf);
        assert!(
            xml.contains("<dateGroupItem year=\"2023\" dateTimeGrouping=\"year\"/>"),
            "{xml}"
        );
        assert!(
            xml.contains("<dateGroupItem year=\"2024\" month=\"3\" dateTimeGrouping=\"month\"/>")
        );
        assert!(
            xml.contains("<top10 val=\"10\"/>")
                && xml.contains("<top10 top=\"0\" percent=\"1\" val=\"25\"/>")
        );
        assert!(xml.contains("<dynamicFilter type=\"aboveAverage\"/>"));
        assert!(xml.contains("<colorFilter dxfId=\"0\"/>"));
        let dxfs = [DxfColors {
            fill: DxfColor::Rgb(0xFFC7CE),
            font: DxfColor::Auto,
        }];
        assert_eq!(scan_with(&xml, &dxfs), Some(filter));
    }

    #[test]
    fn a_colour_filter_names_the_font_or_the_fill_and_no_colour_is_auto() {
        let dxfs = [
            DxfColors {
                fill: DxfColor::Auto,
                font: DxfColor::Rgb(0x9C0006),
            },
            DxfColors {
                fill: DxfColor::Unknown,
                font: DxfColor::Auto,
            },
        ];
        let xml = |attrs: &str| {
            format!("<autoFilter ref=\"A1:B9\"><filterColumn colId=\"0\"><colorFilter {attrs}/></filterColumn></autoFilter>")
        };
        let only = |attrs: &str| {
            scan_with(&xml(attrs), &dxfs)
                .unwrap()
                .columns
                .pop()
                .map(|c| c.criteria)
        };
        assert_eq!(
            only("dxfId=\"0\" cellColor=\"0\""),
            Some(FilterCriteria::Color {
                font: true,
                color: Some(0x9C0006)
            })
        );
        assert_eq!(
            only("dxfId=\"0\""),
            Some(FilterCriteria::Color {
                font: false,
                color: None
            })
        );
        // An unresolved colour, a missing format and a bad id drop the column.
        assert_eq!(only("dxfId=\"1\""), None);
        assert_eq!(only("dxfId=\"9\""), None);
        assert_eq!(only("dxfId=\"x\""), None);
        let dxf = DxfTable::default().with_colors([(true, None), (false, None)]);
        let mut filter = SheetFilter::new(CellBox::parse("A1:B9").unwrap());
        filter.columns = vec![column(
            0,
            FilterCriteria::Color {
                font: true,
                color: None,
            },
        )];
        assert!(
            autofilter_xml(&filter, &dxf).contains("<colorFilter dxfId=\"0\" cellColor=\"0\"/>")
        );
    }

    #[test]
    fn a_filter_without_columns_and_unsupported_columns() {
        let plain = SheetFilter::new(CellBox::parse("A1:C9").unwrap());
        assert_eq!(
            autofilter_xml(&plain, &DxfTable::default()),
            "<autoFilter ref=\"A1:C9\"/>"
        );
        assert_eq!(scan("<autoFilter ref=\"A1:C9\"/>"), Some(plain));
        let xml = concat!(
            "<autoFilter ref=\"A1:C9\">",
            "<filterColumn colId=\"0\"><dynamicFilter type=\"today\"/></filterColumn>",
            "<filterColumn colId=\"1\"><filters calendarType=\"hijri\"><dateGroupItem year=\"1445\" dateTimeGrouping=\"year\"/></filters></filterColumn>",
            "<filterColumn colId=\"2\"><customFilters><customFilter operator=\"equal\" val=\"1\"/>",
            "<customFilter val=\"2\"/><customFilter val=\"3\"/></customFilters></filterColumn>",
            "</autoFilter>"
        );
        assert_eq!(scan(xml).unwrap().columns, Vec::new());
    }

    #[test]
    fn malformed_groups_and_counts_drop_their_column() {
        let one = |inner: &str| {
            let xml = format!(
                "<autoFilter ref=\"A1:B9\"><filterColumn colId=\"0\">{inner}</filterColumn></autoFilter>"
            );
            scan(&xml).unwrap().columns.len()
        };
        assert_eq!(one("<filters><dateGroupItem year=\"2024\" month=\"2\" day=\"30\" dateTimeGrouping=\"day\"/></filters>"), 0);
        assert_eq!(one("<filters><dateGroupItem year=\"2023\" month=\"2\" day=\"29\" dateTimeGrouping=\"day\"/></filters>"), 0);
        assert_eq!(
            one("<filters><dateGroupItem year=\"1800\" dateTimeGrouping=\"year\"/></filters>"),
            0
        );
        assert_eq!(
            one("<filters><dateGroupItem year=\"2024\" dateTimeGrouping=\"month\"/></filters>"),
            0
        );
        assert_eq!(
            one("<filters><dateGroupItem year=\"2024\" dateTimeGrouping=\"week\"/></filters>"),
            0
        );
        assert_eq!(
            one("<filters><dateGroupItem year=\"2024\" dateTimeGrouping=\"year\"/></filters>"),
            1
        );
        assert_eq!(one("<top10 val=\"0\"/>"), 0);
        assert_eq!(one("<top10 val=\"2.5\"/>"), 0);
        assert_eq!(one("<top10 val=\"101\" percent=\"1\"/>"), 0);
        assert_eq!(one("<top10 val=\"101\"/>"), 1);
        assert_eq!(one("<top10 val=\"1000001\"/>"), 0);
        assert_eq!(one("<top10 val=\"NaN\"/>"), 0);
        assert_eq!(one("<top10/>"), 0);
    }

    #[test]
    fn a_flood_of_values_and_groups_is_cut_at_the_bound_and_the_filter_dropped() {
        let groups = "<dateGroupItem year=\"2024\" dateTimeGrouping=\"year\"/>"
            .repeat(MAX_FILTER_DATE_GROUPS + 5);
        let xml = format!(
            "<autoFilter ref=\"A1:B9\"><filterColumn colId=\"0\"><filters>{groups}</filters></filterColumn></autoFilter>"
        );
        // The list stops growing at the bound plus one entry, which `within_bounds` refuses: the filter goes.
        assert_eq!(scan(&xml), None);
    }

    #[test]
    fn a_range_over_the_bounds_or_a_column_outside_it_is_dropped() {
        assert_eq!(scan("<autoFilter ref=\"A1:C1\"/>"), None);
        assert_eq!(scan("<autoFilter ref=\"A1:C300000\"/>"), None);
        assert_eq!(scan("<autoFilter ref=\"nope\"/>"), None);
        let xml = "<autoFilter ref=\"A1:B9\"><filterColumn colId=\"5\"><filters blank=\"1\"/></filterColumn></autoFilter>";
        assert_eq!(scan(xml).unwrap().columns, Vec::new());
        let long = "x".repeat(256);
        let xml = format!(
            "<autoFilter ref=\"A1:B9\"><filterColumn colId=\"0\"><filters><filter val=\"{long}\"/></filters></filterColumn></autoFilter>"
        );
        assert_eq!(scan(&xml), None);
    }
}

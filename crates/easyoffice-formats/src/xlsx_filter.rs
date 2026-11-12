//! `autoFilter` of a worksheet part: reading it from the event stream and writing it back
//! (ECMA-376 18.3.2.1 `autoFilter`, 18.3.2.7 `filterColumn`, 18.3.2.8 `filters`, 18.3.2.4
//! `customFilters`). The model is `sheet_filter`; the rows a filter hides are `row hidden`, which
//! the sheet reader keeps with the user's hidden rows until a caller that has the cells tells the
//! two apart.
//!
//! A filter column the model cannot hold (top-ten, dynamic, colour and icon filters, date groups,
//! more than two custom conditions) is dropped; the rows it hid stay hidden as ordinary hidden
//! rows. A range or a list over the bounds of `sheet_filter` drops the whole filter.

use crate::sheet_filter::{
    CustomCondition, CustomOp, FilterColumn, FilterCriteria, SheetFilter, MAX_FILTER_COLUMNS,
};
use crate::xml_stream::{Attrs, XmlEvent};
use crate::{CellBox, XmlSanitizer};
use std::fmt::Write;

/// What the column element being read holds so far.
enum Open {
    None,
    Values {
        blank: bool,
        values: Vec<String>,
    },
    Custom {
        and: bool,
        conditions: Vec<CustomCondition>,
    },
    /// A kind of filter the model cannot hold.
    Unsupported,
}

pub(crate) struct FilterScan {
    /// Inside the `autoFilter` element; every other element of the sheet is skipped at once.
    inside: bool,
    filter: Option<SheetFilter>,
    col: Option<u32>,
    open: Open,
}

fn flag(attrs: &Attrs<'_>, name: &str) -> bool {
    matches!(attrs.get(name).as_deref(), Some("1" | "true"))
}

impl FilterScan {
    pub(crate) const fn new() -> Self {
        Self {
            inside: false,
            filter: None,
            col: None,
            open: Open::None,
        }
    }

    /// The filter read, or `None` when there is none or it was over the bounds.
    pub(crate) fn finish(self) -> Option<SheetFilter> {
        self.filter.filter(SheetFilter::within_bounds)
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
                self.open = Open::Values {
                    blank: flag(attrs, "blank"),
                    values: Vec::new(),
                };
            }
            "filter" => {
                if let (Open::Values { values, .. }, Some(val)) = (&mut self.open, attrs.get("val"))
                {
                    values.push(val.into_owned());
                }
            }
            "customFilters" => {
                self.open = Open::Custom {
                    and: flag(attrs, "and"),
                    conditions: Vec::new(),
                };
            }
            "customFilter" => self.custom_filter(attrs),
            "top10" | "dynamicFilter" | "colorFilter" | "iconFilter" | "dateGroupItem" => {
                self.open = Open::Unsupported;
            }
            _ => {}
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
            Open::Values { blank, values } if blank || !values.is_empty() => {
                FilterCriteria::Values { blank, values }
            }
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

fn custom_filter_xml(out: &mut String, c: &CustomCondition) {
    out.push_str("<customFilter");
    if let Some(op) = c.op {
        let _ = write!(out, " operator=\"{}\"", op.name());
    }
    let _ = write!(out, " val=\"{}\"/>", XmlSanitizer::escape(&c.text));
}

/// `<autoFilter>` for `filter`.
pub(crate) fn autofilter_xml(filter: &SheetFilter) -> String {
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
            FilterCriteria::Values { blank, values } => {
                out.push_str("<filters");
                if *blank {
                    out.push_str(" blank=\"1\"");
                }
                if values.is_empty() {
                    out.push_str("/>");
                } else {
                    out.push('>');
                    for value in values {
                        let _ = write!(out, "<filter val=\"{}\"/>", XmlSanitizer::escape(value));
                    }
                    out.push_str("</filters>");
                }
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

    fn scan(xml: &str) -> Option<SheetFilter> {
        let mut scan = FilterScan::new();
        let mut reader = XmlReader::unbounded(xml);
        while let Some(event) = reader.next_event().unwrap() {
            scan.feed(&event);
        }
        scan.finish()
    }

    #[test]
    fn values_and_custom_columns_round_trip_through_the_xml() {
        let mut filter = SheetFilter::new(CellBox::parse("B2:E40").unwrap());
        filter.columns = vec![
            FilterColumn {
                col: 1,
                criteria: FilterCriteria::Values {
                    blank: true,
                    values: vec!["a&b".into(), "<x>".into(), "한글".into()],
                },
            },
            FilterColumn {
                col: 3,
                criteria: FilterCriteria::Custom {
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
            },
        ];
        let xml = autofilter_xml(&filter);
        assert!(xml.starts_with("<autoFilter ref=\"B2:E40\"><filterColumn colId=\"0\">"));
        assert!(xml.contains("<filterColumn colId=\"2\"><customFilters and=\"1\">"));
        assert_eq!(scan(&xml), Some(filter));
    }

    #[test]
    fn a_filter_without_columns_and_unsupported_columns() {
        let plain = SheetFilter::new(CellBox::parse("A1:C9").unwrap());
        assert_eq!(autofilter_xml(&plain), "<autoFilter ref=\"A1:C9\"/>");
        assert_eq!(scan("<autoFilter ref=\"A1:C9\"/>"), Some(plain));
        let xml = concat!(
            "<autoFilter ref=\"A1:C9\">",
            "<filterColumn colId=\"0\"><top10 val=\"3\"/></filterColumn>",
            "<filterColumn colId=\"1\"><filters><dateGroupItem year=\"2024\"/></filters></filterColumn>",
            "<filterColumn colId=\"2\"><customFilters><customFilter operator=\"equal\" val=\"1\"/>",
            "<customFilter val=\"2\"/><customFilter val=\"3\"/></customFilters></filterColumn>",
            "</autoFilter>"
        );
        assert_eq!(scan(xml).unwrap().columns, Vec::new());
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

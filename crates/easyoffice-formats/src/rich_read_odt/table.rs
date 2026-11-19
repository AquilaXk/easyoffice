//! ODT table column width, cell style (`style:table-cell-properties`), and cell image state.

use std::borrow::Cow;
use std::collections::HashMap;

use super::{count_attr, length_emu, length_twips, EMU_PER_POINT};
use crate::rich_image::RichImage;
use crate::rich_table::MAX_PADDED_COLUMNS;
use crate::rich_table_style::{
    CellFill, TableBorderEdge, TableBorderKind, TableCellBorders, MAX_COL_WIDTH_TWIPS,
    MIN_COL_WIDTH_TWIPS,
};
use crate::xml_stream::{attr, Attrs, XmlEvent};

#[derive(Default)]
pub(super) struct OdtTableStylesMap {
    pub(super) col_widths_twips: HashMap<String, u32>,
    pub(super) cell_styles: HashMap<String, (CellFill, TableCellBorders)>,
    cur_col_name: Option<String>,
    cur_cell: Option<(String, CellFill, TableCellBorders)>,
}

impl OdtTableStylesMap {
    pub(super) fn start_style(&mut self, name: String, family: &str) {
        self.cur_col_name = None;
        self.cur_cell = None;
        match family {
            "table-column" => self.cur_col_name = Some(name),
            "table-cell" => {
                self.cur_cell = Some((name, CellFill::none(), TableCellBorders::default()))
            }
            _ => {}
        }
    }

    pub(super) fn on_col_props(&mut self, attrs: &Attrs<'_>) {
        let Some(name) = self.cur_col_name.as_ref() else {
            return;
        };
        if let Some(twips) = attr(attrs, "style:column-width")
            .as_deref()
            .and_then(length_twips)
            .and_then(|t| u32::try_from(t).ok())
            .filter(|w| (MIN_COL_WIDTH_TWIPS..=MAX_COL_WIDTH_TWIPS).contains(w))
        {
            self.col_widths_twips.insert(name.clone(), twips);
        }
    }

    pub(super) fn on_cell_props(&mut self, attrs: &Attrs<'_>) {
        let Some((_, fill, borders)) = self.cur_cell.as_mut() else {
            return;
        };
        if let Some(bg) = attr(attrs, "fo:background-color")
            .as_deref()
            .and_then(CellFill::from_hex)
        {
            *fill = bg;
        }
        if let Some(edge) = attr(attrs, "fo:border")
            .as_deref()
            .and_then(parse_odt_border_edge)
        {
            borders.top = edge;
            borders.right = edge;
            borders.bottom = edge;
            borders.left = edge;
        }
        if let Some(edge) = attr(attrs, "fo:border-top")
            .as_deref()
            .and_then(parse_odt_border_edge)
        {
            borders.top = edge;
        }
        if let Some(edge) = attr(attrs, "fo:border-right")
            .as_deref()
            .and_then(parse_odt_border_edge)
        {
            borders.right = edge;
        }
        if let Some(edge) = attr(attrs, "fo:border-bottom")
            .as_deref()
            .and_then(parse_odt_border_edge)
        {
            borders.bottom = edge;
        }
        if let Some(edge) = attr(attrs, "fo:border-left")
            .as_deref()
            .and_then(parse_odt_border_edge)
        {
            borders.left = edge;
        }
    }

    pub(super) fn finish_style(&mut self) {
        self.cur_col_name = None;
        if let Some((name, fill, borders)) = self.cur_cell.take() {
            if !fill.is_none() || !borders.is_unspecified() {
                self.cell_styles.insert(name, (fill, borders));
            }
        }
    }
}

fn parse_odt_border_edge(raw: &str) -> Option<TableBorderEdge> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if t.eq_ignore_ascii_case("none") || t.eq_ignore_ascii_case("hidden") {
        return Some(TableBorderEdge::NONE);
    }
    let mut style: Option<&str> = None;
    let mut width_pt: Option<f64> = None;
    let mut rgb: Option<u32> = None;
    for tok in t.split_whitespace() {
        if tok.eq_ignore_ascii_case("none") || tok.eq_ignore_ascii_case("hidden") {
            style = Some("none");
        } else if tok.eq_ignore_ascii_case("solid") {
            style = Some("solid");
        } else if tok.eq_ignore_ascii_case("dashed") {
            style = Some("dashed");
        } else if tok.eq_ignore_ascii_case("dotted") {
            style = Some("dotted");
        } else if tok.eq_ignore_ascii_case("double") {
            style = Some("double");
        } else if let Some(c) = CellFill::from_hex(tok).and_then(CellFill::rgb) {
            rgb = Some(c);
        } else if let Some(emu) = length_emu(tok) {
            width_pt = Some(emu / EMU_PER_POINT);
        }
    }
    if style == Some("none") || width_pt.is_some_and(|w| w <= 0.0) {
        return Some(TableBorderEdge::NONE);
    }
    if style == Some("solid")
        && width_pt.is_some_and(|w| (w - 0.5).abs() < 0.05)
        && rgb.unwrap_or(0) == 0
    {
        return Some(TableBorderEdge::UNSPECIFIED);
    }
    if style.is_none() && width_pt.is_none() {
        return None;
    }
    let kind = match style {
        Some("double") => TableBorderKind::Double,
        Some("dashed") => TableBorderKind::Dashed,
        Some("dotted") => TableBorderKind::Dotted,
        _ => {
            if width_pt.is_some_and(|w| w >= 1.5) {
                TableBorderKind::Thick
            } else {
                TableBorderKind::Thin
            }
        }
    };
    Some(TableBorderEdge::new(kind, rgb.unwrap_or(0)))
}

#[derive(Default)]
pub(super) struct OdtTableState {
    pub(super) in_cell: bool,
    pub(super) col_widths: Vec<Option<u32>>,
    pub(super) cell_style: Option<(CellFill, TableCellBorders)>,
    pub(super) cell_images: Vec<RichImage>,
}

impl OdtTableState {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn on_table_column(&mut self, attrs: &Attrs<'_>, styles: &OdtTableStylesMap) {
        let room = MAX_PADDED_COLUMNS.saturating_sub(self.col_widths.len()) as u32;
        let repeat = count_attr(attrs, "table:number-columns-repeated").min(room);
        let width = attr(attrs, "table:style-name")
            .as_deref()
            .and_then(|n| styles.col_widths_twips.get(n))
            .copied();
        for _ in 0..repeat {
            self.col_widths.push(width);
        }
    }

    pub(super) fn start_cell(&mut self, attrs: &Attrs<'_>, styles: &OdtTableStylesMap) {
        self.in_cell = true;
        self.cell_style = attr(attrs, "table:style-name")
            .as_deref()
            .and_then(|n| styles.cell_styles.get(n))
            .copied();
        self.cell_images.clear();
    }

    pub(super) fn finish_cell(&mut self) -> (Option<(CellFill, TableCellBorders)>, Vec<RichImage>) {
        self.in_cell = false;
        (
            self.cell_style.take(),
            std::mem::take(&mut self.cell_images),
        )
    }

    pub(super) fn resolve_col_widths(&self, max_cols: usize) -> Option<Vec<u32>> {
        if max_cols == 0 || self.col_widths.len() < max_cols {
            return None;
        }
        let mut widths = Vec::with_capacity(max_cols);
        for slot in &self.col_widths[..max_cols] {
            widths.push((*slot)?);
        }
        Some(widths)
    }
}

/// Fields of one `draw:frame` holding a `draw:image`.
pub(super) struct Frame {
    pub(super) href: String,
    pub(super) alt: String,
    pub(super) size_emu: Option<(u64, u64)>,
}

pub(super) fn read_frame(events: &[XmlEvent<'_>]) -> Option<Frame> {
    let mut href = None;
    let mut size_emu = None;
    let (mut desc, mut title) = (String::new(), String::new());
    let mut capture: Option<&str> = None;
    for (idx, event) in events.iter().enumerate() {
        match event {
            XmlEvent::Start { name, attrs } => match *name {
                "draw:frame" if idx == 0 => {
                    let side = |key| {
                        attr(attrs, key)
                            .as_deref()
                            .and_then(length_emu)
                            .filter(|e| *e >= 1.0)
                    };
                    size_emu = side("svg:width")
                        .zip(side("svg:height"))
                        .map(|(w, h)| (w.round() as u64, h.round() as u64));
                }
                "draw:image" if href.is_none() => {
                    href = attr(attrs, "xlink:href").map(Cow::into_owned);
                }
                "svg:desc" => capture = Some("desc"),
                "svg:title" => capture = Some("title"),
                _ => {}
            },
            XmlEvent::Text(text) => match capture {
                Some("desc") => desc.push_str(text),
                Some(_) => title.push_str(text),
                None => {}
            },
            XmlEvent::End(name) if *name == "svg:desc" || *name == "svg:title" => capture = None,
            XmlEvent::End(_) => {}
        }
    }
    let alt = if desc.trim().is_empty() { title } else { desc };
    Some(Frame {
        href: href?,
        alt,
        size_emu,
    })
}

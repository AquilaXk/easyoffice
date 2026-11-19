//! DOCX table column width, cell shading/borders (`w:tcPr`), and cell picture tracking.

use crate::rich_image::RichImage;
use crate::rich_table::MAX_PADDED_COLUMNS;
use crate::rich_table_style::{
    CellFill, TableBorderEdge, TableBorderKind, TableCellBorders, MAX_COL_WIDTH_TWIPS,
    MIN_COL_WIDTH_TWIPS,
};
use crate::xml_stream::{attr, Attrs};

/// `w:vMerge` state of a table cell.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum VMerge {
    #[default]
    None,
    Restart,
    Continue,
}

/// `w:gridSpan/@w:val`, at least 1; an unreadable value is 1 and a huge one is left for the
/// table assembler to reject.
pub(super) fn grid_span(attrs: &Attrs<'_>) -> u32 {
    attr(attrs, "w:val")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(1, |v| u32::try_from(v).unwrap_or(u32::MAX).max(1))
}

#[derive(Default)]
pub(super) struct DocxTableState {
    pub(super) in_tbl_pr: bool,
    pub(super) in_tbl_grid: bool,
    pub(super) in_cell: bool,
    pub(super) in_tc_pr: bool,
    pub(super) in_tc_borders: bool,
    pub(super) tbl_fixed_layout: bool,
    pub(super) tbl_explicit_dxa: bool,
    pub(super) has_tc_w: bool,
    pub(super) grid_cols: Vec<u32>,
    pub(super) grid_valid: bool,
    pub(super) row_idx: usize,
    pub(super) row0_tc_widths: Vec<(u32, u32)>,
    pub(super) cell_fill: CellFill,
    pub(super) cell_borders: TableCellBorders,
    pub(super) cell_tc_w: Option<u32>,
    pub(super) cell_images: Vec<RichImage>,
}

impl DocxTableState {
    pub(super) fn reset_table(&mut self) {
        *self = Self {
            grid_valid: true,
            ..Self::default()
        };
    }

    pub(super) fn start_cell(&mut self) {
        self.in_cell = true;
        self.in_tc_pr = false;
        self.in_tc_borders = false;
        self.cell_fill = CellFill::none();
        self.cell_borders = TableCellBorders::default();
        self.cell_tc_w = None;
        self.cell_images.clear();
    }

    pub(super) fn finish_cell(
        &mut self,
        cell_span: u32,
    ) -> (CellFill, TableCellBorders, Vec<RichImage>) {
        if self.row_idx == 0 && self.row0_tc_widths.len() < MAX_PADDED_COLUMNS {
            self.row0_tc_widths
                .push((cell_span, self.cell_tc_w.unwrap_or(0)));
        }
        let fill = self.cell_fill;
        let borders = self.cell_borders;
        let images = std::mem::take(&mut self.cell_images);
        self.in_cell = false;
        self.in_tc_pr = false;
        self.in_tc_borders = false;
        self.cell_fill = CellFill::none();
        self.cell_borders = TableCellBorders::default();
        self.cell_tc_w = None;
        (fill, borders, images)
    }

    pub(super) fn finish_row(&mut self) {
        self.row_idx = self.row_idx.saturating_add(1);
    }

    pub(super) fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:tblPr" if !self.in_cell => self.in_tbl_pr = true,
            "w:tblGrid" if !self.in_cell => self.in_tbl_grid = true,
            "w:tblW" if self.in_tbl_pr => {
                if attr(attrs, "w:type").as_deref() == Some("dxa")
                    && attr(attrs, "w:w")
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        .is_some_and(|w| w > 0)
                {
                    self.tbl_explicit_dxa = true;
                }
            }
            "w:tblLayout" if self.in_tbl_pr => {
                if attr(attrs, "w:type").as_deref() == Some("fixed") {
                    self.tbl_fixed_layout = true;
                }
            }
            "w:gridCol" if self.in_tbl_grid => {
                if self.grid_cols.len() < MAX_PADDED_COLUMNS {
                    match attr(attrs, "w:w").and_then(|v| v.trim().parse::<u32>().ok()) {
                        Some(w) if w > 0 => self.grid_cols.push(w),
                        _ => self.grid_valid = false,
                    }
                }
            }
            "w:tcPr" if self.in_cell => self.in_tc_pr = true,
            "w:tcW" if self.in_tc_pr => {
                let is_dxa = attr(attrs, "w:type").as_deref().is_none_or(|t| t == "dxa");
                if is_dxa {
                    if let Some(w) = attr(attrs, "w:w")
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        .filter(|w| *w > 0)
                    {
                        self.cell_tc_w = Some(w);
                        self.has_tc_w = true;
                    }
                }
            }
            "w:tcBorders" if self.in_tc_pr => self.in_tc_borders = true,
            "w:top" if self.in_tc_borders => {
                self.cell_borders.top = parse_docx_border_edge(attrs);
            }
            "w:right" if self.in_tc_borders => {
                self.cell_borders.right = parse_docx_border_edge(attrs);
            }
            "w:bottom" if self.in_tc_borders => {
                self.cell_borders.bottom = parse_docx_border_edge(attrs);
            }
            "w:left" if self.in_tc_borders => {
                self.cell_borders.left = parse_docx_border_edge(attrs);
            }
            "w:shd" if self.in_tc_pr => {
                if let Some(fill) = attr(attrs, "w:fill")
                    .as_deref()
                    .and_then(CellFill::from_hex)
                {
                    self.cell_fill = fill;
                }
            }
            _ => {}
        }
    }

    pub(super) fn on_end(&mut self, name: &str) {
        match name {
            "w:tblPr" => self.in_tbl_pr = false,
            "w:tblGrid" => self.in_tbl_grid = false,
            "w:tcBorders" => self.in_tc_borders = false,
            "w:tcPr" => self.in_tc_pr = false,
            _ => {}
        }
    }

    pub(super) fn resolve_col_widths(&self, max_cols: usize) -> Option<Vec<u32>> {
        if max_cols == 0 {
            return None;
        }
        let widths = if self.grid_valid && self.grid_cols.len() == max_cols {
            self.grid_cols.clone()
        } else if self.row0_tc_widths.len() == max_cols
            && self
                .row0_tc_widths
                .iter()
                .all(|&(span, w)| span == 1 && w > 0)
        {
            self.row0_tc_widths.iter().map(|&(_, w)| w).collect()
        } else {
            return None;
        };
        let in_range = widths
            .iter()
            .all(|w| (MIN_COL_WIDTH_TWIPS..=MAX_COL_WIDTH_TWIPS).contains(w));
        let non_uniform = widths.windows(2).any(|w| w[0] != w[1]);
        (in_range
            && (self.tbl_fixed_layout || self.tbl_explicit_dxa || self.has_tc_w || non_uniform))
            .then_some(widths)
    }
}

fn parse_docx_border_edge(attrs: &Attrs<'_>) -> TableBorderEdge {
    let Some(val) = attr(attrs, "w:val") else {
        return TableBorderEdge::UNSPECIFIED;
    };
    let kind = match val.trim() {
        "nil" | "none" => return TableBorderEdge::NONE,
        "double" => TableBorderKind::Double,
        "dashed" | "dashSmallGap" => TableBorderKind::Dashed,
        "dotted" => TableBorderKind::Dotted,
        "thick" => TableBorderKind::Thick,
        _ => {
            let sz = attr(attrs, "w:sz")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(4);
            if sz >= 10 {
                TableBorderKind::Thick
            } else {
                TableBorderKind::Thin
            }
        }
    };
    let rgb = attr(attrs, "w:color")
        .as_deref()
        .and_then(CellFill::from_hex)
        .and_then(CellFill::rgb)
        .unwrap_or(0);
    TableBorderEdge::new(kind, rgb)
}

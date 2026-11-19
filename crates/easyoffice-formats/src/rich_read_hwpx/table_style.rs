//! HWPX `Contents/header.xml` `<hh:borderFill>` parser (OWPML cell borders and fill brush).
//!
//! 참조 구현: `rhwp` (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`),
//! `src/parser/hwpx/header.rs` (`parse_border_fill` 1588-1740행, `parse_border_line_type`
//! 2379-2406행) 및 `src/serializer/hwpx/header.rs` (`write_border_fill` 352-406행)의 요소·속성
//! 규격을 스트리밍 파서와 Fail-Closed 상한([`MAX_HWPX_BORDER_FILLS`])으로 재작성.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::rich_table_style::{CellFill, TableBorderEdge, TableBorderKind, TableCellBorders};
use crate::xml_stream::{attr, Attrs};
use crate::EasyOfficeError;

/// Fail-closed upper bound on `<hh:borderFill>` entries in one HWPX header.
pub(super) const MAX_HWPX_BORDER_FILLS: usize = 4_096;

#[derive(Default)]
pub(super) struct BorderFillReader {
    pub(super) entries: HashMap<String, (CellFill, TableCellBorders)>,
    pub(super) count: usize,
    cur_id: Option<String>,
    cur_fill: CellFill,
    cur_borders: TableCellBorders,
    in_fill_brush: bool,
}

impl BorderFillReader {
    pub(super) fn on_start(
        &mut self,
        name: &str,
        attrs: &Attrs<'_>,
    ) -> Result<(), EasyOfficeError> {
        match name {
            "hh:borderFill" => {
                self.count += 1;
                if self.count > MAX_HWPX_BORDER_FILLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "HWPX 테두리·배경 정의 수가 한도({MAX_HWPX_BORDER_FILLS})를 넘었습니다."
                    )));
                }
                self.cur_id = attr(attrs, "id").map(Cow::into_owned);
                self.cur_fill = CellFill::none();
                self.cur_borders = TableCellBorders::default();
                self.in_fill_brush = false;
            }
            "hh:leftBorder" if self.cur_id.is_some() => {
                self.cur_borders.left = parse_hwpx_border_edge(attrs);
            }
            "hh:rightBorder" if self.cur_id.is_some() => {
                self.cur_borders.right = parse_hwpx_border_edge(attrs);
            }
            "hh:topBorder" if self.cur_id.is_some() => {
                self.cur_borders.top = parse_hwpx_border_edge(attrs);
            }
            "hh:bottomBorder" if self.cur_id.is_some() => {
                self.cur_borders.bottom = parse_hwpx_border_edge(attrs);
            }
            "hc:fillBrush" if self.cur_id.is_some() => {
                self.in_fill_brush = true;
            }
            "hc:winBrush" if self.in_fill_brush => {
                if let Some(fill) = attr(attrs, "faceColor")
                    .as_deref()
                    .and_then(CellFill::from_hex)
                {
                    self.cur_fill = fill;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn on_end(&mut self, name: &str) {
        match name {
            "hc:fillBrush" => self.in_fill_brush = false,
            "hh:borderFill" => {
                if let Some(id) = self.cur_id.take() {
                    if !self.cur_fill.is_none() || !self.cur_borders.is_unspecified() {
                        self.entries.insert(id, (self.cur_fill, self.cur_borders));
                    }
                }
                self.in_fill_brush = false;
            }
            _ => {}
        }
    }
}

fn parse_mm(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    let num = trimmed.strip_suffix("mm").unwrap_or(trimmed).trim();
    num.parse::<f64>()
        .ok()
        .filter(|w| w.is_finite() && *w >= 0.0)
}

fn parse_hwpx_border_edge(attrs: &Attrs<'_>) -> TableBorderEdge {
    let Some(line_type) = attr(attrs, "type") else {
        return TableBorderEdge::UNSPECIFIED;
    };
    let line = line_type.trim();
    let width_mm = attr(attrs, "width")
        .as_deref()
        .and_then(parse_mm)
        .unwrap_or(0.12);
    let rgb = attr(attrs, "color")
        .as_deref()
        .and_then(CellFill::from_hex)
        .and_then(CellFill::rgb)
        .unwrap_or(0);
    let kind = match line {
        "NONE" => return TableBorderEdge::NONE,
        "DOUBLE_SLIM" | "DOUBLE" => TableBorderKind::Double,
        "DASH" => TableBorderKind::Dashed,
        "DOT" => TableBorderKind::Dotted,
        _ => {
            if width_mm >= 0.30 {
                TableBorderKind::Thick
            } else if rgb == 0 && (width_mm - 0.12).abs() <= 0.03 {
                return TableBorderEdge::UNSPECIFIED;
            } else {
                TableBorderKind::Thin
            }
        }
    };
    TableBorderEdge::new(kind, rgb)
}

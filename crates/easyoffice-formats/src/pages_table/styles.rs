//! Column widths, cell background fills, and RLE cell borders for the Pages table reader.
//!
//! All repeated structures enforce strict per-table limits so hostile sidecars or header buckets
//! fail closed with [`EasyOfficeError::ResourceBudgetExceeded`].

use std::collections::hash_map::Entry;

use super::parsed::ParsedObjects;
use super::{reference, KeyedList, TableBudget, MAX_LIST_ENTRIES, MAX_TABLE_COLS, MAX_TABLE_ROWS};
use crate::decode_budget::DecodeBudget;
use crate::pages_proto::{FastU64Map, Message, ObjectTable};
use crate::pages_schema::{table, ty, COLOR_RGBA};
use crate::rich_table_style::{
    CellFill, TableBorderEdge, TableBorderKind, TableCellBorders, TableCellStyleEntry,
    MAX_COL_WIDTH_TWIPS, MAX_STYLED_TABLE_CELLS, MIN_COL_WIDTH_TWIPS,
};
use crate::EasyOfficeError;

const MAX_HEADER_ENTRIES: usize = MAX_TABLE_COLS * 16;
/// Matches the maximum valid table (`MAX_TABLE_ROWS` x `MAX_TABLE_COLS`) across all 4 axes,
/// including the outer right (`index == cols`) and outer bottom (`index == rows`) border layers.
const MAX_STROKE_LAYERS: usize =
    (MAX_TABLE_ROWS + 1) + MAX_TABLE_ROWS + (MAX_TABLE_COLS + 1) + MAX_TABLE_COLS;
const MAX_STROKE_RUNS: usize = 250_000;
/// Maximum RLE border-cell expansions permitted on a single table before failing closed.
const MAX_STROKE_CELLS_PER_TABLE: usize = MAX_STYLED_TABLE_CELLS * 8;
const DEFAULT_COL_WIDTH_TWIPS: u32 = 1_440;
const MAX_COL_WIDTH_PT: f64 = 2_000.0;
const DOTTED_MAX_DASH_PT: f32 = 2.5;
const THICK_MIN_WIDTH_PT: f32 = 1.5;

fn pts_to_twips_clamped(pt: f64) -> Option<u32> {
    if !pt.is_finite() || pt <= 0.0 || pt > MAX_COL_WIDTH_PT {
        return None;
    }
    let raw = (pt * 20.0).round();
    if raw < 1.0 || raw > f64::from(u32::MAX) {
        return None;
    }
    Some((raw as u32).clamp(MIN_COL_WIDTH_TWIPS, MAX_COL_WIDTH_TWIPS))
}

fn tsp_color_rgb(color: &Message) -> Option<u32> {
    let mut rgb = 0u32;
    for &field in &COLOR_RGBA[..3] {
        let v = color.float(field)?;
        if !v.is_finite() {
            return None;
        }
        let byte = (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        rgb = (rgb << 8) | byte;
    }
    Some(rgb)
}

fn parse_msg<'a>(payload: &'a [u8]) -> Result<Option<Message<'a>>, EasyOfficeError> {
    match Message::parse(payload) {
        Ok(m) => Ok(Some(m)),
        Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) => Err(err),
        Err(_) => Ok(None),
    }
}

fn parse_cell_style_fill(payload: &[u8]) -> Result<Option<CellFill>, EasyOfficeError> {
    let Some(style) = parse_msg(payload)? else {
        return Ok(None);
    };
    let Some(props) = style.sub(table::CELL_STYLE_PROPS).ok().flatten() else {
        return Ok(None);
    };
    let Some(fill) = props.sub(table::CELL_PROPS_FILL).ok().flatten() else {
        return Ok(None);
    };
    let Some(color) = fill.sub(table::FILL_COLOR).ok().flatten() else {
        return Ok(None);
    };
    Ok(tsp_color_rgb(&color).map(CellFill::solid))
}

pub(super) fn read_cell_fills(
    objects: &ObjectTable,
    store: &Message,
    budget: &mut TableBudget,
) -> Result<KeyedList<CellFill>, EasyOfficeError> {
    let mut out = Vec::new();
    let Some(obj) = reference(store, table::STORE_STYLE_LIST)
        .and_then(|id| objects.get(id))
        .filter(|o| o.type_id == ty::TABLE_DATA_LIST)
    else {
        return Ok(out);
    };
    let Some(list) = parse_msg(&obj.payload)? else {
        return Ok(out);
    };
    let mut seen = 0usize;
    let res = list.for_each_sub(table::LIST_ENTRIES, |entry| {
        if seen >= MAX_LIST_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 셀 스타일 목록 한도 초과".to_string(),
            ));
        }
        seen += 1;
        budget.charge_list_entry()?;
        let Some(key) = entry.varint(table::ENTRY_KEY) else {
            return Ok(());
        };
        let Some(style_obj) = reference(&entry, table::ENTRY_CELL_STYLE)
            .and_then(|id| objects.get(id))
            .filter(|o| o.type_id == ty::CELL_STYLE)
        else {
            return Ok(());
        };
        if let Some(fill) = parse_cell_style_fill(&style_obj.payload)? {
            out.push((key, fill));
        }
        Ok(())
    });
    if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = res {
        return Err(err);
    }
    out.sort_by_key(|(k, _)| *k);
    Ok(out)
}

pub(super) fn read_col_widths_twips(
    objects: &ObjectTable,
    model: &Message,
    store: &Message,
    cols: usize,
) -> Result<Vec<u32>, EasyOfficeError> {
    if cols == 0 {
        return Ok(Vec::new());
    }
    let default_twips = model
        .double(table::MODEL_DEFAULT_COL_WIDTH_F64)
        .or_else(|| model.float(table::MODEL_DEFAULT_COL_WIDTH).map(f64::from))
        .and_then(pts_to_twips_clamped);
    let mut widths = vec![default_twips.unwrap_or(DEFAULT_COL_WIDTH_TWIPS); cols];
    let mut has_explicit_col = false;
    if let Some(bucket_obj) = reference(store, table::STORE_COL_HEADERS)
        .and_then(|id| objects.get(id))
        .filter(|o| o.type_id == ty::TABLE_HEADER_BUCKET)
    {
        if let Some(bucket) = parse_msg(&bucket_obj.payload)? {
            let mut seen = 0usize;
            let res = bucket.for_each_sub(table::HEADER_BUCKET_ENTRIES, |entry| {
                if seen >= MAX_HEADER_ENTRIES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 표 열 헤더 항목 수 한도 초과".to_string(),
                    ));
                }
                seen += 1;
                let Some(idx) = entry
                    .varint(table::HEADER_ENTRY_INDEX)
                    .and_then(|v| usize::try_from(v).ok())
                else {
                    return Ok(());
                };
                if idx < cols {
                    if let Some(tw) = entry
                        .float_any(table::HEADER_ENTRY_SIZE)
                        .map(f64::from)
                        .and_then(pts_to_twips_clamped)
                    {
                        widths[idx] = tw;
                        has_explicit_col = true;
                    }
                }
                Ok(())
            });
            if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = res {
                return Err(err);
            }
        }
    }
    if has_explicit_col || default_twips.is_some() {
        Ok(widths)
    } else {
        Ok(Vec::new())
    }
}

fn first_dash_pt(pattern: &Message) -> Option<f32> {
    if let Some(v) = pattern.float(table::STROKE_PATTERN_DASH) {
        return v.is_finite().then_some(v);
    }
    let raw = pattern.bytes(table::STROKE_PATTERN_DASH)?;
    let chunk = raw.get(..4)?;
    let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    v.is_finite().then_some(v)
}

fn parse_stroke_edge(stroke: &Message) -> TableBorderEdge {
    let width = stroke.float(table::STROKE_WIDTH).unwrap_or(1.0);
    let pattern = stroke.sub(table::STROKE_PATTERN).ok().flatten();
    let ptype = pattern
        .as_ref()
        .and_then(|p| p.varint(table::STROKE_PATTERN_TYPE));
    if ptype == Some(table::PATTERN_NONE) || !width.is_finite() || width <= 0.0 {
        return TableBorderEdge::NONE;
    }
    let rgb = stroke
        .sub(table::STROKE_COLOR)
        .ok()
        .flatten()
        .and_then(|c| tsp_color_rgb(&c))
        .unwrap_or(0);
    let kind = if ptype == Some(table::PATTERN_DASHED) {
        if pattern
            .as_ref()
            .and_then(first_dash_pt)
            .is_some_and(|d| d <= DOTTED_MAX_DASH_PT)
        {
            TableBorderKind::Dotted
        } else {
            TableBorderKind::Dashed
        }
    } else if width >= THICK_MIN_WIDTH_PT {
        TableBorderKind::Thick
    } else {
        TableBorderKind::Thin
    };
    TableBorderEdge::new(kind, rgb)
}

#[derive(Clone, Copy)]
enum BorderAxis {
    LeftCol,
    RightCol,
    TopRow,
    BottomRow,
}

#[derive(Clone, Copy, Default)]
struct BorderPriority {
    edges: TableCellBorders,
    orders: [u64; 4],
}

fn set_edge(
    map: &mut FastU64Map<BorderPriority>,
    row: usize,
    col: usize,
    edge_slot: usize,
    edge: TableBorderEdge,
    order: u64,
) -> Result<(), EasyOfficeError> {
    if edge.is_unspecified() {
        return Ok(());
    }
    let key = ((row as u64) << 32) | (col as u64);
    let len = map.len();
    let slot = match map.entry(key) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            if len >= MAX_STYLED_TABLE_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 셀 스타일 수 한도 초과".to_string(),
                ));
            }
            entry.insert(BorderPriority::default())
        }
    };
    if order >= slot.orders[edge_slot] {
        slot.orders[edge_slot] = order;
        match edge_slot {
            0 => slot.edges.top = edge,
            1 => slot.edges.right = edge,
            2 => slot.edges.bottom = edge,
            _ => slot.edges.left = edge,
        }
    }
    Ok(())
}

struct StrokeLayerAcc<'a> {
    rows: usize,
    cols: usize,
    layers_seen: usize,
    runs_seen: usize,
    cells_expanded: usize,
    borders: FastU64Map<BorderPriority>,
    /// Each layer object is parsed once however many sidecar entries name it.
    layers: ParsedObjects<'a>,
}

impl<'a> StrokeLayerAcc<'a> {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            layers_seen: 0,
            runs_seen: 0,
            cells_expanded: 0,
            borders: FastU64Map::default(),
            layers: ParsedObjects::default(),
        }
    }

    fn charge_cells(
        &mut self,
        span: usize,
        budget: &mut TableBudget,
    ) -> Result<(), EasyOfficeError> {
        self.cells_expanded = self
            .cells_expanded
            .checked_add(span)
            .filter(|total| *total <= MAX_STROKE_CELLS_PER_TABLE)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 테두리 확장 셀 수 한도 초과".to_string(),
                )
            })?;
        budget.charge_stroke_cells(span)
    }

    fn apply_layers(
        &mut self,
        objects: &'a ObjectTable,
        sidecar: &Message,
        field: u32,
        axis: BorderAxis,
        budget: &mut TableBudget,
        text_budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let mut layer_ids = Vec::new();
        let collect_res = sidecar.for_each_sub(field, |sub| {
            if self.layers_seen >= MAX_STROKE_LAYERS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 테두리 레이어 수 한도 초과".to_string(),
                ));
            }
            self.layers_seen += 1;
            budget.charge_stroke_layer()?;
            if let Some(id) = sub.varint(1) {
                layer_ids.push(id);
            }
            Ok(())
        });
        if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = collect_res {
            return Err(err);
        }

        let (rows, cols) = (self.rows, self.cols);
        // Taken out of `self` so a parsed layer can be read while `self` collects borders; an error
        // ends the whole table, so the cache is not put back on that path.
        let mut layers = std::mem::take(&mut self.layers);
        for id in layer_ids {
            let Some(layer_obj) = objects.get(id).filter(|o| o.type_id == ty::STROKE_LAYER) else {
                continue;
            };
            let Some(layer) = layers.get(id, layer_obj, text_budget, "Pages 표 테두리 레이어")?
            else {
                continue;
            };
            let Some(index) = layer
                .varint(table::STROKE_LAYER_INDEX)
                .and_then(|v| usize::try_from(v).ok())
            else {
                continue;
            };
            let run_res = layer.for_each_sub(table::STROKE_LAYER_RUNS, |run| {
                if self.runs_seen >= MAX_STROKE_RUNS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 표 테두리 런 수 한도 초과".to_string(),
                    ));
                }
                self.runs_seen += 1;
                budget.charge_stroke_run()?;
                let origin = run
                    .varint(table::STROKE_RUN_ORIGIN)
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(0);
                let length = run
                    .varint(table::STROKE_RUN_LENGTH)
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(0);
                if length == 0 {
                    return Ok(());
                }
                let Some(stroke) = run.sub(table::STROKE_RUN_STROKE).ok().flatten() else {
                    return Ok(());
                };
                let edge = parse_stroke_edge(&stroke);
                if edge.is_unspecified() {
                    return Ok(());
                }
                let order = run.varint(table::STROKE_RUN_ORDER).unwrap_or(0);
                let target = match axis {
                    BorderAxis::LeftCol => {
                        if index < cols && origin < rows {
                            Some((index, 3usize, rows, true))
                        } else if index == cols && cols > 0 && origin < rows {
                            Some((cols - 1, 1usize, rows, true))
                        } else {
                            None
                        }
                    }
                    BorderAxis::RightCol => {
                        (index < cols && origin < rows).then_some((index, 1usize, rows, true))
                    }
                    BorderAxis::TopRow => {
                        if index < rows && origin < cols {
                            Some((index, 0usize, cols, false))
                        } else if index == rows && rows > 0 && origin < cols {
                            Some((rows - 1, 2usize, cols, false))
                        } else {
                            None
                        }
                    }
                    BorderAxis::BottomRow => {
                        (index < rows && origin < cols).then_some((index, 2usize, cols, false))
                    }
                };
                if let Some((fixed, edge_slot, limit, along_rows)) = target {
                    let end = origin.saturating_add(length).min(limit);
                    let span = end.saturating_sub(origin);
                    if span > 0 {
                        self.charge_cells(span, budget)?;
                        let borders = &mut self.borders;
                        if along_rows {
                            for r in origin..end {
                                set_edge(borders, r, fixed, edge_slot, edge, order)?;
                            }
                        } else {
                            for c in origin..end {
                                set_edge(borders, fixed, c, edge_slot, edge, order)?;
                            }
                        }
                    }
                }
                Ok(())
            });
            if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = run_res {
                return Err(err);
            }
        }
        self.layers = layers;
        Ok(())
    }
}

pub(super) fn assemble_cell_styles(
    objects: &ObjectTable,
    model: &Message,
    rows: usize,
    cols: usize,
    cell_fills: &[(u32, u32, CellFill)],
    budget: &mut TableBudget,
    text_budget: &mut DecodeBudget,
) -> Result<Vec<TableCellStyleEntry>, EasyOfficeError> {
    let mut acc = StrokeLayerAcc::new(rows, cols);
    if let Some(sidecar_obj) = reference(model, table::MODEL_STROKE_SIDECAR)
        .and_then(|id| objects.get(id))
        .filter(|o| o.type_id == ty::STROKE_SIDECAR)
    {
        if let Some(sidecar) = parse_msg(&sidecar_obj.payload)? {
            for (field, axis) in [
                (table::STROKE_SIDECAR_LEFT_COLS, BorderAxis::LeftCol),
                (table::STROKE_SIDECAR_RIGHT_COLS, BorderAxis::RightCol),
                (table::STROKE_SIDECAR_TOP_ROWS, BorderAxis::TopRow),
                (table::STROKE_SIDECAR_BOTTOM_ROWS, BorderAxis::BottomRow),
            ] {
                acc.apply_layers(objects, &sidecar, field, axis, budget, text_budget)?;
            }
        }
    }
    let border_map = acc.borders;

    if cell_fills.is_empty() && border_map.is_empty() {
        return Ok(Vec::new());
    }

    let mut combined: FastU64Map<TableCellStyleEntry> = FastU64Map::default();
    let entry_bytes = std::mem::size_of::<TableCellStyleEntry>();
    for &(row, col, fill) in cell_fills {
        if fill.is_none() || row as usize >= rows || col as usize >= cols {
            continue;
        }
        let key = (u64::from(row) << 32) | u64::from(col);
        let len = combined.len();
        match combined.entry(key) {
            Entry::Occupied(mut e) => e.get_mut().fill = fill,
            Entry::Vacant(e) => {
                if len >= MAX_STYLED_TABLE_CELLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 표 셀 스타일 수 한도 초과".to_string(),
                    ));
                }
                budget.charge_styled_cells(1)?;
                text_budget.charge(entry_bytes, "Pages 표 셀 스타일")?;
                e.insert(TableCellStyleEntry {
                    row,
                    col,
                    fill,
                    borders: TableCellBorders::default(),
                });
            }
        }
    }
    for (key, prio) in border_map {
        if prio.edges.is_unspecified() {
            continue;
        }
        let row = (key >> 32) as u32;
        let col = key as u32;
        let len = combined.len();
        match combined.entry(key) {
            Entry::Occupied(mut e) => e.get_mut().borders = prio.edges,
            Entry::Vacant(e) => {
                if len >= MAX_STYLED_TABLE_CELLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 표 셀 스타일 수 한도 초과".to_string(),
                    ));
                }
                budget.charge_styled_cells(1)?;
                text_budget.charge(entry_bytes, "Pages 표 셀 스타일")?;
                e.insert(TableCellStyleEntry {
                    row,
                    col,
                    fill: CellFill::NONE,
                    borders: prio.edges,
                });
            }
        }
    }

    let mut out: Vec<TableCellStyleEntry> = combined.into_values().collect();
    out.sort_by_key(|e| (u64::from(e.row) << 32) | u64::from(e.col));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages_proto::ArchiveObject;

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    fn f_varint(num: u32, v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        varint(u64::from(num) << 3, &mut out);
        varint(v, &mut out);
        out
    }

    fn f_bytes(num: u32, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint((u64::from(num) << 3) | 2, &mut out);
        varint(bytes.len() as u64, &mut out);
        out.extend_from_slice(bytes);
        out
    }

    fn f_ref(num: u32, id: u64) -> Vec<u8> {
        f_bytes(num, &f_varint(1, id))
    }

    /// A sidecar naming layer object 1 `refs` times; the layer carries `padding` unknown fields.
    fn shared_layer(refs: usize, padding: usize) -> (ObjectTable, Vec<u8>) {
        let mut payload = f_varint(table::STROKE_LAYER_INDEX, 0);
        for _ in 0..padding {
            payload.extend_from_slice(&f_varint(99, 1));
        }
        let mut objects = ObjectTable::default();
        objects.objects.insert(
            1,
            ArchiveObject {
                type_id: ty::STROKE_LAYER,
                payload: payload.into(),
                extra: Vec::new(),
            },
        );
        let entry = f_ref(table::STROKE_SIDECAR_TOP_ROWS, 1);
        (objects, entry.repeat(refs))
    }

    #[test]
    fn one_layer_named_many_times_is_parsed_once() {
        let (objects, sidecar_payload) = shared_layer(20_000, 30_000);
        let sidecar = Message::parse(&sidecar_payload).unwrap();
        let mut acc = StrokeLayerAcc::new(MAX_TABLE_ROWS, MAX_TABLE_COLS);
        let mut budget = TableBudget::default();
        let mut text_budget = DecodeBudget::new();
        let start = std::time::Instant::now();
        acc.apply_layers(
            &objects,
            &sidecar,
            table::STROKE_SIDECAR_TOP_ROWS,
            BorderAxis::TopRow,
            &mut budget,
            &mut text_budget,
        )
        .unwrap();
        let took = start.elapsed();
        assert!(took < std::time::Duration::from_secs(2), "{took:?}");
        let used = DecodeBudget::new().remaining() - text_budget.remaining();
        assert!(used < 2 * 1024 * 1024, "{used}");
    }

    #[test]
    fn a_layer_payload_is_charged_to_the_decoded_byte_budget() {
        let (objects, sidecar_payload) = shared_layer(3, 30_000);
        let sidecar = Message::parse(&sidecar_payload).unwrap();
        let mut acc = StrokeLayerAcc::new(MAX_TABLE_ROWS, MAX_TABLE_COLS);
        let mut budget = TableBudget::default();
        let mut text_budget = DecodeBudget::with_limit(8 * 1024);
        let err = acc
            .apply_layers(
                &objects,
                &sidecar,
                table::STROKE_SIDECAR_TOP_ROWS,
                BorderAxis::TopRow,
                &mut budget,
                &mut text_budget,
            )
            .unwrap_err();
        assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));
    }

    #[test]
    fn max_stroke_layers_matches_max_table_four_edge_axes_and_rejects_excess() {
        assert_eq!(
            MAX_STROKE_LAYERS,
            (MAX_TABLE_ROWS + 1) + MAX_TABLE_ROWS + (MAX_TABLE_COLS + 1) + MAX_TABLE_COLS
        );
        let mut objects = ObjectTable::default();
        objects.objects.insert(
            1,
            ArchiveObject {
                type_id: ty::STROKE_LAYER,
                payload: f_varint(table::STROKE_LAYER_INDEX, 0).into(),
                extra: Vec::new(),
            },
        );
        let entry = f_ref(table::STROKE_SIDECAR_TOP_ROWS, 1);
        let mut ok_payload = Vec::with_capacity(entry.len() * 20_512);
        for _ in 0..20_512 {
            ok_payload.extend_from_slice(&entry);
        }
        let ok_sidecar = Message::parse(&ok_payload).unwrap();
        let mut acc = StrokeLayerAcc::new(MAX_TABLE_ROWS, MAX_TABLE_COLS);
        let mut budget = TableBudget::default();
        acc.apply_layers(
            &objects,
            &ok_sidecar,
            table::STROKE_SIDECAR_TOP_ROWS,
            BorderAxis::TopRow,
            &mut budget,
            &mut DecodeBudget::new(),
        )
        .expect("max 10_000 x 256 table 4-edge layer count must succeed");

        let mut over_payload = ok_payload;
        for _ in 0..3 {
            over_payload.extend_from_slice(&entry);
        }
        let over_sidecar = Message::parse(&over_payload).unwrap();
        let mut acc2 = StrokeLayerAcc::new(MAX_TABLE_ROWS, MAX_TABLE_COLS);
        let mut budget2 = TableBudget::default();
        let err = acc2
            .apply_layers(
                &objects,
                &over_sidecar,
                table::STROKE_SIDECAR_TOP_ROWS,
                BorderAxis::TopRow,
                &mut budget2,
                &mut DecodeBudget::new(),
            )
            .unwrap_err();
        assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));
    }
}

//! Table reader for the Pages reader: model dimensions, tiles of packed cells, rich-text payloads
//! and cell formats mapped to a grid of display strings, cell hyperlinks and inline cell objects.
//! Packed cells are decoded in place from slices of the decoded buffers ([`packed`]); all limits
//! are errors or skips, never silent truncation.

use std::rc::Rc;

use self::cell_rich::{decode_rich_cell_storage, RichCellDecoded, MAX_CELL_MARK_ENTRIES};
pub(crate) use self::format::format_iso_date;
use self::format::{render_value, FormatSpec};
use self::packed::{parse_cell, CellScalar, PackedCell};
use self::parsed::ParsedObjects;
use self::styles::{assemble_cell_styles, read_cell_fills, read_col_widths_twips};
mod cell_rich;
mod format;
mod merge;
mod packed;
mod parsed;
mod styles;

use crate::decode_budget::DecodeBudget;
use crate::pages_proto::{read_reference, ArchiveObject, FastU64Map, Message, ObjectTable};
use crate::pages_schema::{cell, table, ty};
use crate::rich_read_pages_image::PageImages;
use crate::rich_table_style::{CellFill, TableCellStyleEntry, MAX_STYLED_TABLE_CELLS};
use crate::{CellMerge, EasyOfficeError, TableCellLink};

const MAX_TABLES: usize = 2_000;
pub(crate) const MAX_TABLE_ROWS: usize = 10_000;
pub(crate) const MAX_TABLE_COLS: usize = 256;
pub(crate) const MAX_TABLE_CELLS: usize = 2_000_000;
const MAX_TOTAL_CELLS: usize = 4_000_000;
const MAX_LIST_ENTRIES: usize = 2_000_000;
const MAX_TOTAL_LIST_ENTRIES: usize = 2_000_000;
const MAX_TOTAL_STROKE_LAYERS: usize = 200_000;
const MAX_TOTAL_STROKE_RUNS: usize = 2_000_000;
const MAX_TOTAL_STROKE_CELLS: usize = 4_000_000;
const MAX_TOTAL_STYLED_CELLS: usize = 2_000_000;
const MAX_TILE_ROWS_PER_TABLE: usize = MAX_TABLE_ROWS * 2;
const MAX_TILE_CELLS_PER_TABLE: usize = MAX_TABLE_CELLS * 2;
const MAX_TOTAL_TILE_ROWS: usize = MAX_TOTAL_CELLS;
const MAX_TOTAL_TILE_CELLS: usize = MAX_TOTAL_CELLS * 2;
const MAX_PAYLOAD_CACHE: usize = 65_536;
const CELL_TEXT_OVERHEAD: usize = 16;
const MAX_TILE_ROWS: u64 = 65_536;

/// Table counters shared by every table and chart of one document.
#[derive(Debug, Default)]
pub(crate) struct TableBudget {
    tables: usize,
    total_cells: usize,
    total_list_entries: usize,
    total_stroke_layers: usize,
    total_stroke_runs: usize,
    total_stroke_cells: usize,
    total_styled_cells: usize,
    total_tile_rows: usize,
    total_tile_cells: usize,
}

impl TableBudget {
    pub(crate) fn charge_table(&mut self) -> Result<(), EasyOfficeError> {
        self.tables = self.tables.saturating_add(1);
        if self.tables > MAX_TABLES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 수 한도 초과".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn charge_cells(&mut self, cells: usize) -> Result<(), EasyOfficeError> {
        self.total_cells = self
            .total_cells
            .checked_add(cells)
            .filter(|total| *total <= MAX_TOTAL_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded("Pages 표 셀 총수 한도 초과".to_string())
            })?;
        Ok(())
    }

    fn charge_list_entry(&mut self) -> Result<(), EasyOfficeError> {
        self.total_list_entries = self
            .total_list_entries
            .checked_add(1)
            .filter(|total| *total <= MAX_TOTAL_LIST_ENTRIES)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 목록 항목 수 한도 초과".to_string(),
                )
            })?;
        Ok(())
    }

    pub(super) fn charge_stroke_layer(&mut self) -> Result<(), EasyOfficeError> {
        self.total_stroke_layers = self
            .total_stroke_layers
            .checked_add(1)
            .filter(|total| *total <= MAX_TOTAL_STROKE_LAYERS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 테두리 레이어 총수 한도 초과".to_string(),
                )
            })?;
        Ok(())
    }

    pub(super) fn charge_stroke_run(&mut self) -> Result<(), EasyOfficeError> {
        self.total_stroke_runs = self
            .total_stroke_runs
            .checked_add(1)
            .filter(|total| *total <= MAX_TOTAL_STROKE_RUNS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 테두리 런 총수 한도 초과".to_string(),
                )
            })?;
        Ok(())
    }

    pub(super) fn charge_stroke_cells(&mut self, cells: usize) -> Result<(), EasyOfficeError> {
        self.total_stroke_cells = self
            .total_stroke_cells
            .checked_add(cells)
            .filter(|total| *total <= MAX_TOTAL_STROKE_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 테두리 확장 셀 총수 한도 초과".to_string(),
                )
            })?;
        Ok(())
    }

    pub(super) fn charge_styled_cells(&mut self, cells: usize) -> Result<(), EasyOfficeError> {
        self.total_styled_cells = self
            .total_styled_cells
            .checked_add(cells)
            .filter(|total| *total <= MAX_TOTAL_STYLED_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 셀 스타일 총수 한도 초과".to_string(),
                )
            })?;
        Ok(())
    }

    fn charge_tile_row(&mut self, cells: usize) -> Result<(), EasyOfficeError> {
        self.total_tile_rows = self
            .total_tile_rows
            .checked_add(1)
            .filter(|total| *total <= MAX_TOTAL_TILE_ROWS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 타일 행 총수 한도 초과".to_string(),
                )
            })?;
        self.total_tile_cells = self
            .total_tile_cells
            .checked_add(cells)
            .filter(|total| *total <= MAX_TOTAL_TILE_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 타일 셀 총수 한도 초과".to_string(),
                )
            })?;
        Ok(())
    }
}

/// A decoded table: display text per cell, merged regions, cell hyperlinks and extracted pictures.
#[derive(Debug)]
pub(crate) struct TableRead {
    pub grid: Vec<Vec<String>>,
    /// Merged regions as declared by the file (validated by the rich table).
    pub merges: Vec<CellMerge>,
    /// Cell-level hyperlinks indexed by UTF-8 byte offsets inside `grid[row][col]`.
    pub links: Vec<TableCellLink>,
    /// Table column widths in twips, when present in model/header bucket.
    pub col_widths_twips: Vec<u32>,
    /// Per-cell background fill and border overrides sorted by `(row, col)`.
    pub cell_styles: Vec<TableCellStyleEntry>,
    /// Picture drawables (`(object_id, fallback_equation_alt)`) anchored inside cells, in cell order.
    pub cell_images: Vec<(u64, Option<String>)>,
    pub unsupported: usize,
    /// Attachment marks found inside cell text that could not be imported.
    pub cell_objects: usize,
    /// Hyperlink marks inside cells whose scheme or syntax was rejected.
    pub unsafe_links: usize,
}

#[derive(Debug)]
pub(crate) enum TableOutcome {
    Read(TableRead),
    /// The table has no rows or columns.
    Absent,
    Skipped(&'static str),
}

fn reference(m: &Message, field: u32) -> Option<u64> {
    m.bytes(field).and_then(read_reference)
}

/// Sorted `(key, value)` pairs for fast lookup by key.
type KeyedList<T> = Vec<(u64, T)>;

fn lookup<T: Copy>(list: &[(u64, T)], key: u64) -> Option<T> {
    list.binary_search_by_key(&key, |(k, _)| *k)
        .ok()
        .map(|i| list[i].1)
}

/// Entries `(key, entry message field)` of a data list object, sorted by key.
fn data_list<'a, T>(
    objects: &'a ObjectTable,
    list_id: u64,
    entry_field: u32,
    budget: &mut TableBudget,
    convert: impl Fn(&Message<'a>) -> Option<T>,
) -> Result<KeyedList<T>, EasyOfficeError> {
    let mut out = KeyedList::new();
    let Some(obj) = objects
        .get(list_id)
        .filter(|o| o.type_id == ty::TABLE_DATA_LIST)
    else {
        return Ok(out);
    };
    let Ok(list) = Message::parse(&obj.payload) else {
        return Ok(out);
    };
    let mut seen = 0usize;
    let res = list.for_each_sub(table::LIST_ENTRIES, |e| {
        if seen >= MAX_LIST_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 목록 항목 수 한도 초과".to_string(),
            ));
        }
        seen += 1;
        budget.charge_list_entry()?;
        if let Some(key) = e.varint(table::ENTRY_KEY) {
            if let Some(v) = e.sub(entry_field).ok().flatten().and_then(|m| convert(&m)) {
                out.push((key, v));
            }
        }
        Ok(())
    });
    if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = res {
        return Err(err);
    }
    out.sort_by_key(|(k, _)| *k);
    Ok(out)
}

/// Entries `(key, text)` of a string list object (entry text = field 3), sorted by key.
fn string_list<'a>(
    objects: &'a ObjectTable,
    list_id: u64,
    budget: &mut TableBudget,
) -> Result<KeyedList<&'a str>, EasyOfficeError> {
    let mut out = KeyedList::new();
    let Some(obj) = objects
        .get(list_id)
        .filter(|o| o.type_id == ty::TABLE_DATA_LIST)
    else {
        return Ok(out);
    };
    let Ok(list) = Message::parse(&obj.payload) else {
        return Ok(out);
    };
    let mut seen = 0usize;
    let res = list.for_each_sub(table::LIST_ENTRIES, |e| {
        if seen >= MAX_LIST_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 문자열 목록 한도 초과".to_string(),
            ));
        }
        seen += 1;
        budget.charge_list_entry()?;
        let text = e
            .bytes(table::ENTRY_STRING)
            .and_then(|b| std::str::from_utf8(b).ok());
        if let (Some(key), Some(text)) = (e.varint(table::ENTRY_KEY), text) {
            out.push((key, text));
        }
        Ok(())
    });
    if let Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) = res {
        return Err(err);
    }
    out.sort_by_key(|(k, _)| *k);
    Ok(out)
}

/// Everything needed to turn packed cells into text.
struct CellContext<'a> {
    objects: &'a ObjectTable,
    rich_text: KeyedList<u64>,
    strings: KeyedList<&'a str>,
    formats: KeyedList<FormatSpec<'a>>,
    cell_fills: KeyedList<CellFill>,
    payload_cache: FastU64Map<Rc<RichCellDecoded>>,
    links: Vec<TableCellLink>,
    cell_fills_out: Vec<(u32, u32, CellFill)>,
    cell_images: Vec<(u64, Option<String>)>,
    cell_objects: usize,
    unsafe_links: usize,
}

enum Rendered {
    Text(String),
    Unsupported,
}

impl<'a> CellContext<'a> {
    fn new(
        objects: &'a ObjectTable,
        store: &Message<'a>,
        budget: &mut TableBudget,
    ) -> Result<Self, EasyOfficeError> {
        let list_of = |field: u32| reference(store, field);
        let rich_text = match list_of(table::STORE_RICH_TEXT_LIST) {
            Some(id) => data_list(objects, id, table::ENTRY_RICH_TEXT, budget, |m| m.varint(1))?,
            None => Vec::new(),
        };
        let formats = match list_of(table::STORE_FORMAT_LIST) {
            Some(id) => data_list(objects, id, table::ENTRY_FORMAT, budget, |m| {
                Some(FormatSpec::parse(m))
            })?,
            None => Vec::new(),
        };
        let strings = match list_of(table::STORE_STRING_LIST) {
            Some(id) => string_list(objects, id, budget)?,
            None => Vec::new(),
        };
        let cell_fills = read_cell_fills(objects, store, budget)?;
        Ok(Self {
            objects,
            rich_text,
            strings,
            formats,
            cell_fills,
            payload_cache: FastU64Map::default(),
            links: Vec::new(),
            cell_fills_out: Vec::new(),
            cell_images: Vec::new(),
            cell_objects: 0,
            unsafe_links: 0,
        })
    }

    fn payload_decoded(
        &mut self,
        payload_id: u64,
        images: &mut PageImages<'_>,
    ) -> Result<Rc<RichCellDecoded>, EasyOfficeError> {
        if let Some(cached) = self.payload_cache.get(&payload_id) {
            return Ok(Rc::clone(cached));
        }
        let storage = self
            .objects
            .get(payload_id)
            .filter(|o| o.type_id == ty::RICH_TEXT_PAYLOAD)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| reference(&m, table::PAYLOAD_STORAGE))
            .and_then(|id| self.objects.get(id))
            .filter(|o| o.type_id == ty::STORAGE)
            .and_then(|o| Message::parse(&o.payload).ok());
        let decoded = match storage {
            Some(st) => decode_rich_cell_storage(self.objects, images, &st)?,
            None => RichCellDecoded::default(),
        };
        let rc = Rc::new(decoded);
        if self.payload_cache.len() < MAX_PAYLOAD_CACHE {
            self.payload_cache.insert(payload_id, Rc::clone(&rc));
        }
        Ok(rc)
    }

    fn render(
        &mut self,
        row: usize,
        col: usize,
        packed: &PackedCell,
        images: &mut PageImages<'_>,
        text_budget: &mut DecodeBudget,
    ) -> Result<Rendered, EasyOfficeError> {
        if packed.kind == cell::TYPE_EMPTY {
            return Ok(Rendered::Text(String::new()));
        }
        if let Some(key) = packed.rich_text_key {
            let Some(payload) = lookup(&self.rich_text, u64::from(key)) else {
                return Ok(Rendered::Unsupported);
            };
            let decoded = self.payload_decoded(payload, images)?;
            self.cell_objects = self.cell_objects.saturating_add(decoded.cell_objects);
            self.unsafe_links = self.unsafe_links.saturating_add(decoded.unsafe_links);
            for (start, end, url) in &decoded.links {
                if self.links.len() >= MAX_CELL_MARK_ENTRIES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 표 셀 하이퍼링크 총수 한도 초과".to_string(),
                    ));
                }
                text_budget.charge(url.len(), "Pages 표 셀 하이퍼링크")?;
                self.links.push(TableCellLink {
                    row: row as u32,
                    col: col as u32,
                    start: *start,
                    end: *end,
                    url: url.clone(),
                });
            }
            for (did, eq_alt) in &decoded.images {
                self.cell_images.push((*did, eq_alt.clone()));
            }
            if !decoded.text.is_empty() {
                text_budget.charge(
                    decoded.text.len().saturating_add(CELL_TEXT_OVERHEAD),
                    "Pages 표 서식 셀 텍스트",
                )?;
            }
            return Ok(Rendered::Text(decoded.text.clone()));
        }
        if matches!(packed.kind, cell::TYPE_STRING | cell::TYPE_ERROR) {
            let Some(text) = packed
                .string_key
                .and_then(|key| lookup(&self.strings, u64::from(key)))
            else {
                return Ok(Rendered::Unsupported);
            };
            if !text.is_empty() {
                text_budget.charge(
                    text.len().saturating_add(CELL_TEXT_OVERHEAD),
                    "Pages 표 문자열 셀",
                )?;
            }
            return Ok(Rendered::Text(text.to_string()));
        }
        if packed.value == CellScalar::None {
            return Ok(Rendered::Unsupported);
        }
        let spec = packed
            .format_key
            .and_then(|k| lookup(&self.formats, u64::from(k)));
        let Some(text) = render_value(packed, spec.as_ref()) else {
            return Ok(Rendered::Unsupported);
        };
        if !text.is_empty() {
            text_budget.charge(
                text.len().saturating_add(CELL_TEXT_OVERHEAD),
                "Pages 표 값 셀",
            )?;
        }
        Ok(Rendered::Text(text))
    }
}

enum TableDimErr {
    Absent,
    Skipped(&'static str),
}

/// Row and column counts of a table model, or the reason it is not read.
fn dimensions(model: &Message) -> Result<(usize, usize), TableDimErr> {
    let count = |field: u32| {
        model
            .varint(field)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(usize::MAX)
    };
    let rows = model
        .varint(table::MODEL_ROWS)
        .map_or(0, |_| count(table::MODEL_ROWS));
    let cols = model
        .varint(table::MODEL_COLS)
        .map_or(0, |_| count(table::MODEL_COLS));
    if rows == 0 || cols == 0 {
        return Err(TableDimErr::Absent);
    }
    let fits = rows <= MAX_TABLE_ROWS
        && cols <= MAX_TABLE_COLS
        && rows.checked_mul(cols).is_some_and(|n| n <= MAX_TABLE_CELLS);
    if fits {
        Ok((rows, cols))
    } else {
        Err(TableDimErr::Skipped("너무 큰 표"))
    }
}

/// Reads one row of a tile into `grid`; returns the number of unsupported cells.
fn fill_row(
    ctx: &mut CellContext,
    row_idx: usize,
    row: &Message,
    cols: usize,
    cells: &mut [String],
    images: &mut PageImages<'_>,
    text_budget: &mut DecodeBudget,
) -> Result<usize, EasyOfficeError> {
    let buf = row.bytes(table::ROW_CELL_BUFFER).unwrap_or(&[]);
    let offsets = row.bytes(table::ROW_CELL_OFFSETS).unwrap_or(&[]);
    let mut unsupported = 0usize;
    for (col_idx, (slot, pair)) in cells
        .iter_mut()
        .zip(offsets.as_chunks::<2>().0.iter().take(cols))
        .enumerate()
    {
        let offset = u16::from_le_bytes(*pair);
        if offset == table::NO_CELL_OFFSET {
            continue;
        }
        let rendered = match parse_cell(buf, usize::from(offset)) {
            Some(p) => {
                if let Some(style_key) = p.cell_style_key {
                    if let Some(fill) = lookup(&ctx.cell_fills, u64::from(style_key)) {
                        if !fill.is_none() && ctx.cell_fills_out.len() < MAX_STYLED_TABLE_CELLS {
                            ctx.cell_fills_out
                                .push((row_idx as u32, col_idx as u32, fill));
                        }
                    }
                }
                Some(ctx.render(row_idx, col_idx, &p, images, text_budget)?)
            }
            None => None,
        };
        match rendered {
            Some(Rendered::Text(text)) => *slot = text,
            _ => unsupported += 1,
        }
    }
    Ok(unsupported)
}

/// Absolute row index of row `index` of tile `tile_id`.
fn absolute_row(tile_id: u64, tile_rows: u64, index: u64) -> Option<usize> {
    let abs = tile_id.checked_mul(tile_rows)?.checked_add(index)?;
    usize::try_from(abs).ok()
}

fn fill_tiles<'a>(
    ctx: &mut CellContext<'a>,
    tiles: &Message,
    grid: &mut [Vec<String>],
    cols: usize,
    budget: &mut TableBudget,
    images: &mut PageImages<'_>,
    text_budget: &mut DecodeBudget,
) -> Result<usize, EasyOfficeError> {
    let tile_rows = tiles
        .varint(table::TILES_SIZE)
        .filter(|v| *v > 0 && *v <= MAX_TILE_ROWS)
        .unwrap_or(table::DEFAULT_TILE_ROWS);
    let mut unsupported = 0usize;
    let mut tile_entries_seen = 0usize;
    let mut tile_rows_seen = 0usize;
    let mut tile_cells_seen = 0usize;
    let mut parsed_tiles = ParsedObjects::default();
    tiles.for_each_sub(table::TILES_ENTRIES, |entry| {
        if tile_entries_seen >= MAX_TILE_ROWS_PER_TABLE {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 타일 항목 수 한도 초과".to_string(),
            ));
        }
        tile_entries_seen += 1;
        budget.charge_tile_row(0)?;
        let tile_id = entry.varint(table::TILE_ID).unwrap_or(0);
        let tile_ref = reference(&entry, table::TILE_REF).and_then(|id| {
            let object = ctx.objects.get(id)?;
            (object.type_id == ty::TABLE_TILE || object.type_id == ty::TABLE_TILE_V2)
                .then_some((id, object))
        });
        let Some((tile_ref, tile_obj)) = tile_ref else {
            return Ok(());
        };
        let Some(tile) = parsed_tiles.get(tile_ref, tile_obj, text_budget, "Pages 표 타일")?
        else {
            return Err(EasyOfficeError::CorruptedContainer(
                "Pages 표 타일을 읽을 수 없습니다".to_string(),
            ));
        };
        tile.for_each_sub(table::TILE_ROWS, |row| {
            if tile_rows_seen >= MAX_TILE_ROWS_PER_TABLE {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 표 타일 행 수 한도 초과".to_string(),
                ));
            }
            tile_rows_seen += 1;
            let abs_row = row
                .varint(table::ROW_INDEX)
                .and_then(|i| absolute_row(tile_id, tile_rows, i));
            let in_grid = abs_row.is_some_and(|r| r < grid.len());
            let row_cells = if in_grid { cols } else { 0 };
            tile_cells_seen = tile_cells_seen
                .checked_add(row_cells)
                .filter(|total| *total <= MAX_TILE_CELLS_PER_TABLE)
                .ok_or_else(|| {
                    EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 표 타일 셀 수 한도 초과".to_string(),
                    )
                })?;
            budget.charge_tile_row(row_cells)?;
            if let Some(r_idx) = abs_row {
                if let Some(cells) = grid.get_mut(r_idx) {
                    unsupported += fill_row(ctx, r_idx, &row, cols, cells, images, text_budget)?;
                }
            }
            Ok(())
        })
    })?;
    Ok(unsupported)
}

/// Reads the table described by a table-info object.
pub(crate) fn read_table(
    objects: &ObjectTable,
    info: &ArchiveObject,
    budget: &mut TableBudget,
    images: &mut PageImages<'_>,
    text_budget: &mut DecodeBudget,
) -> Result<TableOutcome, EasyOfficeError> {
    budget.charge_table()?;
    let info_msg = Message::parse(&info.payload)?;
    let Some(model) = reference(&info_msg, table::INFO_MODEL)
        .and_then(|id| objects.get(id))
        .filter(|o| o.type_id == ty::TABLE_MODEL)
    else {
        return Ok(TableOutcome::Skipped("읽을 수 없는 표"));
    };
    let model_msg = Message::parse(&model.payload)?;
    let (rows, cols) = match dimensions(&model_msg) {
        Ok(dim) => dim,
        Err(TableDimErr::Absent) => return Ok(TableOutcome::Absent),
        Err(TableDimErr::Skipped(reason)) => return Ok(TableOutcome::Skipped(reason)),
    };
    budget.charge_cells(rows.checked_mul(cols).ok_or_else(|| {
        EasyOfficeError::ResourceBudgetExceeded("Pages 표 셀 수 계산 오버플로".to_string())
    })?)?;
    let Some(store) = model_msg.sub(table::MODEL_DATA_STORE)? else {
        return Ok(TableOutcome::Skipped("읽을 수 없는 표"));
    };
    let mut ctx = CellContext::new(objects, &store, budget)?;
    let header_cost = rows.saturating_mul(std::mem::size_of::<Vec<String>>());
    let slots_cost = rows
        .saturating_mul(cols)
        .saturating_mul(std::mem::size_of::<String>());
    text_budget.charge(
        header_cost.saturating_add(slots_cost),
        "Pages 표 그리드 슬롯",
    )?;
    let mut grid = vec![vec![String::new(); cols]; rows];
    let unsupported = match store.sub(table::STORE_TILES)? {
        Some(tiles) => fill_tiles(
            &mut ctx,
            &tiles,
            &mut grid,
            cols,
            budget,
            images,
            text_budget,
        )?,
        None => 0,
    };
    let merges = merge::read_merges(objects, &model_msg, &store);
    let col_widths_twips = read_col_widths_twips(objects, &model_msg, &store, cols)?;
    let cell_styles = assemble_cell_styles(
        objects,
        &model_msg,
        rows,
        cols,
        &ctx.cell_fills_out,
        budget,
        text_budget,
    )?;
    Ok(TableOutcome::Read(TableRead {
        grid,
        merges,
        links: ctx.links,
        col_widths_twips,
        cell_styles,
        cell_images: ctx.cell_images,
        unsupported,
        cell_objects: ctx.cell_objects,
        unsafe_links: ctx.unsafe_links,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages_schema::storage;
    use std::collections::HashMap;

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

    fn insert_obj(t: &mut ObjectTable, id: u64, type_id: u32, payload: Vec<u8>) {
        t.objects.insert(
            id,
            ArchiveObject {
                type_id,
                payload: payload.into(),
                extra: Vec::new(),
            },
        );
    }

    /// A table of one row whose single tile has `padding` unknown fields (3 bytes each) and is
    /// named by `entries` tile entries; returns the table-info object.
    fn shared_tile_table(
        objects: &mut ObjectTable,
        entries: usize,
        padding: usize,
    ) -> ArchiveObject {
        let offsets = table::NO_CELL_OFFSET.to_le_bytes();
        let row = [
            f_varint(table::ROW_INDEX, 0),
            f_bytes(table::ROW_CELL_OFFSETS, &offsets),
        ]
        .concat();
        let mut tile = f_bytes(table::TILE_ROWS, &row);
        for _ in 0..padding {
            tile.extend_from_slice(&f_varint(99, 1));
        }
        insert_obj(objects, 13, ty::TABLE_TILE, tile);
        let entry = [f_varint(table::TILE_ID, 0), f_ref(table::TILE_REF, 13)].concat();
        let mut tiles = f_varint(table::TILES_SIZE, 1);
        for _ in 0..entries {
            tiles.extend_from_slice(&f_bytes(table::TILES_ENTRIES, &entry));
        }
        let store = f_bytes(table::STORE_TILES, &tiles);
        let model = [
            f_varint(table::MODEL_ROWS, 1),
            f_varint(table::MODEL_COLS, 1),
            f_bytes(table::MODEL_DATA_STORE, &store),
        ]
        .concat();
        insert_obj(objects, 14, ty::TABLE_MODEL, model);
        insert_obj(objects, 15, ty::TABLE_INFO, f_ref(table::INFO_MODEL, 14));
        objects.get(15).unwrap().clone()
    }

    #[test]
    fn one_tile_named_by_many_entries_is_parsed_once() {
        let mut objects = ObjectTable::default();
        let info = shared_tile_table(&mut objects, 19_000, 30_000);
        let files = HashMap::new();
        let mut images = PageImages::new(&objects, &files);
        let mut budget = TableBudget::default();
        let mut text_budget = DecodeBudget::new();
        let outcome = read_table(&objects, &info, &mut budget, &mut images, &mut text_budget);
        // a parse per entry would cost 19,000 payloads and their fields, far over the budget
        assert!(matches!(outcome, Ok(TableOutcome::Read(_))));
        // the tile payload is paid for once, not once per entry
        let used = DecodeBudget::new().remaining() - text_budget.remaining();
        assert!(used < 2 * 1024 * 1024, "{used}");
    }

    #[test]
    fn a_tile_payload_is_charged_to_the_decoded_byte_budget() {
        let mut objects = ObjectTable::default();
        let info = shared_tile_table(&mut objects, 3, 30_000);
        let files = HashMap::new();
        let mut images = PageImages::new(&objects, &files);
        let mut budget = TableBudget::default();
        // room for the grid slots, not for the 90 KB tile
        let mut text_budget = DecodeBudget::with_limit(8 * 1024);
        assert!(matches!(
            read_table(&objects, &info, &mut budget, &mut images, &mut text_budget),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }

    #[test]
    fn repeated_rich_text_cell_uses_cache_and_trips_decode_budget() {
        let mut objects = ObjectTable::default();
        let text_chunk = "R".repeat(16 * 1024);
        insert_obj(
            &mut objects,
            10,
            ty::STORAGE,
            f_bytes(storage::TEXT, text_chunk.as_bytes()),
        );
        insert_obj(
            &mut objects,
            11,
            ty::RICH_TEXT_PAYLOAD,
            f_ref(table::PAYLOAD_STORAGE, 10),
        );
        let rt_entry = [
            f_varint(table::ENTRY_KEY, 1),
            f_ref(table::ENTRY_RICH_TEXT, 11),
        ]
        .concat();
        insert_obj(
            &mut objects,
            12,
            ty::TABLE_DATA_LIST,
            f_bytes(table::LIST_ENTRIES, &rt_entry),
        );

        let mut cell_buf = vec![cell::CURRENT_VERSION, cell::TYPE_RICH_TEXT, 0, 0];
        cell_buf.extend_from_slice(&(1u32 << cell::RICH_TEXT_KEY_BIT).to_le_bytes());
        cell_buf.extend_from_slice(&1u32.to_le_bytes());
        let cols = 10usize;
        let offsets: Vec<u8> = std::iter::repeat_n(0u16.to_le_bytes(), cols)
            .flatten()
            .collect();

        let make_table = |objects: &mut ObjectTable, rows: usize| {
            let mut tile_payload = Vec::new();
            for r in 0..rows {
                let row_msg = [
                    f_varint(table::ROW_INDEX, r as u64),
                    f_bytes(table::ROW_CELL_BUFFER, &cell_buf),
                    f_bytes(table::ROW_CELL_OFFSETS, &offsets),
                ]
                .concat();
                tile_payload.extend_from_slice(&f_bytes(table::TILE_ROWS, &row_msg));
            }
            insert_obj(objects, 13, ty::TABLE_TILE, tile_payload);
            let tile_entry = [f_varint(table::TILE_ID, 0), f_ref(table::TILE_REF, 13)].concat();
            let tiles_msg = [
                f_varint(table::TILES_SIZE, rows.max(1) as u64),
                f_bytes(table::TILES_ENTRIES, &tile_entry),
            ]
            .concat();
            let store_msg = [
                f_ref(table::STORE_RICH_TEXT_LIST, 12),
                f_bytes(table::STORE_TILES, &tiles_msg),
            ]
            .concat();
            let model_msg = [
                f_varint(table::MODEL_ROWS, rows as u64),
                f_varint(table::MODEL_COLS, cols as u64),
                f_bytes(table::MODEL_DATA_STORE, &store_msg),
            ]
            .concat();
            insert_obj(objects, 14, ty::TABLE_MODEL, model_msg);
            insert_obj(objects, 15, ty::TABLE_INFO, f_ref(table::INFO_MODEL, 14));
        };

        let files = HashMap::new();
        make_table(&mut objects, 1);
        let info_ok = objects.get(15).unwrap().clone();
        let mut tbudget = TableBudget::default();
        let mut imgs1 = PageImages::new(&objects, &files);
        let mut dbudget = DecodeBudget::with_limit(1024 * 1024);
        let TableOutcome::Read(read) =
            read_table(&objects, &info_ok, &mut tbudget, &mut imgs1, &mut dbudget).unwrap()
        else {
            panic!("expected TableOutcome::Read");
        };
        assert_eq!(read.grid.len(), 1);
        assert_eq!(read.grid[0].len(), cols);
        assert!(read.grid[0].iter().all(|c| c.len() == 16 * 1024));

        make_table(&mut objects, 80);
        let info_over = objects.get(15).unwrap().clone();
        let mut tbudget2 = TableBudget::default();
        let mut imgs2 = PageImages::new(&objects, &files);
        let mut dbudget2 = DecodeBudget::with_limit(1024 * 1024);
        assert!(matches!(
            read_table(
                &objects,
                &info_over,
                &mut tbudget2,
                &mut imgs2,
                &mut dbudget2
            ),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}

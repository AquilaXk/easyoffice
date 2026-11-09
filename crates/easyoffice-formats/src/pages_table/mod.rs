//! Table reader for the Pages reader: model dimensions, tiles of packed cells, rich-text payloads
//! and cell formats mapped to a grid of display strings. Packed cells are decoded in place from
//! slices of the decoded buffers ([`packed`]); the only allocation per cell is its
//! final text. All limits are errors or skips, never silent truncation.

use self::format::{render_value, FormatSpec};
use self::packed::{parse_cell, CellScalar, PackedCell};
mod format;
mod merge;
mod packed;

use crate::pages_proto::{read_reference, ArchiveObject, Message, ObjectTable};
use crate::pages_schema::{cell, table, ty};
use crate::{CellMerge, EasyOfficeError};

const MAX_TABLES: usize = 2_000;
const MAX_TABLE_ROWS: usize = 10_000;
const MAX_TABLE_COLS: usize = 256;
const MAX_TABLE_CELLS: usize = 2_000_000;
const MAX_TOTAL_CELLS: usize = 4_000_000;
const MAX_LIST_ENTRIES: usize = 2_000_000;
const MAX_TILE_ROWS: u64 = 65_536;

/// Table counters shared by every table of one document.
#[derive(Debug, Default)]
pub(crate) struct TableBudget {
    tables: usize,
    total_cells: usize,
}

impl TableBudget {
    fn charge_table(&mut self) -> Result<(), EasyOfficeError> {
        self.tables = self.tables.saturating_add(1);
        if self.tables > MAX_TABLES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 수 한도 초과".to_string(),
            ));
        }
        Ok(())
    }

    fn charge_cells(&mut self, cells: usize) -> Result<(), EasyOfficeError> {
        self.total_cells = self
            .total_cells
            .checked_add(cells)
            .filter(|total| *total <= MAX_TOTAL_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded("Pages 표 셀 총수 한도 초과".to_string())
            })?;
        Ok(())
    }
}

/// A decoded table: display text per cell and the number of cells whose value was not shown.
#[derive(Debug)]
pub(crate) struct TableRead {
    pub grid: Vec<Vec<String>>,
    /// Merged regions as declared by the file (validated by the rich table).
    pub merges: Vec<CellMerge>,
    pub unsupported: usize,
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

/// Text of a rich-text cell storage: control characters and attachment marks removed, line
/// separators turned into spaces, trailing paragraph breaks trimmed.
pub(crate) fn cell_plain_text(storage: &Message, field: u32) -> Result<String, EasyOfficeError> {
    let raw = storage.text(field)?;
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '\u{2028}' => out.push(' '),
            '\n' | '\t' => out.push(c),
            '\u{FFFC}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.truncate(out.trim_end_matches('\n').len());
    Ok(out)
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
    convert: impl Fn(&Message<'a>) -> Option<T>,
) -> KeyedList<T> {
    let mut out = KeyedList::new();
    let Some(obj) = objects
        .get(list_id)
        .filter(|o| o.type_id == ty::TABLE_DATA_LIST)
    else {
        return out;
    };
    let Ok(list) = Message::parse(&obj.payload) else {
        return out;
    };
    let mut seen = 0usize;
    let _ = list.for_each_sub(table::LIST_ENTRIES, |e| {
        if seen < MAX_LIST_ENTRIES {
            seen += 1;
            if let Some(key) = e.varint(table::ENTRY_KEY) {
                if let Some(v) = e.sub(entry_field).ok().flatten().and_then(|m| convert(&m)) {
                    out.push((key, v));
                }
            }
        }
        Ok(())
    });
    out.sort_by_key(|(k, _)| *k);
    out
}

/// Entries `(key, text)` of a string list object (entry text = field 3), sorted by key.
fn string_list(objects: &ObjectTable, list_id: u64) -> KeyedList<&str> {
    let mut out = KeyedList::new();
    let Some(obj) = objects
        .get(list_id)
        .filter(|o| o.type_id == ty::TABLE_DATA_LIST)
    else {
        return out;
    };
    let Ok(list) = Message::parse(&obj.payload) else {
        return out;
    };
    let mut seen = 0usize;
    let _ = list.for_each_sub(table::LIST_ENTRIES, |e| {
        if seen < MAX_LIST_ENTRIES {
            seen += 1;
            let text = e
                .bytes(table::ENTRY_STRING)
                .and_then(|b| std::str::from_utf8(b).ok());
            if let (Some(key), Some(text)) = (e.varint(table::ENTRY_KEY), text) {
                out.push((key, text));
            }
        }
        Ok(())
    });
    out.sort_by_key(|(k, _)| *k);
    out
}

/// Everything needed to turn packed cells into text.
struct CellContext<'a> {
    objects: &'a ObjectTable,
    rich_text: KeyedList<u64>,
    strings: KeyedList<&'a str>,
    formats: KeyedList<FormatSpec<'a>>,
}

enum Rendered {
    Text(String),
    Unsupported,
}

impl<'a> CellContext<'a> {
    fn new(objects: &'a ObjectTable, store: &Message<'a>) -> Self {
        let list_of = |field: u32| reference(store, field);
        let rich_text = list_of(table::STORE_RICH_TEXT_LIST)
            .map(|id| data_list(objects, id, table::ENTRY_RICH_TEXT, |m| m.varint(1)))
            .unwrap_or_default();
        let formats = list_of(table::STORE_FORMAT_LIST)
            .map(|id| {
                data_list(objects, id, table::ENTRY_FORMAT, |m| {
                    Some(FormatSpec::parse(m))
                })
            })
            .unwrap_or_default();
        let strings = list_of(table::STORE_STRING_LIST)
            .map(|id| string_list(objects, id))
            .unwrap_or_default();
        Self {
            objects,
            rich_text,
            strings,
            formats,
        }
    }

    fn payload_text(&self, payload_id: u64) -> String {
        let storage = self
            .objects
            .get(payload_id)
            .filter(|o| o.type_id == ty::RICH_TEXT_PAYLOAD)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| reference(&m, table::PAYLOAD_STORAGE))
            .and_then(|id| self.objects.get(id))
            .filter(|o| o.type_id == ty::STORAGE)
            .and_then(|o| Message::parse(&o.payload).ok());
        storage
            .and_then(|st| cell_plain_text(&st, crate::pages_schema::storage::TEXT).ok())
            .unwrap_or_default()
    }

    fn render(&self, packed: &PackedCell) -> Rendered {
        if packed.kind == cell::TYPE_EMPTY {
            return Rendered::Text(String::new());
        }
        if let Some(key) = packed.rich_text_key {
            return match lookup(&self.rich_text, u64::from(key)) {
                Some(payload) => Rendered::Text(self.payload_text(payload)),
                None => Rendered::Unsupported,
            };
        }
        if matches!(packed.kind, cell::TYPE_STRING | cell::TYPE_ERROR) {
            // String cells show their text; error cells show the cached error text.
            return packed
                .string_key
                .and_then(|key| lookup(&self.strings, u64::from(key)))
                .map_or(Rendered::Unsupported, |text| {
                    Rendered::Text(text.to_string())
                });
        }
        if packed.value == CellScalar::None {
            return Rendered::Unsupported;
        }
        let spec = packed
            .format_key
            .and_then(|k| lookup(&self.formats, u64::from(k)));
        render_value(packed, spec.as_ref()).map_or(Rendered::Unsupported, Rendered::Text)
    }
}

/// Row and column counts of a table model, or the reason it is not read.
fn dimensions(model: &Message) -> Result<(usize, usize), TableOutcome> {
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
        return Err(TableOutcome::Absent);
    }
    let fits = rows <= MAX_TABLE_ROWS
        && cols <= MAX_TABLE_COLS
        && rows.checked_mul(cols).is_some_and(|n| n <= MAX_TABLE_CELLS);
    if fits {
        Ok((rows, cols))
    } else {
        Err(TableOutcome::Skipped("너무 큰 표"))
    }
}

/// Reads one row of a tile into `grid`; returns the number of unsupported cells.
fn fill_row(ctx: &CellContext, row: &Message, cols: usize, cells: &mut [String]) -> usize {
    let buf = row.bytes(table::ROW_CELL_BUFFER).unwrap_or(&[]);
    let offsets = row.bytes(table::ROW_CELL_OFFSETS).unwrap_or(&[]);
    let mut unsupported = 0usize;
    for (slot, pair) in cells.iter_mut().zip(offsets.chunks_exact(2).take(cols)) {
        let offset = u16::from_le_bytes([pair[0], pair[1]]);
        if offset == table::NO_CELL_OFFSET {
            continue;
        }
        let rendered = parse_cell(buf, usize::from(offset)).map(|p| ctx.render(&p));
        match rendered {
            Some(Rendered::Text(text)) => *slot = text,
            _ => unsupported += 1,
        }
    }
    unsupported
}

/// Absolute row index of row `index` of tile `tile_id`.
fn absolute_row(tile_id: u64, tile_rows: u64, index: u64) -> Option<usize> {
    let abs = tile_id.checked_mul(tile_rows)?.checked_add(index)?;
    usize::try_from(abs).ok()
}

fn fill_tiles(
    ctx: &CellContext,
    tiles: &Message,
    grid: &mut [Vec<String>],
    cols: usize,
) -> Result<usize, EasyOfficeError> {
    let tile_rows = tiles
        .varint(table::TILES_SIZE)
        .filter(|v| *v > 0 && *v <= MAX_TILE_ROWS)
        .unwrap_or(table::DEFAULT_TILE_ROWS);
    let mut unsupported = 0usize;
    tiles.for_each_sub(table::TILES_ENTRIES, |entry| {
        let tile_id = entry.varint(table::TILE_ID).unwrap_or(0);
        let tile_obj = reference(&entry, table::TILE_REF)
            .and_then(|id| ctx.objects.get(id))
            .filter(|o| o.type_id == ty::TABLE_TILE || o.type_id == ty::TABLE_TILE_V2);
        let Some(tile_obj) = tile_obj else {
            return Ok(());
        };
        let tile = Message::parse(&tile_obj.payload)?;
        tile.for_each_sub(table::TILE_ROWS, |row| {
            let target = row
                .varint(table::ROW_INDEX)
                .and_then(|i| absolute_row(tile_id, tile_rows, i))
                .and_then(|r| grid.get_mut(r));
            if let Some(cells) = target {
                unsupported += fill_row(ctx, &row, cols, cells);
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
        Err(outcome) => return Ok(outcome),
    };
    budget.charge_cells(rows.checked_mul(cols).ok_or_else(|| {
        EasyOfficeError::ResourceBudgetExceeded("Pages 표 셀 수 계산 오버플로".to_string())
    })?)?;
    let Some(store) = model_msg.sub(table::MODEL_DATA_STORE)? else {
        return Ok(TableOutcome::Skipped("읽을 수 없는 표"));
    };
    let ctx = CellContext::new(objects, &store);
    let mut grid = vec![vec![String::new(); cols]; rows];
    let unsupported = match store.sub(table::STORE_TILES)? {
        Some(tiles) => fill_tiles(&ctx, &tiles, &mut grid, cols)?,
        None => 0,
    };
    let merges = merge::read_merges(objects, &store);
    Ok(TableOutcome::Read(TableRead {
        grid,
        merges,
        unsupported,
    }))
}

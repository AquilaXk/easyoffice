//! Table reader for the Pages reader: model dimensions, tiles of packed cells, rich-text payloads
//! and cell formats mapped to a grid of display strings. Packed cells are decoded in place from
//! slices of the decoded buffers ([`packed`]); the only allocation per cell is its
//! final text. All limits are errors or skips, never silent truncation.

use std::rc::Rc;

use self::format::{render_value, FormatSpec};
use self::packed::{parse_cell, CellScalar, PackedCell};
mod format;
mod merge;
mod packed;

use crate::decode_budget::DecodeBudget;
use crate::pages_proto::{read_reference, ArchiveObject, FastU64Map, Message, ObjectTable};
use crate::pages_schema::{cell, table, ty};
use crate::{CellMerge, EasyOfficeError};

const MAX_TABLES: usize = 2_000;
const MAX_TABLE_ROWS: usize = 10_000;
const MAX_TABLE_COLS: usize = 256;
const MAX_TABLE_CELLS: usize = 2_000_000;
const MAX_TOTAL_CELLS: usize = 4_000_000;
const MAX_LIST_ENTRIES: usize = 2_000_000;
const MAX_TOTAL_LIST_ENTRIES: usize = 2_000_000;
const MAX_PAYLOAD_CACHE: usize = 65_536;
const CELL_TEXT_OVERHEAD: usize = 16;
const MAX_TILE_ROWS: u64 = 65_536;

/// Table counters shared by every table of one document.
#[derive(Debug, Default)]
pub(crate) struct TableBudget {
    tables: usize,
    total_cells: usize,
    total_list_entries: usize,
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
}

/// A decoded table: display text per cell and the number of cells whose value was not shown.
#[derive(Debug)]
pub(crate) struct TableRead {
    pub grid: Vec<Vec<String>>,
    /// Merged regions as declared by the file (validated by the rich table).
    pub merges: Vec<CellMerge>,
    pub unsupported: usize,
    /// Attachment marks (pictures, fields) found inside cell text and not imported.
    pub cell_objects: usize,
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

/// Text of a rich-text cell storage and the number of attachment marks (pictures, fields) that
/// were left out: control characters and attachment marks removed, line separators turned into
/// spaces, trailing paragraph breaks trimmed.
pub(crate) fn cell_plain_text(
    storage: &Message,
    field: u32,
) -> Result<(String, usize), EasyOfficeError> {
    let raw = storage.text(field)?;
    let mut out = String::with_capacity(raw.len());
    let mut attachments = 0usize;
    for c in raw.chars() {
        match c {
            '\u{2028}' => out.push(' '),
            '\n' | '\t' => out.push(c),
            '\u{FFFC}' => attachments += 1,
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.truncate(out.trim_end_matches('\n').len());
    Ok((out, attachments))
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
    payload_cache: FastU64Map<(Rc<str>, usize)>,
    cell_objects: usize,
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
        Ok(Self {
            objects,
            rich_text,
            strings,
            formats,
            payload_cache: FastU64Map::default(),
            cell_objects: 0,
        })
    }

    fn payload_text(&mut self, payload_id: u64) -> (Rc<str>, usize) {
        if let Some(cached) = self.payload_cache.get(&payload_id) {
            return (Rc::clone(&cached.0), cached.1);
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
        let (text, objects) = storage
            .and_then(|st| cell_plain_text(&st, crate::pages_schema::storage::TEXT).ok())
            .unwrap_or_default();
        let rc: Rc<str> = Rc::from(text.into_boxed_str());
        if self.payload_cache.len() < MAX_PAYLOAD_CACHE {
            self.payload_cache
                .insert(payload_id, (Rc::clone(&rc), objects));
        }
        (rc, objects)
    }

    fn render(
        &mut self,
        packed: &PackedCell,
        text_budget: &mut DecodeBudget,
    ) -> Result<Rendered, EasyOfficeError> {
        if packed.kind == cell::TYPE_EMPTY {
            return Ok(Rendered::Text(String::new()));
        }
        if let Some(key) = packed.rich_text_key {
            let Some(payload) = lookup(&self.rich_text, u64::from(key)) else {
                return Ok(Rendered::Unsupported);
            };
            let (rc, objects) = self.payload_text(payload);
            self.cell_objects = self.cell_objects.saturating_add(objects);
            if !rc.is_empty() {
                text_budget.charge(
                    rc.len().saturating_add(CELL_TEXT_OVERHEAD),
                    "Pages 표 서식 셀 텍스트",
                )?;
            }
            return Ok(Rendered::Text(rc.as_ref().to_string()));
        }
        if matches!(packed.kind, cell::TYPE_STRING | cell::TYPE_ERROR) {
            // String cells show their text; error cells show the cached error text.
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
fn fill_row(
    ctx: &mut CellContext,
    row: &Message,
    cols: usize,
    cells: &mut [String],
    text_budget: &mut DecodeBudget,
) -> Result<usize, EasyOfficeError> {
    let buf = row.bytes(table::ROW_CELL_BUFFER).unwrap_or(&[]);
    let offsets = row.bytes(table::ROW_CELL_OFFSETS).unwrap_or(&[]);
    let mut unsupported = 0usize;
    for (slot, pair) in cells.iter_mut().zip(offsets.chunks_exact(2).take(cols)) {
        let offset = u16::from_le_bytes([pair[0], pair[1]]);
        if offset == table::NO_CELL_OFFSET {
            continue;
        }
        let rendered = match parse_cell(buf, usize::from(offset)) {
            Some(p) => Some(ctx.render(&p, text_budget)?),
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

fn fill_tiles(
    ctx: &mut CellContext,
    tiles: &Message,
    grid: &mut [Vec<String>],
    cols: usize,
    text_budget: &mut DecodeBudget,
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
                unsupported += fill_row(ctx, &row, cols, cells, text_budget)?;
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
        Err(outcome) => return Ok(outcome),
    };
    budget.charge_cells(rows.checked_mul(cols).ok_or_else(|| {
        EasyOfficeError::ResourceBudgetExceeded("Pages 표 셀 수 계산 오버플로".to_string())
    })?)?;
    let Some(store) = model_msg.sub(table::MODEL_DATA_STORE)? else {
        return Ok(TableOutcome::Skipped("읽을 수 없는 표"));
    };
    let mut ctx = CellContext::new(objects, &store, budget)?;
    let mut grid = vec![vec![String::new(); cols]; rows];
    let unsupported = match store.sub(table::STORE_TILES)? {
        Some(tiles) => fill_tiles(&mut ctx, &tiles, &mut grid, cols, text_budget)?,
        None => 0,
    };
    let merges = merge::read_merges(objects, &model_msg, &store);
    Ok(TableOutcome::Read(TableRead {
        grid,
        merges,
        unsupported,
        cell_objects: ctx.cell_objects,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages_schema::storage;

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
                payload,
                extra: Vec::new(),
            },
        );
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

        // 1 row x 10 cols = 160 KiB fits comfortably inside 1 MiB budget.
        make_table(&mut objects, 1);
        let info_ok = objects.get(15).unwrap().clone();
        let mut tbudget = TableBudget::default();
        let mut dbudget = DecodeBudget::with_limit(1024 * 1024);
        let TableOutcome::Read(read) =
            read_table(&objects, &info_ok, &mut tbudget, &mut dbudget).unwrap()
        else {
            panic!("expected TableOutcome::Read");
        };
        assert_eq!(read.grid.len(), 1);
        assert_eq!(read.grid[0].len(), cols);
        assert!(read.grid[0].iter().all(|c| c.len() == 16 * 1024));

        // 80 rows x 10 cols = 800 * 16 KiB = 12.8 MiB trips 1 MiB budget fail-closed.
        make_table(&mut objects, 80);
        let info_over = objects.get(15).unwrap().clone();
        let mut tbudget2 = TableBudget::default();
        let mut dbudget2 = DecodeBudget::with_limit(1024 * 1024);
        assert!(matches!(
            read_table(&objects, &info_over, &mut tbudget2, &mut dbudget2),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}

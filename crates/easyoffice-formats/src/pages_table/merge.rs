//! Merged regions of a Pages table, read from either of the two places files keep them.
//!
//! 1. The data store's merge region map (field 13, confirmed against real documents): repeated
//!    entries `{ origin = 1, size = 2 }`, both varints that pack `row << 16 | column` (the size
//!    packs `rows << 16 | columns`). Written by Pages 14.x.
//! 2. The merge owner's formula store (model field 47): one formula per merged range whose first
//!    node is a colon tract (absolute column and row ranges) or a single zigzag-encoded cell
//!    reference. 검증 필요: this form is described by the public `iwork` crate (MIT License,
//!    Copyright (c) 2026 Zebra Pig, `src/table.rs` `merges_from_owner`) for files written by
//!    newer app versions; no fixture of ours carries it. Reference implementation ported against
//!    this crate's message API, with budgets added.
//!
//! Counts are bounded here; whether the regions fit the grid and do not overlap is decided by
//! the rich table.

use super::{reference, MAX_TABLE_CELLS};
use crate::pages_proto::{Message, ObjectTable};
use crate::pages_schema::table;
use crate::CellMerge;

const PACKED_LOW_MASK: u64 = 0xFFFF;
const PACKED_SHIFT: u32 = 16;
const PACKED_MAX: u64 = u32::MAX as u64;
/// Formula AST node kinds of the owner's store.
const NODE_CELL_REFERENCE: u64 = 36;
const NODE_COLON_TRACT: u64 = 67;
/// A region covers at least two cells, so more regions than half the cells cannot be valid.
const MAX_REGIONS: usize = MAX_TABLE_CELLS / 2;

/// `(high half, low half)` of a packed value, `None` when it does not fit 32 bits.
fn unpack(packed: u64) -> Option<(u32, u32)> {
    (packed <= PACKED_MAX).then_some((
        (packed >> PACKED_SHIFT) as u32,
        (packed & PACKED_LOW_MASK) as u32,
    ))
}

/// Regions of the data store's merge region map.
fn from_region_map(objects: &ObjectTable, store: &Message) -> Vec<CellMerge> {
    let Some(map) = reference(store, table::STORE_MERGE_REGION_MAP)
        .and_then(|id| objects.get(id))
        .and_then(|o| Message::parse(&o.payload).ok())
    else {
        return Vec::new();
    };
    let Ok(ranges) = map.subs(table::MERGE_RANGES) else {
        return Vec::new();
    };
    ranges
        .iter()
        .take(MAX_REGIONS)
        .filter_map(|range| {
            let (row, col) = unpack(range.varint(table::RANGE_ORIGIN)?)?;
            let (row_span, col_span) = unpack(range.varint(table::RANGE_SIZE)?)?;
            Some(CellMerge {
                row,
                col,
                row_span,
                col_span,
            })
        })
        .collect()
}

/// `(begin, end)` of one axis of a colon tract; an absent end means the same as the begin.
fn axis(tract: &Message, field: u32) -> Option<(u64, u64)> {
    let range = tract.sub(field).ok().flatten()?;
    let begin = range.varint(table::TRACT_BEGIN)?;
    let end = range.varint(table::TRACT_END).unwrap_or(begin);
    (end >= begin).then_some((begin, end))
}

fn unzigzag(value: u64) -> u64 {
    (value >> 1) ^ 0u64.wrapping_sub(value & 1)
}

/// Region named by the first recognised node of one formula.
fn region_of_formula(pair: &Message) -> Option<CellMerge> {
    let nodes = pair
        .sub(table::OWNER_PAIR_FORMULA)
        .ok()
        .flatten()?
        .sub(table::FORMULA_NODES)
        .ok()
        .flatten()?
        .subs(table::NODE_LIST)
        .ok()?;
    nodes
        .iter()
        .find_map(|node| match node.varint(table::NODE_KIND)? {
            NODE_COLON_TRACT => {
                let tract = node.sub(table::NODE_COLON_TRACT).ok().flatten()?;
                let (cols, rows) = (
                    axis(&tract, table::TRACT_COLUMNS)?,
                    axis(&tract, table::TRACT_ROWS)?,
                );
                Some(CellMerge {
                    row: u32::try_from(rows.0).ok()?,
                    col: u32::try_from(cols.0).ok()?,
                    row_span: u32::try_from((rows.1 - rows.0).checked_add(1)?).ok()?,
                    col_span: u32::try_from((cols.1 - cols.0).checked_add(1)?).ok()?,
                })
            }
            NODE_CELL_REFERENCE => {
                let coordinate = |field: u32| {
                    let value = node.sub(field).ok().flatten()?.varint(1)?;
                    u32::try_from(unzigzag(value)).ok()
                };
                Some(CellMerge {
                    row: coordinate(table::NODE_ROW)?,
                    col: coordinate(table::NODE_COLUMN)?,
                    row_span: 1,
                    col_span: 1,
                })
            }
            _ => None,
        })
}

/// Regions of the merge owner's formula store.
fn from_owner_formulas(model: &Message) -> Vec<CellMerge> {
    let Some(store) = model
        .sub(table::MODEL_MERGE_OWNER)
        .ok()
        .flatten()
        .and_then(|owner| owner.sub(table::OWNER_FORMULA_STORE).ok().flatten())
    else {
        return Vec::new();
    };
    let Ok(pairs) = store.subs(table::OWNER_STORE_PAIRS) else {
        return Vec::new();
    };
    pairs
        .iter()
        .take(MAX_REGIONS)
        .filter_map(region_of_formula)
        .collect()
}

/// Merge regions of a table: the region map first, the owner's formula store when it is empty.
pub(super) fn read_merges(
    objects: &ObjectTable,
    model: &Message,
    store: &Message,
) -> Vec<CellMerge> {
    let merges = from_region_map(objects, store);
    if merges.is_empty() {
        from_owner_formulas(model)
    } else {
        merges
    }
}

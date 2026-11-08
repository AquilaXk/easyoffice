//! Merged regions of a Pages table: the data store's merge region map lists cell ranges, each
//! with an origin and a size packed as `column << 16 | row`. Counts are bounded; whether the
//! regions fit the grid and do not overlap is decided by the rich table.

use super::{reference, MAX_TABLE_CELLS};
use crate::pages_proto::{Message, ObjectTable};
use crate::pages_schema::table;
use crate::CellMerge;

const PACKED_LOW_MASK: u32 = 0xFFFF;
const PACKED_SHIFT: u32 = 16;

/// `(column, row)` of a packed coordinate or size.
fn unpack(range: &Message, field: u32) -> Option<(u32, u32)> {
    let packed = range
        .sub(field)
        .ok()
        .flatten()?
        .fixed32(table::RANGE_PACKED)?;
    Some((packed >> PACKED_SHIFT, packed & PACKED_LOW_MASK))
}

/// Merge regions of the table whose data store is `store`; empty when it has no merge map.
pub(super) fn read_merges(objects: &ObjectTable, store: &Message) -> Vec<CellMerge> {
    let Some(map) = reference(store, table::STORE_MERGE_REGION_MAP)
        .and_then(|id| objects.get(id))
        .and_then(|o| Message::parse(&o.payload).ok())
    else {
        return Vec::new();
    };
    let Ok(ranges) = map.subs(table::MERGE_RANGES) else {
        return Vec::new();
    };
    // A region covers at least two cells, so more regions than half the cells cannot be valid.
    ranges
        .iter()
        .take(MAX_TABLE_CELLS / 2)
        .filter_map(|range| {
            let (col, row) = unpack(range, table::RANGE_ORIGIN)?;
            let (col_span, row_span) = unpack(range, table::RANGE_SIZE)?;
            Some(CellMerge {
                row,
                col,
                row_span,
                col_span,
            })
        })
        .collect()
}

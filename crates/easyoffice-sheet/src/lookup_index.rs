//! Exact-match sparse range traversal and lazy O(1) lookup indexing (`VLOOKUP`, `HLOOKUP`,
//! `MATCH`, `XLOOKUP`, `XMATCH`), plus shared lookup comparison helpers (Q1).

use crate::{EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine};
use std::collections::hash_map::Entry;
use std::collections::HashMap;

pub(crate) type CellResult = Result<EvaluatedCellValue, FormulaError>;

const MAX_CACHED_SLICES: usize = 64;
const MAX_CACHED_ENTRIES: usize = 500_000;

/// Canonical equality key matching [`SheetEngine::values_match`] semantics:
/// `Empty` and `""` share `Blank`; `-0.0` normalizes to `0.0`; `NaN` and `Error(_)` never match.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LookupKey {
    Number(u64),
    Boolean(bool),
    Text(String),
    Blank,
}

impl LookupKey {
    pub(crate) fn from_value(v: &EvaluatedCellValue) -> Option<Self> {
        match v {
            EvaluatedCellValue::Number(n) => {
                if n.is_nan() {
                    None
                } else {
                    let norm = if *n == 0.0 { 0.0 } else { *n };
                    Some(Self::Number(norm.to_bits()))
                }
            }
            EvaluatedCellValue::Boolean(b) => Some(Self::Boolean(*b)),
            EvaluatedCellValue::Text(s) => {
                if s.is_empty() {
                    Some(Self::Blank)
                } else if s.is_ascii() {
                    Some(Self::Text(s.to_ascii_lowercase()))
                } else {
                    Some(Self::Text(s.chars().flat_map(char::to_lowercase).collect()))
                }
            }
            EvaluatedCellValue::Empty => Some(Self::Blank),
            EvaluatedCellValue::Error(_) => None,
        }
    }
}

/// Allocation-free equality check against a pre-canonicalized [`LookupKey`].
fn matches_lookup_key(v: &EvaluatedCellValue, key: &LookupKey) -> bool {
    match (v, key) {
        (EvaluatedCellValue::Number(n), LookupKey::Number(bits)) => *n == f64::from_bits(*bits),
        (EvaluatedCellValue::Boolean(a), LookupKey::Boolean(b)) => a == b,
        (EvaluatedCellValue::Empty, LookupKey::Blank) => true,
        (EvaluatedCellValue::Text(s), LookupKey::Blank) => s.is_empty(),
        (EvaluatedCellValue::Text(s), LookupKey::Text(lower)) => {
            if s.is_ascii() && lower.is_ascii() {
                s.eq_ignore_ascii_case(lower)
            } else {
                s.chars().flat_map(char::to_lowercase).eq(lower.chars())
            }
        }
        _ => false,
    }
}

#[inline]
fn has_wildcard_chars(s: &str) -> bool {
    s.bytes().any(|b| matches!(b, b'*' | b'?' | b'~'))
}

#[inline]
fn needs_wildcard_scan(wildcard_mode: bool, lookup: &EvaluatedCellValue) -> bool {
    wildcard_mode
        && matches!(
            lookup,
            EvaluatedCellValue::Text(p) if has_wildcard_chars(p) || !p.is_ascii()
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SliceAxis {
    Col,
    Row,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SliceKey {
    /// Identity of the target `SheetEngine` for this immutable evaluation pass.
    pub(crate) sheet_ptr: usize,
    pub(crate) axis: SliceAxis,
    pub(crate) fixed: u32,
    pub(crate) start: u32,
    pub(crate) end: u32,
}

impl SliceKey {
    #[inline]
    pub(crate) fn for_col(sheet: &SheetEngine, col: u32, min_r: u32, max_r: u32) -> Self {
        Self {
            sheet_ptr: sheet as *const SheetEngine as usize,
            axis: SliceAxis::Col,
            fixed: col,
            start: min_r,
            end: max_r,
        }
    }

    #[inline]
    pub(crate) fn for_row(sheet: &SheetEngine, row: u32, min_c: u32, max_c: u32) -> Self {
        Self {
            sheet_ptr: sheet as *const SheetEngine as usize,
            axis: SliceAxis::Row,
            fixed: row,
            start: min_c,
            end: max_c,
        }
    }
}

#[derive(Debug, Default)]
struct LookupColIndex {
    /// `(first_offset, last_offset)` across populated cells in the slice (`0..=(end - start)`).
    by_key: HashMap<LookupKey, (u32, u32)>,
    first_unpop_blank: Option<u32>,
    last_unpop_blank: Option<u32>,
}

impl LookupColIndex {
    fn lookup(&self, key: &LookupKey, reverse: bool) -> Option<u32> {
        let pop_hit = self
            .by_key
            .get(key)
            .map(|&(first, last)| if reverse { last } else { first });
        if !matches!(key, LookupKey::Blank) {
            return pop_hit;
        }
        if reverse {
            match (pop_hit, self.last_unpop_blank) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            }
        } else {
            match (pop_hit, self.first_unpop_blank) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        }
    }
}

#[derive(Debug)]
enum SliceSlot {
    SeenOnce,
    Indexed(LookupColIndex),
}

/// Per-`EvalContext` lazy exact-lookup index cache (bounded fail-closed).
#[derive(Debug, Default)]
pub(crate) struct LookupCache {
    slices: HashMap<SliceKey, SliceSlot>,
    total_entries: usize,
}

enum SlicePopulated<'a> {
    Col { fixed_col: u32, rows: &'a [u32] },
    Row { coords: &'a [PackedCellCoordinate] },
}

impl<'a> SlicePopulated<'a> {
    fn of(sheet: &'a SheetEngine, slice: SliceKey) -> Self {
        let idx = sheet.cell_index();
        match slice.axis {
            SliceAxis::Col => Self::Col {
                fixed_col: slice.fixed,
                rows: idx.col_rows_slice(slice.fixed, slice.start, slice.end),
            },
            SliceAxis::Row => Self::Row {
                coords: idx.row_coords_slice(slice.fixed, slice.start, slice.end),
            },
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match self {
            Self::Col { rows, .. } => rows.len(),
            Self::Row { coords } => coords.len(),
        }
    }

    #[inline]
    fn pos_at(&self, i: usize) -> u32 {
        match self {
            Self::Col { rows, .. } => rows[i],
            Self::Row { coords } => coords[i].col(),
        }
    }

    #[inline]
    fn coord_at(&self, i: usize) -> Result<PackedCellCoordinate, FormulaError> {
        match self {
            Self::Col { fixed_col, rows } => packed(rows[i], *fixed_col),
            Self::Row { coords } => Ok(coords[i]),
        }
    }

    fn first_blank_gap(&self, start: u32, end: u32) -> Option<u32> {
        if start > end {
            return None;
        }
        let span = end.saturating_sub(start).saturating_add(1) as usize;
        if self.len() >= span {
            return None;
        }
        let mut lo = 0usize;
        let mut hi = self.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.pos_at(mid) == start + mid as u32 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Some(lo as u32)
    }

    fn last_blank_gap(&self, start: u32, end: u32) -> Option<u32> {
        if start > end {
            return None;
        }
        let n = self.len();
        let span = end.saturating_sub(start).saturating_add(1) as usize;
        if n >= span {
            return None;
        }
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let packed_pos = end - (n - 1 - mid) as u32;
            if self.pos_at(mid) == packed_pos {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let last_pos = end - (n - lo) as u32;
        Some(last_pos - start)
    }
}

#[inline]
fn charge_lookup_step(ctx: &mut crate::EvalContext<'_>) -> Result<(), FormulaError> {
    ctx.steps = ctx.steps.saturating_add(1);
    if ctx.steps & crate::deadline::POLL_STEPS_MASK == 0 {
        ctx.poll_deadline();
    }
    if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

fn sparse_wildcard_scan(
    target_sheet: &SheetEngine,
    slice: SliceKey,
    pop: &SlicePopulated<'_>,
    lookup_val: &EvaluatedCellValue,
    reverse: bool,
    ctx: &mut crate::EvalContext<'_>,
    depth: usize,
) -> Result<Option<u32>, FormulaError> {
    let n = pop.len();
    for step in 0..n {
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
            return Err(FormulaError::Value);
        }
        let i = if reverse { n - 1 - step } else { step };
        let coord = pop.coord_at(i)?;
        let matched = target_sheet.with_coord_value(coord, ctx, depth, |kv| {
            SheetEngine::values_match_wildcard(kv, lookup_val)
        });
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
            return Err(FormulaError::Value);
        }
        if matched {
            return Ok(Some(pop.pos_at(i) - slice.start));
        }
    }
    Ok(None)
}

fn sparse_exact_scan(
    target_sheet: &SheetEngine,
    slice: SliceKey,
    pop: &SlicePopulated<'_>,
    query_key: &LookupKey,
    reverse: bool,
    ctx: &mut crate::EvalContext<'_>,
    depth: usize,
) -> Result<Option<u32>, FormulaError> {
    let n = pop.len();
    let blank_gap = if matches!(query_key, LookupKey::Blank) {
        if reverse {
            pop.last_blank_gap(slice.start, slice.end)
        } else {
            pop.first_blank_gap(slice.start, slice.end)
        }
    } else {
        None
    };
    if !reverse && blank_gap == Some(0) {
        return Ok(Some(0));
    }
    for step in 0..n {
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
            return Err(FormulaError::Value);
        }
        let i = if reverse { n - 1 - step } else { step };
        let off = pop.pos_at(i) - slice.start;
        if let Some(g) = blank_gap {
            if (!reverse && off > g) || (reverse && off < g) {
                return Ok(Some(g));
            }
        }
        let coord = pop.coord_at(i)?;
        let matched = target_sheet
            .with_coord_value(coord, ctx, depth, |kv| matches_lookup_key(kv, query_key));
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
            return Err(FormulaError::Value);
        }
        if matched {
            return Ok(Some(off));
        }
    }
    Ok(blank_gap)
}

fn try_build_col_index(
    target_sheet: &SheetEngine,
    slice: SliceKey,
    pop: &SlicePopulated<'_>,
    ctx: &mut crate::EvalContext<'_>,
    depth: usize,
) -> Result<Option<LookupColIndex>, FormulaError> {
    let n = pop.len();
    if ctx.lookup_cache.total_entries.saturating_add(n) > MAX_CACHED_ENTRIES {
        return Ok(None);
    }
    let sid = target_sheet.resolved_sheet_id();
    let mut by_key: HashMap<LookupKey, (u32, u32)> = HashMap::with_capacity(n.min(4096));
    for i in 0..n {
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
            return Err(FormulaError::Value);
        }
        let coord = pop.coord_at(i)?;
        if ctx.memo.is_visiting(sid, coord) {
            return Ok(None);
        }
        let key_opt = target_sheet.with_coord_value(coord, ctx, depth, LookupKey::from_value);
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS || ctx.expired() {
            return Err(FormulaError::Value);
        }
        if let Some(key) = key_opt {
            let off = pop.pos_at(i) - slice.start;
            by_key
                .entry(key)
                .and_modify(|e| e.1 = off)
                .or_insert((off, off));
        }
    }
    Ok(Some(LookupColIndex {
        by_key,
        first_unpop_blank: pop.first_blank_gap(slice.start, slice.end),
        last_unpop_blank: pop.last_blank_gap(slice.start, slice.end),
    }))
}

impl SheetEngine {
    /// Exact match over a single contiguous 1D slice (`Col` or `Row`), returning `Ok(None)`
    /// when the range resides on a virtual array sheet so the caller falls back to dense scan.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn exact_lookup_slice(
        &self,
        sheet_opt: Option<&str>,
        axis: SliceAxis,
        fixed: u32,
        start: u32,
        end: u32,
        lookup_val: &EvaluatedCellValue,
        wildcard_mode: bool,
        reverse: bool,
        ctx: &mut crate::EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<Option<u32>>, FormulaError> {
        let target_sheet = match sheet_opt {
            None => self,
            Some(name) if crate::WorkbookEngine::is_same_sheet_identity(name, &self.name) => self,
            Some(name) => match ctx.workbook.and_then(|wb| wb.sheet(name)) {
                Some(sheet) if crate::array::virtual_sheet_index(name).is_none() => sheet,
                _ => return Ok(None),
            },
        };
        let slice = match axis {
            SliceAxis::Col => SliceKey::for_col(target_sheet, fixed, start, end),
            SliceAxis::Row => SliceKey::for_row(target_sheet, fixed, start, end),
        };
        charge_lookup_step(ctx)?;
        if slice.start > slice.end {
            return Ok(Some(None));
        }
        let pop = SlicePopulated::of(target_sheet, slice);
        if needs_wildcard_scan(wildcard_mode, lookup_val) {
            return sparse_wildcard_scan(
                target_sheet,
                slice,
                &pop,
                lookup_val,
                reverse,
                ctx,
                depth,
            )
            .map(Some);
        }
        let Some(query_key) = LookupKey::from_value(lookup_val) else {
            return Ok(Some(None));
        };
        if pop.len() == 0 {
            return Ok(Some(
                matches!(query_key, LookupKey::Blank).then_some(if reverse {
                    slice.end - slice.start
                } else {
                    0
                }),
            ));
        }
        if let Some(SliceSlot::Indexed(col_idx)) = ctx.lookup_cache.slices.get(&slice) {
            return Ok(Some(col_idx.lookup(&query_key, reverse)));
        }
        let build_now = matches!(
            ctx.lookup_cache.slices.get(&slice),
            Some(SliceSlot::SeenOnce)
        );
        if !build_now {
            if ctx.lookup_cache.slices.len() < MAX_CACHED_SLICES {
                if let Entry::Vacant(e) = ctx.lookup_cache.slices.entry(slice) {
                    e.insert(SliceSlot::SeenOnce);
                }
            }
            return sparse_exact_scan(target_sheet, slice, &pop, &query_key, reverse, ctx, depth)
                .map(Some);
        }
        if let Some(col_idx) = try_build_col_index(target_sheet, slice, &pop, ctx, depth)? {
            let hit = col_idx.lookup(&query_key, reverse);
            ctx.lookup_cache.total_entries += col_idx.by_key.len();
            ctx.lookup_cache
                .slices
                .insert(slice, SliceSlot::Indexed(col_idx));
            Ok(Some(hit))
        } else {
            sparse_exact_scan(target_sheet, slice, &pop, &query_key, reverse, ctx, depth).map(Some)
        }
    }
}

/// Lookup values only compete for approximate matches when they share a type.
pub(crate) fn same_value_type(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
    matches!(
        (a, b),
        (EvaluatedCellValue::Number(_), EvaluatedCellValue::Number(_))
            | (EvaluatedCellValue::Text(_), EvaluatedCellValue::Text(_))
            | (
                EvaluatedCellValue::Boolean(_),
                EvaluatedCellValue::Boolean(_)
            )
    )
}

pub(crate) fn check_lookup_arity(
    args: &[Expr],
    min: usize,
    max: usize,
) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

pub(crate) fn number_value(v: f64) -> CellResult {
    Ok(EvaluatedCellValue::Number(v))
}

pub(crate) fn packed(row: u32, col: u32) -> Result<PackedCellCoordinate, FormulaError> {
    PackedCellCoordinate::new(row, col).map_err(|_| FormulaError::Ref)
}

/// Position of a VLOOKUP / HLOOKUP table: key line and result line offsets.
pub(crate) struct TableGeometry {
    pub(crate) min_r: u32,
    pub(crate) min_c: u32,
    pub(crate) is_vertical: bool,
    pub(crate) target_idx: u32,
}

impl TableGeometry {
    /// Key cell and result cell coordinates for the `off`-th row (VLOOKUP) or column (HLOOKUP).
    pub(crate) fn coord_pair(
        &self,
        off: u32,
    ) -> Result<(PackedCellCoordinate, PackedCellCoordinate), FormulaError> {
        if self.is_vertical {
            Ok((
                packed(self.min_r + off, self.min_c)?,
                packed(self.min_r + off, self.min_c + self.target_idx)?,
            ))
        } else {
            Ok((
                packed(self.min_r, self.min_c + off)?,
                packed(self.min_r + self.target_idx, self.min_c + off)?,
            ))
        }
    }
}

/// Keeps the greatest key that is `<=` the lookup value (sorted approximate match).
pub(crate) fn update_best_floor(
    best: &mut Option<(u32, EvaluatedCellValue)>,
    off: u32,
    key: EvaluatedCellValue,
    lookup: &EvaluatedCellValue,
) {
    let Some(ord) = SheetEngine::compare_excel_values(&key, lookup) else {
        return;
    };
    if !ord.is_le() {
        return;
    }
    let replace = best.as_ref().is_none_or(|(_, prev)| {
        SheetEngine::compare_excel_values(&key, prev).is_some_and(std::cmp::Ordering::is_ge)
    });
    if replace {
        *best = Some((off, key));
    }
}

/// Attempts an $O(\log N)$ binary search over `0..len` for approximate ascending (`ascending == true`,
/// largest `key <= lookup`) or descending (`ascending == false`, smallest `key >= lookup`) lookup.
///
/// Falls back (`Ok(Err(()))`) to linear scan if any probed element has a different type (including
/// empty or error cells) or violates monotonicity.
pub(crate) fn sorted_approx_binary_search(
    len: usize,
    ascending: bool,
    lookup: &EvaluatedCellValue,
    mut eval_at: impl FnMut(usize) -> Result<EvaluatedCellValue, FormulaError>,
) -> Result<Result<Option<usize>, ()>, FormulaError> {
    if len == 0 {
        return Ok(Ok(None));
    }
    let mut lo = 0usize;
    let mut hi = len;
    let mut best: Option<usize> = None;
    let mut lower_bound_val: Option<EvaluatedCellValue> = None;
    let mut upper_bound_val: Option<EvaluatedCellValue> = None;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let k = eval_at(mid)?;
        if !same_value_type(&k, lookup) {
            return Ok(Err(()));
        }
        let Some(ord) = SheetEngine::compare_excel_values(&k, lookup) else {
            return Ok(Err(()));
        };
        if let Some(ref lb) = lower_bound_val {
            let Some(lb_ord) = SheetEngine::compare_excel_values(&k, lb) else {
                return Ok(Err(()));
            };
            if (ascending && lb_ord.is_lt()) || (!ascending && lb_ord.is_gt()) {
                return Ok(Err(()));
            }
        }
        if let Some(ref ub) = upper_bound_val {
            let Some(ub_ord) = SheetEngine::compare_excel_values(&k, ub) else {
                return Ok(Err(()));
            };
            if (ascending && ub_ord.is_gt()) || (!ascending && ub_ord.is_lt()) {
                return Ok(Err(()));
            }
        }
        if (ascending && ord.is_le()) || (!ascending && ord.is_ge()) {
            best = Some(mid);
            lower_bound_val = Some(k);
            lo = mid + 1;
        } else {
            upper_bound_val = Some(k);
            hi = mid;
        }
    }
    Ok(Ok(best))
}

/// XLOOKUP match / search mode pair.
pub(crate) struct XlookupModes {
    pub(crate) match_mode: i32,
    pub(crate) search_mode: i32,
}

impl XlookupModes {
    pub(crate) fn cell_matches(
        &self,
        key: &EvaluatedCellValue,
        lookup: &EvaluatedCellValue,
    ) -> bool {
        if self.match_mode == 2 {
            SheetEngine::values_match_wildcard(key, lookup)
        } else {
            SheetEngine::values_match(key, lookup)
        }
    }

    /// Tracks the nearest smaller (-1) / larger (1) candidate for approximate matches.
    pub(crate) fn update_approx(
        &self,
        best: &mut Option<(usize, EvaluatedCellValue)>,
        idx: usize,
        key: &EvaluatedCellValue,
        lookup: &EvaluatedCellValue,
    ) {
        let mode = self.match_mode;
        if !(mode == -1 || mode == 1) || !same_value_type(key, lookup) {
            return;
        }
        let Some(ord) = SheetEngine::compare_excel_values(key, lookup) else {
            return;
        };
        let valid = (mode == -1 && ord.is_lt()) || (mode == 1 && ord.is_gt());
        if !valid {
            return;
        }
        let replace = best.as_ref().is_none_or(|(_, prev)| {
            SheetEngine::compare_excel_values(key, prev).is_some_and(|o| {
                if mode == -1 {
                    o.is_gt()
                } else {
                    o.is_lt()
                }
            })
        });
        if replace {
            *best = Some((idx, key.clone()));
        }
    }

    #[inline]
    pub(crate) fn scan_index(&self, step: usize, total_len: usize) -> usize {
        if self.search_mode < 0 {
            total_len - 1 - step
        } else {
            step
        }
    }

    /// $O(\log N)$ binary search for `search_mode == 2` (ascending) or `-2` (descending).
    pub(crate) fn binary_search(
        &self,
        total_len: usize,
        lookup: &EvaluatedCellValue,
        mut eval_at: impl FnMut(usize) -> Result<EvaluatedCellValue, FormulaError>,
    ) -> Result<Option<usize>, FormulaError> {
        if self.match_mode == 2 {
            return Err(FormulaError::Value);
        }
        let ascending = self.search_mode == 2;
        let mut lo = 0usize;
        let mut hi = total_len;
        let mut approx_hit: Option<usize> = None;
        let mut first_exact: Option<usize> = None;
        while lo < hi {
            // An error cell has no place in the order: probe to its right, then its left, for the
            // nearest cell that can be compared.
            let mid = lo + (hi - lo) / 2;
            let mut probe = mid;
            let mut key = eval_at(probe)?;
            if matches!(key, EvaluatedCellValue::Error(_)) {
                let right = (mid + 1..hi).chain((lo..mid).rev());
                let Some((idx, value)) = Self::first_comparable(right, &mut eval_at)? else {
                    break;
                };
                probe = idx;
                key = value;
            }
            let Some(ord) = SheetEngine::compare_excel_values(&key, lookup) else {
                break;
            };
            if ord.is_eq() && same_value_type(&key, lookup) {
                // Duplicates: keep narrowing left so the first one in array order wins.
                first_exact = Some(probe);
                hi = probe;
                continue;
            }
            let same_type = same_value_type(&key, lookup);
            if ord.is_lt() {
                if self.match_mode == -1 && same_type {
                    approx_hit = Some(probe);
                }
                if ascending {
                    lo = probe + 1;
                } else {
                    hi = probe;
                }
            } else {
                if self.match_mode == 1 && same_type {
                    approx_hit = Some(probe);
                }
                if ascending {
                    hi = probe;
                } else {
                    lo = probe + 1;
                }
            }
        }
        Ok(first_exact.or(approx_hit))
    }

    /// The first index of `indices` whose cell is not an error, with its value.
    fn first_comparable(
        indices: impl Iterator<Item = usize>,
        eval_at: &mut impl FnMut(usize) -> Result<EvaluatedCellValue, FormulaError>,
    ) -> Result<Option<(usize, EvaluatedCellValue)>, FormulaError> {
        for idx in indices {
            let value = eval_at(idx)?;
            if !matches!(value, EvaluatedCellValue::Error(_)) {
                return Ok(Some((idx, value)));
            }
        }
        Ok(None)
    }
}

/// MATCH candidate tracking for approximate (1 / -1) modes.
pub(crate) struct MatchState {
    pub(crate) match_type: i32,
    pub(crate) best: Option<(usize, EvaluatedCellValue)>,
}

impl MatchState {
    /// Returns true on an exact (wildcard) hit for `match_type == 0`.
    pub(crate) fn consider(
        &mut self,
        idx: usize,
        v: &EvaluatedCellValue,
        lookup: &EvaluatedCellValue,
    ) -> bool {
        if self.match_type == 0 {
            return SheetEngine::values_match_wildcard(v, lookup);
        }
        if !same_value_type(v, lookup) {
            return false;
        }
        let Some(ord) = SheetEngine::compare_excel_values(v, lookup) else {
            return false;
        };
        let better = if self.match_type > 0 && ord.is_le() {
            self.beats_best(v, std::cmp::Ordering::is_ge)
        } else if self.match_type < 0 && ord.is_ge() {
            self.beats_best(v, std::cmp::Ordering::is_le)
        } else {
            false
        };
        if better {
            self.best = Some((idx, v.clone()));
        }
        false
    }

    fn beats_best(&self, v: &EvaluatedCellValue, accept: fn(std::cmp::Ordering) -> bool) -> bool {
        self.best
            .as_ref()
            .is_none_or(|(_, prev)| SheetEngine::compare_excel_values(v, prev).is_some_and(accept))
    }

    pub(crate) fn result(&self) -> CellResult {
        match &self.best {
            Some((best_idx, _)) => number_value(*best_idx as f64),
            None => Err(FormulaError::Na),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{EvaluatedCellValue, SheetEngine, WorkbookEngine};
    use std::time::Instant;

    #[test]
    fn sparse_whole_column_exact_lookups_avoid_step_exhaustion() {
        let mut s = SheetEngine::new("Sheet1");
        s.set_cell("A1000000", "target").unwrap();
        s.set_cell("B1000000", "42").unwrap();
        assert_eq!(
            s.evaluate_formula_str(
                "=VLOOKUP(\"target\", A1:B1048576, 2, FALSE) + VLOOKUP(\"target\", A1:B1048576, 2, FALSE) + VLOOKUP(\"target\", A1:B1048576, 2, FALSE)"
            ),
            EvaluatedCellValue::Number(126.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=MATCH(\"target\", A1:A1048576, 0)"),
            EvaluatedCellValue::Number(1_000_000.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(\"target\", A1:A1048576, 0, 1)"),
            EvaluatedCellValue::Number(1_000_000.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(\"target\", A1:A1048576, B1:B1048576)"),
            EvaluatedCellValue::Number(42.0)
        );
    }

    #[test]
    #[ignore = "measurement harness: run with --release -- --ignored --nocapture"]
    fn bench_q1_lookup_release() {
        let mut wb = WorkbookEngine::new();
        let _ = wb.add_sheet("Sheet1");
        let rows = 5_000u32;
        for r in 1..=rows {
            wb.set_cell("Sheet1", &format!("A{r}"), format!("KEY_{r:05}"))
                .unwrap();
            wb.set_cell("Sheet1", &format!("B{r}"), (r * 10).to_string())
                .unwrap();
        }
        let queries = 2_000u32;
        for q in 1..=queries {
            let target = ((q * 97) % rows) + 1;
            wb.set_cell("Sheet1", &format!("C{q}"), format!("KEY_{target:05}"))
                .unwrap();
            wb.set_cell(
                "Sheet1",
                &format!("D{q}"),
                format!("=VLOOKUP(C{q}, $A$1:$B${rows}, 2, FALSE)"),
            )
            .unwrap();
            wb.set_cell(
                "Sheet1",
                &format!("E{q}"),
                format!("=XLOOKUP(C{q}, $A$1:$A${rows}, $B$1:$B${rows})"),
            )
            .unwrap();
            wb.set_cell(
                "Sheet1",
                &format!("F{q}"),
                format!("=MATCH(C{q}, $A$1:$A${rows}, 0)"),
            )
            .unwrap();
            wb.set_cell(
                "Sheet1",
                &format!("G{q}"),
                format!("=XMATCH(C{q}, $A$1:$A${rows}, 0, -1)"),
            )
            .unwrap();
        }
        let started = Instant::now();
        let all = wb.sheet("Sheet1").unwrap().recalculate_all();
        let elapsed = started.elapsed();
        assert_eq!(
            all.get(&crate::PackedCellCoordinate::parse_a1("D1").unwrap()),
            Some(&EvaluatedCellValue::Number(980.0))
        );
        eprintln!(
            "bench_q1_lookup_release: {queries}x4 exact lookups over {rows} rows took {:.3} ms",
            elapsed.as_secs_f64() * 1000.0
        );
    }

    #[test]
    fn exact_lookup_parity_edge_cases() {
        let mut s = SheetEngine::new("Sheet1");
        // Mix numbers (-0.0, 0.0, NaN), booleans, ASCII/Unicode text, "", blanks, and errors.
        s.set_cell("A1", "=1/0").unwrap();
        s.set_cell("A2", "=\"\"").unwrap();
        // A3 is intentionally blank (unpopulated)
        s.set_cell("A4", "0").unwrap();
        s.set_cell("A5", "TRUE").unwrap();
        s.set_cell("A6", "Café").unwrap();
        s.set_cell("A7", "=\"\"").unwrap();
        s.set_cell("A8", "café").unwrap();
        for r in 1..=8u32 {
            s.set_cell(&format!("B{r}"), (r * 10).to_string()).unwrap();
        }
        // Negative zero matches +0, TRUE does not match 1, errors in slice do not abort,
        // first vs reverse match agree on both 1st (sparse) and 2nd+ (indexed) calls.
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(-0, A1:A8, B1:B8) + XLOOKUP(0, A1:A8, B1:B8)"),
            EvaluatedCellValue::Number(80.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(1, A1:A8, 0)"),
            EvaluatedCellValue::Error(crate::FormulaError::Na)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(TRUE, A1:A8, 0)"),
            EvaluatedCellValue::Number(5.0)
        );
        // Blank / "" match: A2 is populated "", A3 is unpopulated Empty, A7 is populated "".
        assert_eq!(
            s.evaluate_formula_str("=MATCH(\"\", A1:A8, 0)"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(\"\", A1:A8, 0, 1)"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(\"\", A1:A8, 0, -1)"),
            EvaluatedCellValue::Number(7.0)
        );
        // If A3 is after A2, unpopulated A9 in A1:A9 becomes last blank on reverse search.
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(\"\", A1:A9, 0, -1)"),
            EvaluatedCellValue::Number(9.0)
        );
        // Unpopulated A1 in A1:A8 matches first when A1:A1 is empty.
        s.set_cell("C2", "hit").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(\"\", C1:C3, 0, 1)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(\"\", C1:C3, 0, -1)"),
            EvaluatedCellValue::Number(3.0)
        );
        // Wildcard "*" matches populated "" at A2, skipping unpopulated C1!
        assert_eq!(
            s.evaluate_formula_str("=MATCH(\"*\", C1:C3, 0)"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=MATCH(\"*\", A1:A8, 0)"),
            EvaluatedCellValue::Number(2.0)
        );
        // Unicode case folding + reverse search (A6 precomposed vs A8).
        assert_eq!(
            s.evaluate_formula_str(
                "=XLOOKUP(\"CAFÉ\", A1:A8, B1:B8) + XLOOKUP(\"CAFÉ\", A1:A8, B1:B8, 0, 0, -1)"
            ),
            EvaluatedCellValue::Number(140.0)
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=XLOOKUP(\"CAFÉ\", A1:A8, B1:B8, 0, 2) + XLOOKUP(\"CAF*\", A1:A8, B1:B8, 0, 2, -1)"
            ),
            EvaluatedCellValue::Number(140.0)
        );
        // Horizontal HLOOKUP / row XLOOKUP + virtual temp array fallback + cross-sheet.
        s.set_cell("D1", "k1").unwrap();
        s.set_cell("F1", "k2").unwrap();
        s.set_cell("D2", "11").unwrap();
        s.set_cell("F2", "33").unwrap();
        assert_eq!(
            s.evaluate_formula_str(
                "=HLOOKUP(\"K2\", D1:F2, 2, FALSE) + XLOOKUP(\"k2\", D1:F1, D2:F2) + VLOOKUP(20, HSTACK({10;20;30}, {100;200;300}), 2, FALSE)"
            ),
            EvaluatedCellValue::Number(266.0)
        );
        // Dynamic self-visiting range does not cause a cycle when matching an earlier row.
        let mut s_cyc = SheetEngine::new("S");
        s_cyc.set_cell("A1", "first").unwrap();
        s_cyc.set_cell("B1", "7").unwrap();
        s_cyc
            .set_cell(
                "A5",
                "=VLOOKUP(\"first\", INDIRECT(\"A1:B10\"), 2, FALSE) + VLOOKUP(\"first\", INDIRECT(\"A1:B10\"), 2, FALSE)",
            )
            .unwrap();
        assert_eq!(s_cyc.evaluate_cell("A5"), EvaluatedCellValue::Number(14.0));
    }
}

//! Dynamic multi-dimensional filtering pivot engine (PD-255).
//!
//! Single-pass $O(R)$ aggregation over `row_fields`, `col_fields`, `filter_fields`,
//! `value_fields`, and active `filters`, collecting both pre-filter `field_items` and
//! cross-filtered `available_field_items` while enforcing explicit cell budgets and wall-clock deadlines.

use crate::memo::FastMap;
use crate::pivot::{normalize_pivot_key, pivot_input, PivotAccumulator, CLOCK_ROWS};
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, PackedCellCoordinate, PivotAggregation, PivotError,
    WorkbookEngine,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// Maximum fields allowed per axis (`row_fields`, `col_fields`, `filter_fields`, `value_fields`).
pub const MAX_PIVOT_FIELDS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PivotFieldFilter {
    pub col_offset: usize,
    pub allowed_values: Vec<String>,
    pub allow_blank: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicPivotSpec {
    pub range: CellRange,
    pub header: bool,
    pub row_fields: Vec<usize>,
    pub col_fields: Vec<usize>,
    pub filter_fields: Vec<usize>,
    pub value_fields: Vec<(usize, PivotAggregation)>,
    pub filters: Vec<PivotFieldFilter>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DynamicPivotSummary {
    pub row_tuples: Vec<Vec<String>>,
    pub col_tuples: Vec<Vec<String>>,
    pub values_matrix: Vec<Vec<Vec<f64>>>,
    pub row_totals: Vec<Vec<f64>>,
    pub col_totals: Vec<Vec<f64>>,
    pub grand_totals: Vec<f64>,
    pub field_items: BTreeMap<usize, Vec<String>>,
    pub available_field_items: BTreeMap<usize, Vec<String>>,
    pub field_blanks: BTreeMap<usize, bool>,
    pub available_field_blanks: BTreeMap<usize, bool>,
}

#[derive(Default)]
struct FieldCollector {
    col_offset: usize,
    slot: usize,
    filter_idx: Option<usize>,
    all_displays: Vec<String>,
    all_canon: HashMap<String, u32>,
    avail_displays: Vec<String>,
    avail_canon: HashSet<String>,
    all_blank: bool,
    avail_blank: bool,
    blank_id: Option<u32>,
}

impl FieldCollector {
    fn new(col_offset: usize, slot: usize, filter_idx: Option<usize>) -> Self {
        Self {
            col_offset,
            slot,
            filter_idx,
            all_displays: Vec::new(),
            all_canon: HashMap::new(),
            avail_displays: Vec::new(),
            avail_canon: HashSet::new(),
            all_blank: false,
            avail_blank: false,
            blank_id: None,
        }
    }

    #[inline]
    fn record_all(&mut self, disp: &str, canon: &str) -> u32 {
        if disp.is_empty() {
            self.all_blank = true;
            return *self.blank_id.get_or_insert_with(|| {
                let id = self.all_displays.len() as u32;
                self.all_displays.push("(Blank)".to_string());
                id
            });
        }
        if let Some(&id) = self.all_canon.get(canon) {
            return id;
        }
        let id = self.all_displays.len() as u32;
        self.all_displays.push(disp.to_string());
        self.all_canon.insert(canon.to_string(), id);
        id
    }

    #[inline]
    fn record_avail(&mut self, disp: &str, canon: &str) {
        if disp.is_empty() {
            self.avail_blank = true;
            return;
        }
        if !self.avail_canon.contains(canon) {
            self.avail_canon.insert(canon.to_string());
            self.avail_displays.push(disp.to_string());
        }
    }

    fn export_all_non_blank(&self) -> Vec<String> {
        match self.blank_id {
            Some(bid) => self
                .all_displays
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != bid as usize)
                .map(|(_, s)| s.clone())
                .collect(),
            None => self.all_displays.clone(),
        }
    }
}

struct CompiledFilter {
    slot: usize,
    allowed: HashSet<String>,
    allow_blank: bool,
}

fn normalize_filter_text(raw: &str) -> String {
    let trimmed = match crate::coercion::parse_strict_number(raw) {
        Some(n) => crate::coercion::format_general_number(n),
        None => raw.to_string(),
    };
    if trimmed.is_ascii() {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed.to_lowercase()
    }
}

#[inline]
fn pack_rc(r: usize, c: usize) -> u64 {
    ((r as u64) << 32) | (c as u64)
}

#[inline]
fn exceeds_cell_budget(rows: usize, cols: usize, vals: usize, max_cells: usize) -> bool {
    let out_rows = rows.saturating_add(2);
    let out_cols = cols.max(1).saturating_mul(vals.max(1)).saturating_add(2);
    out_rows.saturating_mul(out_cols) > max_cells
}

#[inline]
fn is_cell_blank(v: &EvaluatedCellValue) -> bool {
    match v {
        EvaluatedCellValue::Empty => true,
        EvaluatedCellValue::Text(s) => s.is_empty(),
        _ => false,
    }
}

fn validate_spec(spec: &DynamicPivotSpec, col_count: usize) -> Result<(), PivotError> {
    if spec.row_fields.len() > MAX_PIVOT_FIELDS
        || spec.col_fields.len() > MAX_PIVOT_FIELDS
        || spec.filter_fields.len() > MAX_PIVOT_FIELDS
        || spec.value_fields.len() > MAX_PIVOT_FIELDS
    {
        return Err(PivotError::InvalidSpec);
    }
    if spec.row_fields.is_empty() && spec.value_fields.is_empty() {
        return Err(PivotError::InvalidSpec);
    }
    let out_of_range = spec
        .row_fields
        .iter()
        .chain(spec.col_fields.iter())
        .chain(spec.filter_fields.iter())
        .chain(spec.value_fields.iter().map(|(c, _)| c))
        .chain(spec.filters.iter().map(|f| &f.col_offset))
        .any(|&c| c >= col_count);
    if out_of_range {
        return Err(PivotError::InvalidSpec);
    }
    Ok(())
}

fn build_compiled_filters(
    filters: &[PivotFieldFilter],
    slot_of: &impl Fn(usize) -> usize,
) -> (Vec<CompiledFilter>, HashMap<usize, usize>) {
    let mut by_col: BTreeMap<usize, (HashSet<String>, bool)> = BTreeMap::new();
    for f in filters {
        let vals: HashSet<String> = f
            .allowed_values
            .iter()
            .filter(|v| !v.is_empty())
            .map(|v| normalize_filter_text(v))
            .collect();
        by_col
            .entry(f.col_offset)
            .and_modify(|(existing, blank)| {
                existing.retain(|k| vals.contains(k));
                *blank &= f.allow_blank;
            })
            .or_insert((vals, f.allow_blank));
    }
    let mut compiled = Vec::with_capacity(by_col.len());
    let mut filter_idx_by_col = HashMap::with_capacity(by_col.len());
    for (col_off, (allowed, allow_blank)) in by_col {
        let idx = compiled.len();
        filter_idx_by_col.insert(col_off, idx);
        compiled.push(CompiledFilter {
            slot: slot_of(col_off),
            allowed,
            allow_blank,
        });
    }
    (compiled, filter_idx_by_col)
}

fn tally_dynamic(
    spec: &DynamicPivotSpec,
    max_cells: usize,
    max_populated_row: Option<u32>,
    deadline: Option<Instant>,
    mut value_at: impl FnMut(PackedCellCoordinate) -> Result<EvaluatedCellValue, PivotError>,
) -> Result<DynamicPivotSummary, PivotError> {
    let (raw_row_count, col_count) = spec.range.dimensions();
    validate_spec(spec, col_count)?;

    let val_count = spec.value_fields.len();
    if exceeds_cell_budget(0, 1, val_count, max_cells) {
        return Err(PivotError::TooLarge);
    }

    let min_r = spec.range.start.row().min(spec.range.end.row());
    let max_r = spec.range.start.row().max(spec.range.end.row());
    let min_c = spec.range.start.col().min(spec.range.end.col());
    let effective_row_count = match max_populated_row {
        Some(pop_max) if pop_max >= min_r => ((pop_max.min(max_r) - min_r) as usize) + 1,
        _ => 0,
    };
    let row_count = raw_row_count.min(effective_row_count);
    let first_row = usize::from(spec.header);

    // Collect deduplicated columns we actually need to read per row.
    let mut needed_cols: Vec<usize> = spec
        .row_fields
        .iter()
        .chain(spec.col_fields.iter())
        .chain(spec.filter_fields.iter())
        .chain(spec.value_fields.iter().map(|(c, _)| c))
        .chain(spec.filters.iter().map(|f| &f.col_offset))
        .copied()
        .collect();
    needed_cols.sort_unstable();
    needed_cols.dedup();
    let slot_of = |col: usize| needed_cols.binary_search(&col).expect("needed col exists");

    let (compiled_filters, filter_idx_by_col) = build_compiled_filters(&spec.filters, &slot_of);

    let mut tracked_cols: Vec<usize> = spec
        .row_fields
        .iter()
        .chain(spec.col_fields.iter())
        .chain(spec.filter_fields.iter())
        .copied()
        .collect();
    tracked_cols.sort_unstable();
    tracked_cols.dedup();

    let mut collectors: Vec<FieldCollector> = tracked_cols
        .iter()
        .map(|&col| FieldCollector::new(col, slot_of(col), filter_idx_by_col.get(&col).copied()))
        .collect();
    let collector_idx_of = |col: usize| tracked_cols.binary_search(&col).ok();

    let mut needs_key_norm = vec![false; needed_cols.len()];
    for fc in &collectors {
        needs_key_norm[fc.slot] = true;
    }
    for cf in &compiled_filters {
        needs_key_norm[cf.slot] = true;
    }

    let row_collector_indices: Vec<usize> = spec
        .row_fields
        .iter()
        .map(|&c| collector_idx_of(c).expect("row field tracked"))
        .collect();
    let col_collector_indices: Vec<usize> = spec
        .col_fields
        .iter()
        .map(|&c| collector_idx_of(c).expect("col field tracked"))
        .collect();
    let val_slots: Vec<(usize, PivotAggregation)> = spec
        .value_fields
        .iter()
        .map(|&(c, agg)| (slot_of(c), agg))
        .collect();

    let mut row_vals: Vec<EvaluatedCellValue> = vec![EvaluatedCellValue::Empty; needed_cols.len()];
    let mut row_keys: Vec<(String, String)> =
        vec![(String::new(), String::new()); needed_cols.len()];
    let mut row_ids_buf = [0u32; MAX_PIVOT_FIELDS];
    let mut col_ids_buf = [0u32; MAX_PIVOT_FIELDS];

    let mut row_tuples: Vec<Vec<String>> = Vec::new();
    let mut col_tuples: Vec<Vec<String>> = Vec::new();
    let mut row_map: FastMap<Vec<u32>, usize> = FastMap::default();
    let mut col_map: FastMap<Vec<u32>, usize> = FastMap::default();
    let mut cell_slots: FastMap<u64, usize> = FastMap::default();
    let mut buckets: Vec<PivotAccumulator> = Vec::new();
    let mut row_accs: Vec<PivotAccumulator> = Vec::new();
    let mut col_accs: Vec<PivotAccumulator> = Vec::new();
    let mut grand_accs: Vec<PivotAccumulator> = vec![PivotAccumulator::default(); val_count];

    for r_off in first_row..row_count {
        if (r_off as u32).is_multiple_of(CLOCK_ROWS)
            && deadline.is_some_and(|d| Instant::now() >= d)
        {
            return Err(PivotError::TimedOut);
        }
        let row_idx = min_r + r_off as u32;
        let mut any_non_empty = false;
        for (slot, &col_off) in needed_cols.iter().enumerate() {
            let Ok(coord) = PackedCellCoordinate::new(row_idx, min_c + col_off as u32) else {
                row_vals[slot] = EvaluatedCellValue::Empty;
                continue;
            };
            let val = value_at(coord)?;
            if !is_cell_blank(&val) {
                any_non_empty = true;
            }
            row_vals[slot] = val;
        }
        if !any_non_empty {
            continue;
        }

        for (slot, need) in needs_key_norm.iter().enumerate() {
            if *need {
                let v = std::mem::replace(&mut row_vals[slot], EvaluatedCellValue::Empty);
                let is_val_col = val_slots.iter().any(|&(vs, _)| vs == slot);
                if is_val_col {
                    row_vals[slot] = v.clone();
                }
                row_keys[slot] = normalize_pivot_key(v);
            }
        }

        for fc in &mut collectors {
            let (ref disp, ref canon) = row_keys[fc.slot];
            fc.record_all(disp, canon);
        }

        let mut fail_count = 0usize;
        let mut only_failed = 0usize;
        for (f_idx, cf) in compiled_filters.iter().enumerate() {
            let (_, ref canon) = row_keys[cf.slot];
            let pass = if canon.is_empty() {
                cf.allow_blank
            } else {
                cf.allowed.contains(canon)
            };
            if !pass {
                fail_count += 1;
                only_failed = f_idx;
                if fail_count > 1 {
                    break;
                }
            }
        }

        if fail_count <= 1 {
            for fc in &mut collectors {
                if fail_count == 0 || fc.filter_idx == Some(only_failed) {
                    let (ref disp, ref canon) = row_keys[fc.slot];
                    fc.record_avail(disp, canon);
                }
            }
        }
        if fail_count > 0 {
            continue;
        }

        // Intern row tuple (O(1) stack slice lookup on hits)
        for (k, &ci) in row_collector_indices.iter().enumerate() {
            let fc = &mut collectors[ci];
            let (ref disp, ref canon) = row_keys[fc.slot];
            row_ids_buf[k] = fc.record_all(disp, canon);
        }
        let r_slice = &row_ids_buf[..row_collector_indices.len()];
        let r_idx = if let Some(&existing) = row_map.get(r_slice) {
            existing
        } else {
            let idx = row_tuples.len();
            let tuple: Vec<String> = row_collector_indices
                .iter()
                .zip(r_slice.iter())
                .map(|(&ci, &id)| collectors[ci].all_displays[id as usize].clone())
                .collect();
            row_tuples.push(tuple);
            row_accs.resize(row_accs.len() + val_count, PivotAccumulator::default());
            if exceeds_cell_budget(row_tuples.len(), col_tuples.len(), val_count, max_cells) {
                return Err(PivotError::TooLarge);
            }
            row_map.insert(r_slice.to_vec(), idx);
            idx
        };

        // Intern column tuple
        for (k, &ci) in col_collector_indices.iter().enumerate() {
            let fc = &mut collectors[ci];
            let (ref disp, ref canon) = row_keys[fc.slot];
            col_ids_buf[k] = fc.record_all(disp, canon);
        }
        let c_slice = &col_ids_buf[..col_collector_indices.len()];
        let c_idx = if let Some(&existing) = col_map.get(c_slice) {
            existing
        } else {
            let idx = col_tuples.len();
            let tuple: Vec<String> = col_collector_indices
                .iter()
                .zip(c_slice.iter())
                .map(|(&ci, &id)| collectors[ci].all_displays[id as usize].clone())
                .collect();
            col_tuples.push(tuple);
            col_accs.resize(col_accs.len() + val_count, PivotAccumulator::default());
            if exceeds_cell_budget(row_tuples.len(), col_tuples.len(), val_count, max_cells) {
                return Err(PivotError::TooLarge);
            }
            col_map.insert(c_slice.to_vec(), idx);
            idx
        };

        if val_count > 0 {
            let rc_key = pack_rc(r_idx, c_idx);
            let cell_base = if let Some(&slot) = cell_slots.get(&rc_key) {
                slot * val_count
            } else {
                let slot = buckets.len() / val_count;
                buckets.resize(buckets.len() + val_count, PivotAccumulator::default());
                cell_slots.insert(rc_key, slot);
                slot * val_count
            };

            for (v_idx, &(val_slot, agg)) in val_slots.iter().enumerate() {
                if let Some(input) = pivot_input(&row_vals[val_slot], agg) {
                    buckets[cell_base + v_idx].add_input(input);
                    row_accs[r_idx * val_count + v_idx].add_input(input);
                    col_accs[c_idx * val_count + v_idx].add_input(input);
                    grand_accs[v_idx].add_input(input);
                }
            }
        }
    }

    if spec.row_fields.is_empty() && row_tuples.is_empty() {
        row_tuples.push(Vec::new());
        row_accs.resize(val_count, PivotAccumulator::default());
    }
    if spec.col_fields.is_empty() && col_tuples.is_empty() {
        col_tuples.push(Vec::new());
        col_accs.resize(val_count, PivotAccumulator::default());
    }
    if exceeds_cell_budget(row_tuples.len(), col_tuples.len(), val_count, max_cells) {
        return Err(PivotError::TooLarge);
    }

    let mut values_matrix = Vec::with_capacity(row_tuples.len());
    for r in 0..row_tuples.len() {
        let mut row_cells = Vec::with_capacity(col_tuples.len());
        for c in 0..col_tuples.len() {
            let vals = match cell_slots.get(&pack_rc(r, c)) {
                Some(&slot) => {
                    let base = slot * val_count;
                    spec.value_fields
                        .iter()
                        .enumerate()
                        .map(|(v, &(_, agg))| buckets[base + v].result(agg))
                        .collect()
                }
                None => vec![0.0; val_count],
            };
            row_cells.push(vals);
        }
        values_matrix.push(row_cells);
    }

    let row_totals: Vec<Vec<f64>> = (0..row_tuples.len())
        .map(|r| {
            let base = r * val_count;
            spec.value_fields
                .iter()
                .enumerate()
                .map(|(v, &(_, agg))| row_accs[base + v].result(agg))
                .collect()
        })
        .collect();

    let col_totals: Vec<Vec<f64>> = (0..col_tuples.len())
        .map(|c| {
            let base = c * val_count;
            spec.value_fields
                .iter()
                .enumerate()
                .map(|(v, &(_, agg))| col_accs[base + v].result(agg))
                .collect()
        })
        .collect();

    let grand_totals: Vec<f64> = spec
        .value_fields
        .iter()
        .enumerate()
        .map(|(v, &(_, agg))| grand_accs[v].result(agg))
        .collect();

    let mut field_items = BTreeMap::new();
    let mut available_field_items = BTreeMap::new();
    let mut field_blanks = BTreeMap::new();
    let mut available_field_blanks = BTreeMap::new();
    for fc in collectors {
        field_items.insert(fc.col_offset, fc.export_all_non_blank());
        available_field_items.insert(fc.col_offset, fc.avail_displays);
        field_blanks.insert(fc.col_offset, fc.all_blank);
        available_field_blanks.insert(fc.col_offset, fc.avail_blank);
    }

    Ok(DynamicPivotSummary {
        row_tuples,
        col_tuples,
        values_matrix,
        row_totals,
        col_totals,
        grand_totals,
        field_items,
        available_field_items,
        field_blanks,
        available_field_blanks,
    })
}

impl WorkbookEngine {
    /// Computes a multi-dimensional filtered pivot over the workbook's evaluated cells in a single pass.
    pub fn compute_dynamic_pivot_within(
        &self,
        sheet: &str,
        spec: &DynamicPivotSpec,
        max_cells: usize,
        limit: Duration,
    ) -> Result<DynamicPivotSummary, PivotError> {
        let engine = self.sheet(sheet).ok_or(PivotError::NoSheet)?;
        let deadline = Instant::now().checked_add(limit);
        if self.has_array_formulas() {
            if let Some(settled) = self.settle(limit) {
                if settled.outcome.timed_out {
                    return Err(PivotError::TimedOut);
                }
                let values = settled.session.sheet_values(sheet);
                let pop_max = values.keys().map(|c| c.row()).max();
                return tally_dynamic(spec, max_cells, pop_max, deadline, |coord| {
                    Ok(values
                        .get(&coord)
                        .cloned()
                        .unwrap_or(EvaluatedCellValue::Empty))
                });
            }
        }
        let pop_max = engine.raw_cells().keys().map(|c| c.row()).max();
        let mut ctx =
            EvalContext::with_workbook(self, engine.resolved_sheet_id()).with_time_limit(limit);
        tally_dynamic(spec, max_cells, pop_max, deadline, |coord| {
            ctx.reset_steps();
            let value = engine.eval_coord_ctx(coord, &mut ctx, 0);
            if ctx.expired() {
                return Err(PivotError::TimedOut);
            }
            Ok(value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_wb() -> WorkbookEngine {
        let mut wb = WorkbookEngine::new();
        let s = wb.add_sheet("S");
        let rows = [
            ("A1", "Region"),
            ("B1", "Category"),
            ("C1", "Quarter"),
            ("D1", "Channel"),
            ("E1", "Sales"),
            ("A2", "East"),
            ("B2", "Tech"),
            ("C2", "Q1"),
            ("D2", "Web"),
            ("E2", "100"),
            ("A3", "east"),
            ("B3", "Tech"),
            ("C3", "Q1"),
            ("D3", "Store"),
            ("E3", "50"),
            ("A4", "West"),
            ("B4", "Home"),
            ("C4", "Q2"),
            ("D4", ""),
            ("E4", "200"),
            ("A5", "North"),
            ("B5", "Tech"),
            ("C5", "Q2"),
            ("D5", "Web"),
            ("E5", "300"),
        ];
        for (c, v) in rows {
            s.set_cell(c, v).unwrap();
        }
        wb
    }

    #[test]
    fn multi_dim_pivot_filters_case_folding_and_cross_availability() {
        let wb = sample_wb();
        let spec = DynamicPivotSpec {
            range: CellRange::parse("A1:E5").unwrap(),
            header: true,
            row_fields: vec![0, 1],
            col_fields: vec![2],
            filter_fields: vec![3],
            value_fields: vec![
                (4, PivotAggregation::Sum),
                (4, PivotAggregation::Average),
                (4, PivotAggregation::Count),
            ],
            filters: vec![
                PivotFieldFilter {
                    col_offset: 0,
                    allowed_values: vec!["EAST".to_string(), "West".to_string()],
                    allow_blank: false,
                },
                PivotFieldFilter {
                    col_offset: 3,
                    allowed_values: vec!["web".to_string()],
                    allow_blank: true,
                },
            ],
        };
        let out = wb
            .compute_dynamic_pivot_within("S", &spec, 100_000, Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            out.row_tuples,
            vec![
                vec!["East".to_string(), "Tech".to_string()],
                vec!["West".to_string(), "Home".to_string()],
            ]
        );
        assert_eq!(
            out.col_tuples,
            vec![vec!["Q1".to_string()], vec!["Q2".to_string()]]
        );
        // East+Tech @ Q1 (Web=100), West+Home @ Q2 (blank=200)
        assert_eq!(out.values_matrix[0][0], vec![100.0, 100.0, 1.0]);
        assert_eq!(out.values_matrix[1][1], vec![200.0, 200.0, 1.0]);
        assert_eq!(out.grand_totals, vec![300.0, 150.0, 2.0]);
        // Pre-filter Region has East, West, North
        assert_eq!(out.field_items[&0], vec!["East", "West", "North"]);
        // Cross-filtered Region (ignoring Region filter, honoring Channel in {web, blank}) includes North!
        assert_eq!(out.available_field_items[&0], vec!["East", "West", "North"]);
        // Cross-filtered Channel (ignoring Channel filter, honoring Region in {East, West}) includes Web, Store, and blank!
        assert_eq!(out.available_field_items[&3], vec!["Web", "Store"]);
        assert!(out.available_field_blanks[&3]);
    }

    #[test]
    fn enforces_max_cells_deadline_and_invalid_spec() {
        let mut wb = WorkbookEngine::new();
        let s = wb.add_sheet("S");
        for r in 1..=200u32 {
            s.set_cell(&format!("A{r}"), format!("R{r}")).unwrap();
            s.set_cell(&format!("B{r}"), format!("C{r}")).unwrap();
            s.set_cell(&format!("C{r}"), "1").unwrap();
        }
        let spec = DynamicPivotSpec {
            range: CellRange::parse("A1:C200").unwrap(),
            header: false,
            row_fields: vec![0],
            col_fields: vec![1],
            filter_fields: vec![],
            value_fields: vec![(2, PivotAggregation::Sum)],
            filters: vec![],
        };
        assert_eq!(
            wb.compute_dynamic_pivot_within("S", &spec, 500, Duration::from_secs(5))
                .unwrap_err(),
            PivotError::TooLarge
        );
        assert_eq!(
            wb.compute_dynamic_pivot_within("S", &spec, 100_000, Duration::ZERO)
                .unwrap_err(),
            PivotError::TimedOut
        );
        let row_only = DynamicPivotSpec {
            col_fields: vec![],
            value_fields: vec![],
            ..spec.clone()
        };
        let row_only_out = wb
            .compute_dynamic_pivot_within("S", &row_only, 100_000, Duration::from_secs(5))
            .unwrap();
        assert_eq!(row_only_out.row_tuples.len(), 200);
        assert!(row_only_out.grand_totals.is_empty());

        let bad = DynamicPivotSpec {
            row_fields: vec![],
            value_fields: vec![],
            ..spec
        };
        assert_eq!(
            wb.compute_dynamic_pivot_within("S", &bad, 100_000, Duration::from_secs(5))
                .unwrap_err(),
            PivotError::InvalidSpec
        );
    }
}

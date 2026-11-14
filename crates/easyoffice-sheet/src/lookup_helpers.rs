//! Helper structures and functions for spreadsheet lookup operations.

use super::lookup_index::CellResult;
use crate::expr::Expr;
use crate::{EvaluatedCellValue, FormulaError, PackedCellCoordinate, SheetEngine};

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

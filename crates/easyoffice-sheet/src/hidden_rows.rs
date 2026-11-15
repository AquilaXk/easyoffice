//! The rows a sheet hides, for `SUBTOTAL` and `AGGREGATE` (PD-232): the rows a user (or a folded group) hid and the
//! rows a filter hid, kept apart, and what each function number does with them.
//!
//! 참조 구현: IronCalc `base/src/functions/subtotal.rs` lines 13-35 (the rule and `SubTotalMode`, `CellTableStatus`), 58-76
//! (`cell_hidden_status`), 126-132 (the two `continue`s of the row loop) and 200-225 (the function number table), MIT OR
//! Apache-2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, commit
//! 6dc9e8bae4b8d066276f399de7b3e1cf991c220b. Source and license text: `THIRD_PARTY_NOTICES.md`.
//!
//! Modifications: the reference reads the rows of its worksheet model (`Worksheet::rows`, a scan per cell) and takes a hidden row
//! for a filtered one when a table with filters holds the cell; this engine is told the two kinds by the page, as sorted runs
//! answered by a binary search. The reference counts the hidden rows for 110 and 111 as it does for 1 to 11; the function
//! numbers above 100 skip them (`subtotal_mode`, the two arms below), which is the spreadsheet program's rule.

use std::sync::Arc;

/// The spreadsheet program has a complicated way of filtering and hiding rows. The function numbers above 100 ignore the rows a
/// filter hides and the rows a user hides; 1 to 11 ignore the filtered rows and take the hidden ones into account.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SubtotalMode {
    Full,
    SkipHidden,
}

/// Whether a row is shown, hidden by the user, or hidden by a filter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowStatus {
    Normal,
    Hidden,
    Filtered,
}

/// The base function number (1 to 11) and the mode of `SUBTOTAL` function number `code`; `None` for a number it does not have.
pub(crate) fn subtotal_mode(code: i32) -> Option<(i32, SubtotalMode)> {
    match code {
        1..=11 => Some((code, SubtotalMode::Full)),
        // 110 and 111 skip hidden rows like the rest of the 100s (the reference's `Full` for them is not taken).
        101..=111 => Some((code - 100, SubtotalMode::SkipHidden)),
        _ => None,
    }
}

impl SubtotalMode {
    /// Whether a row with `status` is left out: a filtered row always, a hidden row only in `SkipHidden`.
    pub(crate) fn skips(self, status: RowStatus) -> bool {
        status == RowStatus::Filtered || (self == Self::SkipHidden && status == RowStatus::Hidden)
    }
}

/// The hidden rows of one sheet as sorted, disjoint, zero-based inclusive runs; a row in both is filtered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HiddenRows {
    hidden: Vec<(u32, u32)>,
    filtered: Vec<(u32, u32)>,
}

fn contains(runs: &[(u32, u32)], row: u32) -> bool {
    let at = runs.partition_point(|&(first, _)| first <= row);
    at > 0 && runs[at - 1].1 >= row
}

impl HiddenRows {
    /// Runs that are sorted and disjoint (a reader or the page's request guarantees it); `None` when there are none.
    pub fn new(hidden: Vec<(u32, u32)>, filtered: Vec<(u32, u32)>) -> Option<Arc<Self>> {
        (!hidden.is_empty() || !filtered.is_empty()).then(|| Arc::new(Self { hidden, filtered }))
    }

    pub(crate) fn status(&self, row: u32) -> RowStatus {
        if contains(&self.filtered, row) {
            RowStatus::Filtered
        } else if contains(&self.hidden, row) {
            RowStatus::Hidden
        } else {
            RowStatus::Normal
        }
    }

    /// The same rows after a row edit.
    pub(crate) fn shifted(&self, edit: &crate::axis::AxisEdit) -> Option<Arc<Self>> {
        Self::new(
            crate::axis::shift_runs(&self.hidden, edit),
            crate::axis::shift_runs(&self.filtered, edit),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_function_numbers_split_into_the_two_modes() {
        for code in 1..=11 {
            assert_eq!(subtotal_mode(code), Some((code, SubtotalMode::Full)));
            assert_eq!(
                subtotal_mode(code + 100),
                Some((code, SubtotalMode::SkipHidden))
            );
        }
        for code in [-1, 0, 12, 100, 112, 999] {
            assert_eq!(subtotal_mode(code), None, "{code}");
        }
    }

    #[test]
    fn a_filtered_row_is_always_left_out_a_hidden_one_only_by_the_100s() {
        let rows = HiddenRows::new(vec![(2, 3), (9, 9)], vec![(3, 5)]).unwrap();
        assert_eq!(rows.status(1), RowStatus::Normal);
        assert_eq!(rows.status(2), RowStatus::Hidden);
        assert_eq!(rows.status(3), RowStatus::Filtered, "both: filtered");
        assert_eq!(rows.status(5), RowStatus::Filtered);
        assert_eq!(rows.status(6), RowStatus::Normal);
        assert_eq!(rows.status(9), RowStatus::Hidden);
        assert!(SubtotalMode::Full.skips(RowStatus::Filtered));
        assert!(SubtotalMode::SkipHidden.skips(RowStatus::Filtered));
        assert!(!SubtotalMode::Full.skips(RowStatus::Hidden));
        assert!(SubtotalMode::SkipHidden.skips(RowStatus::Hidden));
        assert!(!SubtotalMode::SkipHidden.skips(RowStatus::Normal));
        assert!(HiddenRows::new(vec![], vec![]).is_none());
    }
}

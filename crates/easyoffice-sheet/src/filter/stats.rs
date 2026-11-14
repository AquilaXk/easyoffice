//! The criteria that compare a cell with the other cells of its column: the largest or smallest
//! numbers, a share of them, and the numbers above or below the average (PD-226).
//!
//! The reference filter model has no such criterion (it declares a dynamic filter type and never uses
//! it), so the rules are the file format's (ECMA-376 18.3.2.10 `top10`, 18.3.2.5 `dynamicFilter`) and the spreadsheet
//! program's documented behaviour: only number cells compete, the cells tied with the last one shown
//! are shown too, a percent share is rounded down but never below one cell, and a cell that is not a
//! number is hidden. The cut-off is found by selection in linear time instead of a sort.

use super::{CellAux, RowBits};
use easyoffice_formats::FilterCriteria;

/// Hides, among the rows `hidden` leaves visible, the rows `criteria` does not show. `criteria` must
/// be `Top` or `Average`.
pub(super) fn hide_by_statistic(
    criteria: &FilterCriteria,
    aux: &CellAux,
    rows: usize,
    hidden: &mut RowBits,
) {
    let numbers: Vec<f64> = (0..rows)
        .filter(|&i| !hidden.get(i))
        .filter_map(|i| aux.number(i))
        .collect();
    let keep: Box<dyn Fn(f64) -> bool> = match *criteria {
        FilterCriteria::Top {
            bottom,
            percent,
            count,
        } => match cut_off(&numbers, bottom, percent, count) {
            Some(edge) if bottom => Box::new(move |n| n <= edge),
            Some(edge) => Box::new(move |n| n >= edge),
            None => Box::new(|_| false),
        },
        FilterCriteria::Average { above } => {
            let mean = numbers.iter().sum::<f64>() / numbers.len().max(1) as f64;
            if above {
                Box::new(move |n| n > mean)
            } else {
                Box::new(move |n| n < mean)
            }
        }
        _ => return,
    };
    // Every visible row that is not a number the criterion keeps goes.
    for i in 0..rows {
        if !hidden.get(i) && !aux.number(i).is_some_and(&keep) {
            hidden.set(i);
        }
    }
}

/// The number the last shown cell has, `None` when no cell shows.
fn cut_off(numbers: &[f64], bottom: bool, percent: bool, count: u32) -> Option<f64> {
    if numbers.is_empty() {
        return None;
    }
    let total = numbers.len();
    let wanted = if percent {
        (total as u64 * u64::from(count) / 100).max(1)
    } else {
        u64::from(count)
    };
    let k = usize::try_from(wanted).unwrap_or(usize::MAX).min(total);
    let mut values = numbers.to_vec();
    let at = if bottom { k - 1 } else { total - k };
    let (_, edge, _) = values.select_nth_unstable_by(at, f64::total_cmp);
    Some(*edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(criteria: &FilterCriteria, numbers: &[f64]) -> Vec<bool> {
        let aux = CellAux {
            numbers: numbers.to_vec(),
            ..CellAux::default()
        };
        let mut hidden = RowBits::new(numbers.len());
        hide_by_statistic(criteria, &aux, numbers.len(), &mut hidden);
        (0..numbers.len()).map(|i| !hidden.get(i)).collect()
    }

    const N: f64 = f64::NAN;

    #[test]
    fn top_and_bottom_items_keep_the_ties() {
        let nums = [5.0, 9.0, N, 9.0, 1.0, 7.0, 3.0];
        let top = |bottom, count| FilterCriteria::Top {
            bottom,
            percent: false,
            count,
        };
        // Two largest are 9, 9 (a tie at the edge keeps both); three largest add 7.
        assert_eq!(
            run(&top(false, 2), &nums),
            [false, true, false, true, false, false, false]
        );
        assert_eq!(
            run(&top(false, 3), &nums),
            [false, true, false, true, false, true, false]
        );
        // Three of the smallest: 1, 3, 5.
        assert_eq!(
            run(&top(true, 3), &nums),
            [true, false, false, false, true, false, true]
        );
        // Asking for more than there are keeps every number and still hides the text.
        assert_eq!(
            run(&top(false, 99), &nums),
            [true, true, false, true, true, true, true]
        );
        // A tie at the edge: top 1 of 4, 4, 2 keeps both fours.
        assert_eq!(run(&top(false, 1), &[4.0, 4.0, 2.0]), [true, true, false]);
    }

    #[test]
    fn a_percent_share_rounds_down_to_at_least_one_cell() {
        let nums: Vec<f64> = (1..=20).map(f64::from).collect();
        let pct = |bottom, count| FilterCriteria::Top {
            bottom,
            percent: true,
            count,
        };
        let shown = |c: &FilterCriteria| run(c, &nums).iter().filter(|s| **s).count();
        assert_eq!(shown(&pct(false, 10)), 2);
        assert_eq!(shown(&pct(true, 25)), 5);
        assert_eq!(
            shown(&pct(false, 1)),
            1,
            "20 * 1% is 0.2 cells, at least one is shown"
        );
        assert_eq!(shown(&pct(false, 100)), 20);
        assert!(run(&pct(false, 10), &nums)[19] && run(&pct(false, 10), &nums)[18]);
    }

    #[test]
    fn above_and_below_the_average() {
        let nums = [1.0, 2.0, 3.0, 10.0, N];
        let avg = |above| FilterCriteria::Average { above };
        assert_eq!(run(&avg(true), &nums), [false, false, false, true, false]);
        assert_eq!(run(&avg(false), &nums), [true, true, true, false, false]);
        // The average itself is neither above nor below.
        assert_eq!(run(&avg(true), &[2.0, 2.0]), [false, false]);
        assert_eq!(run(&avg(false), &[]), Vec::<bool>::new());
    }

    #[test]
    fn rows_already_hidden_do_not_compete() {
        let aux = CellAux {
            numbers: vec![100.0, 5.0, 4.0],
            ..CellAux::default()
        };
        let mut hidden = RowBits::new(3);
        hidden.set(0);
        let c = FilterCriteria::Top {
            bottom: false,
            percent: false,
            count: 1,
        };
        hide_by_statistic(&c, &aux, 3, &mut hidden);
        assert!(hidden.get(0) && !hidden.get(1) && hidden.get(2));
    }
}

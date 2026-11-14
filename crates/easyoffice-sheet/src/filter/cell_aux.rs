//! What a column's criteria read besides the displayed text of its rows: the number a cell holds,
//! whether it shows as a date, and its fill and text colours (PD-226).

use super::RowBits;
use std::collections::BTreeMap;

/// A colour slot with no colour of its own. Colours are `0xRRGGBB`, so no colour can equal it.
pub const NO_COLOR: u32 = u32::MAX;

/// Per-row facts of one column, row `i` being the data row `i` below the header. A vector shorter
/// than the rows (the default is empty) stands for rows without that fact: no number, not a date,
/// no colour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellAux {
    /// The number of a cell that holds one, `NaN` for every other cell.
    pub numbers: Vec<f64>,
    /// The cells whose number format shows a date or a time.
    pub dates: RowBits,
    pub fills: Vec<u32>,
    pub fonts: Vec<u32>,
}

impl CellAux {
    #[inline]
    pub fn number(&self, i: usize) -> Option<f64> {
        self.numbers.get(i).copied().filter(|n| !n.is_nan())
    }

    /// The serial number of a cell that shows as a date or a time.
    #[inline]
    pub fn date(&self, i: usize) -> Option<f64> {
        self.dates.get(i).then(|| self.number(i)).flatten()
    }

    /// Whether the caller told anything about the fill (or text) colours of the column. A colour
    /// condition over a column it told nothing about is not judged: the sheet has no colour of its own
    /// for a cell, so a colour condition a file carries (a cell colour the model dropped) only stays.
    #[inline]
    pub fn has_color(&self, font: bool) -> bool {
        !(if font { &self.fonts } else { &self.fills }).is_empty()
    }

    #[inline]
    pub fn color(&self, font: bool, i: usize) -> u32 {
        let list = if font { &self.fonts } else { &self.fills };
        list.get(i).copied().unwrap_or(NO_COLOR)
    }
}

/// Distinct colours one column offers at most.
pub const MAX_COLOR_LIST: usize = 1_000;

/// The colours of a column among the rows `excluded` leaves visible.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColorList {
    /// `(0xRRGGBB, rows)`, ascending, at most `MAX_COLOR_LIST` colours (the first ones met).
    pub colors: Vec<(u32, u32)>,
    /// Rows with no colour of their own.
    pub none: u32,
}

/// The distinct colours of `colors` (one slot per row, a short list means no colour) among the rows
/// not in `excluded`, with how many rows have each.
pub fn color_list(colors: &[u32], excluded: &RowBits) -> ColorList {
    let mut found: BTreeMap<u32, u32> = BTreeMap::new();
    let mut none = 0u32;
    let rows = excluded.len().max(colors.len());
    for i in 0..rows {
        if excluded.get(i) {
            continue;
        }
        match colors.get(i).copied().unwrap_or(NO_COLOR) {
            NO_COLOR => none += 1,
            rgb => {
                if let Some(count) = found.get_mut(&rgb) {
                    *count += 1;
                } else if found.len() < MAX_COLOR_LIST {
                    found.insert(rgb, 1);
                }
            }
        }
    }
    ColorList {
        colors: found.into_iter().collect(),
        none,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colours_of_the_visible_rows_are_counted_and_the_list_is_bounded() {
        let colors = [0xFF0000, NO_COLOR, 0x00FF00, 0xFF0000, 0x0000FF];
        let mut excluded = RowBits::new(6);
        excluded.set(4);
        let list = color_list(&colors, &excluded);
        // Row 5 is hidden; row 6 has no slot, so it has no colour.
        assert_eq!(list.colors, vec![(0x00FF00, 1), (0xFF0000, 2)]);
        assert_eq!(list.none, 2);
        let many: Vec<u32> = (0..(MAX_COLOR_LIST as u32 + 50)).collect();
        let capped = color_list(&many, &RowBits::new(many.len()));
        assert_eq!(capped.colors.len(), MAX_COLOR_LIST);
        let aux = CellAux {
            fills: vec![7],
            ..CellAux::default()
        };
        assert_eq!(aux.color(false, 0), 7);
        assert_eq!(aux.color(false, 1), NO_COLOR);
        assert_eq!(aux.color(true, 0), NO_COLOR);
    }
}

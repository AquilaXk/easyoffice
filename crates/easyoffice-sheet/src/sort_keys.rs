//! Multi-key row ordering for the sheet sort.
//!
//! Reference implementation: the comparison rules (blank cells after every other value in both
//! directions, numbers before text, text compared without case after dropping `-` and `'`) are
//! ported from the Univer sheets sort package (`packages/sheets-sort/src/controllers/utils.ts`:
//! `compareNull`, `compareNumber`, `compareString`; `commands/sheets-sort.command.ts`:
//! `reorderFnGenerator`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9), Apache-2.0, Copyright
//! 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: the sort is stable by construction, the optional case-sensitive
//! mode puts a lowercase letter before the same uppercase letter (as spreadsheet programs do), keys
//! are prepared once per row (the original re-lowercases both texts on every comparison) into
//! compact columns whose texts share one arena, and the number of keys is bounded.

use std::cmp::Ordering;

/// Sort keys (columns) one sort may use.
pub const MAX_SORT_KEYS: usize = 64;

/// One prepared cell of a key column.
#[derive(Clone, Copy)]
enum Slot {
    Empty,
    Number(f64),
    /// Byte range of the folded text in the column's arena.
    Text(u32, u32),
}

/// The values of one sort key, one per row, ready to compare.
pub struct SortColumn {
    ascending: bool,
    slots: Vec<Slot>,
    /// Folded texts (lowercase, without `-` and `'`), back to back.
    folded: String,
    /// Original texts, kept only for the case-sensitive tie-break (`None` otherwise).
    originals: Option<(Vec<(u32, u32)>, String)>,
}

fn fold_into(out: &mut String, text: &str) {
    if text.is_ascii() {
        out.extend(
            text.bytes()
                .filter(|&b| b != b'-' && b != b'\'')
                .map(|b| char::from(b.to_ascii_lowercase())),
        );
    } else {
        out.extend(
            text.chars()
                .flat_map(char::to_lowercase)
                .filter(|&c| c != '-' && c != '\''),
        );
    }
}

fn span(arena: &str, start: usize) -> (u32, u32) {
    (start as u32, (arena.len() - start) as u32)
}

impl SortColumn {
    /// `capacity` is the number of rows the column will hold.
    pub fn new(ascending: bool, case_sensitive: bool, capacity: usize) -> Self {
        Self {
            ascending,
            slots: Vec::with_capacity(capacity),
            folded: String::new(),
            originals: case_sensitive.then(|| (Vec::with_capacity(capacity), String::new())),
        }
    }

    pub fn push_empty(&mut self) {
        self.slots.push(Slot::Empty);
        if let Some((spans, _)) = &mut self.originals {
            spans.push((0, 0));
        }
    }

    pub fn push_number(&mut self, n: f64) {
        self.slots.push(Slot::Number(n));
        if let Some((spans, _)) = &mut self.originals {
            spans.push((0, 0));
        }
    }

    /// Empty text counts as a blank cell.
    pub fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            self.push_empty();
            return;
        }
        let start = self.folded.len();
        fold_into(&mut self.folded, text);
        let (off, len) = span(&self.folded, start);
        self.slots.push(Slot::Text(off, len));
        if let Some((spans, arena)) = &mut self.originals {
            let at = arena.len();
            arena.push_str(text);
            spans.push(span(arena, at));
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    fn folded_text(&self, off: u32, len: u32) -> &str {
        &self.folded[off as usize..(off + len) as usize]
    }

    fn original(&self, row: u32) -> &str {
        match &self.originals {
            Some((spans, arena)) => {
                let (off, len) = spans[row as usize];
                &arena[off as usize..(off + len) as usize]
            }
            None => "",
        }
    }

    fn directed(&self, order: Ordering) -> Ordering {
        if self.ascending {
            order
        } else {
            order.reverse()
        }
    }

    fn compare(&self, a: u32, b: u32) -> Ordering {
        match (self.slots[a as usize], self.slots[b as usize]) {
            (Slot::Empty, Slot::Empty) => Ordering::Equal,
            (Slot::Empty, _) => Ordering::Greater,
            (_, Slot::Empty) => Ordering::Less,
            (Slot::Number(x), Slot::Number(y)) => {
                self.directed(x.partial_cmp(&y).unwrap_or(Ordering::Equal))
            }
            (Slot::Number(_), Slot::Text(..)) => self.directed(Ordering::Less),
            (Slot::Text(..), Slot::Number(_)) => self.directed(Ordering::Greater),
            (Slot::Text(ao, al), Slot::Text(bo, bl)) => {
                let order = self
                    .folded_text(ao, al)
                    .cmp(self.folded_text(bo, bl))
                    .then_with(|| case_first(self.original(a), self.original(b)));
                self.directed(order)
            }
        }
    }
}

/// Orders two texts that are equal without case: at the first difference a lowercase letter comes
/// before the uppercase one. Equal (`Equal`) when no original text was kept.
fn case_first(a: &str, b: &str) -> Ordering {
    for (x, y) in a.chars().zip(b.chars()) {
        if x == y {
            continue;
        }
        return match (x.is_lowercase(), y.is_lowercase()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => x.cmp(&y),
        };
    }
    Ordering::Equal
}

/// The row order the keys give: `order[i]` is the index of the row that goes to position `i`.
/// The first key decides, later keys break ties, rows that tie everywhere keep their order. All
/// columns must hold the same number of rows.
pub fn sort_order(columns: &[SortColumn]) -> Vec<u32> {
    let rows = columns.first().map_or(0, SortColumn::len);
    debug_assert!(columns.iter().all(|c| c.len() == rows));
    let mut order: Vec<u32> = (0..rows as u32).collect();
    match columns {
        [] => {}
        [only] => order.sort_by(|&a, &b| only.compare(a, b)),
        _ => order.sort_by(|&a, &b| {
            columns
                .iter()
                .map(|c| c.compare(a, b))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        }),
    }
    order
}

#[cfg(test)]
#[path = "sort_keys_tests.rs"]
mod tests;

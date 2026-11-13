//! The arrays of a sheet that spill (PD-217): where the formula sits and how many rows and columns of
//! values it filled when the file was written. The cells after the first are the formula's results,
//! not cells of their own: a reader drops their cached values, and a writer stores them as values
//! next to the formula.

/// Anchors one sheet may list; further ones read as plain formulas.
pub const MAX_SPILL_REFS: usize = 4_096;
/// Cells the lists of one sheet may cover together (the engine's spill budget, PD-217).
pub const MAX_SPILL_REF_CELLS: usize = 2_097_152;

/// One array: the anchor's cell and the size of the area, rows and columns, at least two cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpillRef {
    pub row: u32,
    pub col: u32,
    pub rows: u32,
    pub cols: u32,
}

impl SpillRef {
    pub fn area(&self) -> usize {
        self.rows as usize * self.cols as usize
    }

    /// Whether `(row, col)` is a cell of the area after the anchor.
    pub fn covers_after_anchor(&self, row: u32, col: u32) -> bool {
        row >= self.row
            && col >= self.col
            && row - self.row < self.rows
            && col - self.col < self.cols
            && (row, col) != (self.row, self.col)
    }

    /// The area in A1 notation (`B2:D4`).
    pub fn a1(&self) -> String {
        let end = |row: u32, col: u32| {
            let mut text = String::new();
            let mut n = col + 1;
            let mut letters = Vec::new();
            while n > 0 {
                letters.push(b'A' + ((n - 1) % 26) as u8);
                n = (n - 1) / 26;
            }
            letters.reverse();
            text.push_str(std::str::from_utf8(&letters).unwrap_or(""));
            text.push_str(&(row + 1).to_string());
            text
        };
        format!(
            "{}:{}",
            end(self.row, self.col),
            end(self.row + self.rows - 1, self.col + self.cols - 1)
        )
    }
}

/// The refs that fit the bounds of a sheet and the budget, one per anchor, by position.
pub fn normalize_spills(refs: &[SpillRef], max_rows: u32, max_cols: u32) -> Vec<SpillRef> {
    let mut kept: Vec<SpillRef> = Vec::new();
    let mut covered = 0usize;
    for r in refs {
        let fits = r.rows >= 1
            && r.cols >= 1
            && r.area() > 1
            && r.row.checked_add(r.rows).is_some_and(|end| end <= max_rows)
            && r.col.checked_add(r.cols).is_some_and(|end| end <= max_cols);
        if !fits || kept.len() >= MAX_SPILL_REFS || covered + r.area() > MAX_SPILL_REF_CELLS {
            continue;
        }
        covered += r.area();
        kept.push(*r);
    }
    kept.sort_unstable();
    kept.dedup_by_key(|r| (r.row, r.col));
    kept
}

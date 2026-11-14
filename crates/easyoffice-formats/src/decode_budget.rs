//! Document-wide cap on what the format decoders may reconstruct from one input (PD-175).
//!
//! A streaming reader bounds each step (an element, a record, a cell) but not their sum: one shared
//! string that a hundred thousand cells point at, one text run that a piece table names sixty
//! thousand times, or a table of deflated sections all expand a small file into gigabytes. Every
//! such expansion charges its output here before the bytes are copied, and the charge fails the
//! open (Fail-Closed) once the document total passes the limit. The structure and the 256 MiB
//! default follow the per-document allowance of the Pages reader (`pages_iwa::MAX_DOCUMENT_BYTES`),
//! which this module generalizes.

use crate::EasyOfficeError;

const MIB: usize = 1024 * 1024;

/// Decoded bytes one document may reconstruct in total: copied cell text, formula text, piece-table
/// text and inflated sections.
pub(crate) const MAX_DECODED_DOCUMENT_BYTES: usize = 256 * MIB;

/// Bytes a cell record costs besides its text: the `(row, column, String)` tuple the readers keep.
pub(crate) const CELL_OVERHEAD_BYTES: usize = std::mem::size_of::<(u32, u32, String)>();

/// Most UTF-8 bytes one UTF-16 code unit or one Windows-1252 byte can decode to.
pub(crate) const MAX_UTF8_BYTES_PER_UNIT: usize = 3;

/// Remaining decoded-byte allowance of one document.
#[derive(Debug)]
pub(crate) struct DecodeBudget {
    limit: usize,
    remaining: usize,
}

impl Default for DecodeBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl DecodeBudget {
    pub(crate) const fn new() -> Self {
        Self::with_limit(MAX_DECODED_DOCUMENT_BYTES)
    }

    pub(crate) const fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            remaining: limit,
        }
    }

    /// Bytes still available: the output ceiling for a decoder that can stop at a given size.
    pub(crate) const fn remaining(&self) -> usize {
        self.remaining
    }

    /// Takes `bytes` out of the allowance, or fails when the document has used it up.
    pub(crate) fn charge(&mut self, bytes: usize, what: &str) -> Result<(), EasyOfficeError> {
        match self.remaining.checked_sub(bytes) {
            Some(left) => {
                self.remaining = left;
                Ok(())
            }
            None => Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "{what}: 문서 전체 복원 크기가 예산({} MiB)을 넘었습니다.",
                self.limit / MIB
            ))),
        }
    }

    /// Charges one cell: its text plus the record that holds it.
    pub(crate) fn charge_cell(
        &mut self,
        text_len: usize,
        what: &str,
    ) -> Result<(), EasyOfficeError> {
        self.charge(text_len.saturating_add(CELL_OVERHEAD_BYTES), what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charges_until_the_limit_and_then_refuses() {
        let mut budget = DecodeBudget::with_limit(10);
        budget.charge(4, "a").unwrap();
        budget.charge(6, "a").unwrap();
        assert!(matches!(
            budget.charge(1, "a"),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }

    #[test]
    fn an_overflowing_charge_is_refused_not_wrapped() {
        let mut budget = DecodeBudget::with_limit(10);
        assert!(budget.charge(usize::MAX, "a").is_err());
        assert!(budget.charge_cell(usize::MAX, "a").is_err());
        budget
            .charge(10, "a")
            .expect("a refused charge takes nothing");
    }
}

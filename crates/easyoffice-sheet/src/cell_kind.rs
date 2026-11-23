//! Explicit per-cell value kinds on [`SheetEngine`], [`WorkbookEngine`], and [`RecalcSession`] (PD-241).
//!
//! Uses [`easyoffice_formats::CellValueKind`] as the single 1-byte (`#[repr(u8)]`) representation across
//! codecs and the calculation engine. Only cells whose explicit kind differs from
//! [`CellValueKind::infer`] occupy an entry in [`SheetEngine::explicit_kinds`], keeping zero memory
//! overhead on ordinary numeric, string, boolean, error, and formula cells.

use crate::dag;
use crate::recalc::{RecalcError, RecalcSession};
use crate::{EasyOfficeError, PackedCellCoordinate, SheetEngine, WorkbookEngine};
pub use easyoffice_formats::CellValueKind;

/// One cell edit carrying optional explicit value kind and phonetic guide (PD-241, PD-319).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedCellChange {
    pub sheet: String,
    pub coord: PackedCellCoordinate,
    pub raw: String,
    pub kind: Option<CellValueKind>,
    pub phonetic: Option<String>,
}

/// Normalizes `kind` to sparse delta form (`Some(k)` iff `!raw.is_empty() && k != infer(raw)`).
#[inline]
pub(crate) fn normalize_explicit_kind(
    raw: &str,
    kind: Option<CellValueKind>,
) -> Option<CellValueKind> {
    let k = kind?;
    (!raw.is_empty() && k != CellValueKind::infer(raw)).then_some(k)
}

/// Normalizes `phonetic` to sparse form (`Some(p)` iff non-empty text cell with non-empty reading).
#[inline]
pub(crate) fn normalize_phonetic<'a>(
    raw: &str,
    kind: Option<CellValueKind>,
    phonetic: Option<&'a str>,
) -> Option<&'a str> {
    let p = phonetic.filter(|s| !s.is_empty())?;
    let effective = normalize_explicit_kind(raw, kind).unwrap_or_else(|| CellValueKind::infer(raw));
    (!raw.is_empty() && effective == CellValueKind::Text).then_some(p)
}

impl SheetEngine {
    /// Effective [`CellValueKind`] of `(coord, raw)` combining sparse overrides with [`CellValueKind::infer`].
    #[inline]
    pub(crate) fn effective_cell_kind(
        &self,
        coord: PackedCellCoordinate,
        raw: &str,
    ) -> CellValueKind {
        if raw.is_empty() {
            return CellValueKind::Text;
        }
        self.explicit_kinds
            .get(&coord)
            .copied()
            .unwrap_or_else(|| CellValueKind::infer(raw))
    }

    /// Sparse explicit override stored at `coord`, if any.
    #[inline]
    pub fn explicit_cell_kind(&self, coord: PackedCellCoordinate) -> Option<CellValueKind> {
        self.explicit_kinds.get(&coord).copied()
    }

    /// Effective [`CellValueKind`] of a populated cell at `coord` (`None` when the cell is empty).
    #[inline]
    pub fn cell_kind_coord(&self, coord: PackedCellCoordinate) -> Option<CellValueKind> {
        let raw = self.cells.get(&coord)?;
        Some(self.effective_cell_kind(coord, raw))
    }

    /// Effective [`CellValueKind`] of a populated cell at `a1` (`None` when empty or invalid address).
    pub fn cell_kind(&self, a1: &str) -> Option<CellValueKind> {
        let coord = PackedCellCoordinate::parse_a1(a1).ok()?;
        self.cell_kind_coord(coord)
    }

    /// Whether `coord` carries an explicit [`CellValueKind::Text`] override (e.g. `'123`, `'=1+1`).
    #[inline]
    pub(crate) fn is_explicit_text(&self, coord: PackedCellCoordinate) -> bool {
        self.explicit_kinds.get(&coord) == Some(&CellValueKind::Text)
    }

    /// Sets `a1` with an optional explicit [`CellValueKind`] (`None` infers from `raw_value`).
    pub fn set_cell_typed(
        &mut self,
        a1: &str,
        raw_value: impl Into<String>,
        kind: Option<CellValueKind>,
    ) -> Result<(), EasyOfficeError> {
        let coord = PackedCellCoordinate::parse_a1(a1)?;
        self.set_cell_typed_coord(coord, raw_value, kind);
        Ok(())
    }

    /// Sets `coord` with an optional explicit [`CellValueKind`] (`None` infers from `raw_value`),
    /// clearing any previous phonetic guide.
    pub fn set_cell_typed_coord(
        &mut self,
        coord: PackedCellCoordinate,
        raw_value: impl Into<String>,
        kind: Option<CellValueKind>,
    ) {
        self.set_cell_typed_with_phonetic_coord(coord, raw_value, kind, None);
    }

    /// Sets `coord` with explicit [`CellValueKind`] and optional phonetic guide string (PD-319).
    pub fn set_cell_typed_with_phonetic_coord(
        &mut self,
        coord: PackedCellCoordinate,
        raw_value: impl Into<String>,
        kind: Option<CellValueKind>,
        phonetic: Option<&str>,
    ) {
        self.invalidate_plan();
        let was_populated = self.cells.contains_key(&coord);
        let was_formula = dag::is_formula(self.compiled_asts.get(&coord));
        let val = raw_value.into();
        let norm_kind = normalize_explicit_kind(&val, kind);
        match norm_kind {
            Some(explicit) => {
                self.explicit_kinds.insert(coord, explicit);
            }
            None => {
                self.explicit_kinds.remove(&coord);
            }
        }
        match normalize_phonetic(&val, norm_kind, phonetic) {
            Some(ph) => {
                self.phonetics.insert(coord, Box::from(ph));
            }
            None => {
                self.phonetics.remove(&coord);
            }
        }
        self.store_cell_preserving_kind(coord, val);
        self.patch_indexes(coord, was_populated, was_formula);
    }

    /// Updates only the phonetic guide of an existing text cell at `coord` (PD-319).
    pub fn set_cell_phonetic_coord(&mut self, coord: PackedCellCoordinate, phonetic: Option<&str>) {
        let Some(raw) = self.cells.get(&coord) else {
            self.phonetics.remove(&coord);
            return;
        };
        let kind = self.explicit_kinds.get(&coord).copied();
        match normalize_phonetic(raw, kind, phonetic) {
            Some(ph) => {
                self.phonetics.insert(coord, Box::from(ph));
            }
            None => {
                self.phonetics.remove(&coord);
            }
        }
    }

    /// Stored phonetic guide string of `coord`, if any (PD-319).
    #[inline]
    pub fn cell_phonetic_coord(&self, coord: PackedCellCoordinate) -> Option<&str> {
        self.phonetics.get(&coord).map(Box::as_ref)
    }
}

impl WorkbookEngine {
    /// Sets `a1` on `sheet_name` with an optional explicit [`CellValueKind`].
    pub fn set_cell_typed(
        &mut self,
        sheet_name: &str,
        a1: &str,
        raw_value: impl Into<String>,
        kind: Option<CellValueKind>,
    ) -> Result<(), EasyOfficeError> {
        let sheet = self.sheet_mut(sheet_name).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {sheet_name}"))
        })?;
        sheet.set_cell_typed(a1, raw_value, kind)
    }
}

impl RecalcSession {
    /// Stages one typed edit; it takes effect at the next [`RecalcSession::recalculate`].
    pub fn set_cell_typed(
        &mut self,
        sheet: &str,
        coord: PackedCellCoordinate,
        raw: &str,
        kind: Option<CellValueKind>,
    ) -> Result<(), RecalcError> {
        self.stage_one_typed(sheet, coord, raw, kind)
    }

    /// Stages a batch of typed edits (`kind == None` infers from `raw`).
    pub fn apply_typed(&mut self, changes: &[TypedCellChange]) -> Result<(), RecalcError> {
        self.apply_typed_batch(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvaluatedCellValue, FastMap, FormulaError, SortKey};
    use easyoffice_formats::{CellBox, Cmp, DataValidation, DvKind, SheetRules};
    use std::collections::HashMap;
    use std::time::Instant;

    #[test]
    fn explicit_text_cells_behave_as_strings_in_formulas_sort_and_validation() {
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("S");
        wb.set_cell_typed("S", "A1", "123", Some(CellValueKind::Text))
            .unwrap();
        wb.set_cell_typed("S", "A2", "=1+1", Some(CellValueKind::Text))
            .unwrap();
        wb.set_cell_typed("S", "A3", "TRUE", Some(CellValueKind::Text))
            .unwrap();
        wb.set_cell_typed("S", "A4", "#N/A", Some(CellValueKind::Text))
            .unwrap();
        wb.set_cell_typed("S", "A5", "10", None).unwrap();
        wb.set_cell_typed("S", "A6", "#N/A", None).unwrap();

        // Only the 4 overridden cells live in `explicit_kinds`; A5 and A6 match `infer`.
        assert_eq!(wb.sheet("S").unwrap().explicit_kinds.len(), 4);

        let eval = |expr: &str| wb.evaluate_formula_on_sheet("S", expr);
        assert_eq!(eval("=ISTEXT(A1)"), EvaluatedCellValue::Boolean(true));
        assert_eq!(eval("=ISNUMBER(A1)"), EvaluatedCellValue::Boolean(false));
        assert_eq!(eval("=SUM(A1:A5)"), EvaluatedCellValue::Number(10.0));
        assert_eq!(eval("=COUNT(A1:A5)"), EvaluatedCellValue::Number(1.0));
        assert_eq!(eval("=COUNTA(A1:A5)"), EvaluatedCellValue::Number(5.0));
        assert_eq!(eval("=A2"), EvaluatedCellValue::Text("=1+1".to_string()));
        assert_eq!(eval("=ISFORMULA(A2)"), EvaluatedCellValue::Boolean(false));
        assert_eq!(eval("=ISLOGICAL(A3)"), EvaluatedCellValue::Boolean(false));
        assert_eq!(eval("=ISTEXT(A3)"), EvaluatedCellValue::Boolean(true));
        assert_eq!(eval("=ISNA(A4)"), EvaluatedCellValue::Boolean(false));
        assert_eq!(eval("=ISTEXT(A4)"), EvaluatedCellValue::Boolean(true));
        assert_eq!(eval("=ISNA(A6)"), EvaluatedCellValue::Boolean(true));
        assert_eq!(eval("=ISERROR(A6)"), EvaluatedCellValue::Boolean(true));
        // Direct arithmetic still coerces numeric text `"123" + 1 == 124`, as spreadsheets do.
        assert_eq!(eval("=A1+1"), EvaluatedCellValue::Number(124.0));
        // Exact MATCH distinguishes text `"123"` from number `123`.
        assert_eq!(
            eval("=MATCH(\"123\",A1:A5,0)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            eval("=MATCH(123,A1:A5,0)"),
            EvaluatedCellValue::Error(FormulaError::Na)
        );

        // Sort ranks numbers (A5=10) before text (A1="123").
        let mut sorted = wb.sheet("S").unwrap().clone();
        sorted.sort_rows(0, 4, &[SortKey::asc(0)], false).unwrap();
        assert_eq!(
            sorted.evaluate_cell("A1"),
            EvaluatedCellValue::Number(10.0),
            "number 10 sorts before text rows"
        );

        // Validation rejects apostrophe-prefixed `'5` under WholeNumber rule and strips `'` for TextLength.
        let mut sess = RecalcSession::new(wb).unwrap();
        let mut whole = DataValidation::new(vec![CellBox::cell(0, 1)], DvKind::Whole);
        whole.cmp = Cmp::Between;
        whole.a = "1".to_string();
        whole.b = "10".to_string();
        let mut tlen = DataValidation::new(vec![CellBox::cell(0, 2)], DvKind::TextLength);
        tlen.cmp = Cmp::Equal;
        tlen.a = "3".to_string();
        sess.set_rules(
            "S",
            SheetRules {
                cf: Vec::new(),
                dv: vec![whole, tlen],
                ..SheetRules::default()
            },
        )
        .unwrap();
        let b1 = PackedCellCoordinate::parse_a1("B1").unwrap();
        let c1 = PackedCellCoordinate::parse_a1("C1").unwrap();
        assert_eq!(sess.validate("S", b1, "5").unwrap(), crate::Verdict::Valid);
        assert!(matches!(
            sess.validate("S", b1, "'5").unwrap(),
            crate::Verdict::Invalid { .. }
        ));
        assert_eq!(
            sess.validate("S", c1, "'123").unwrap(),
            crate::Verdict::Valid
        );
    }

    #[test]
    fn recalc_session_recomputes_when_only_cell_kind_flips_between_number_and_text() {
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("S");
        let mut sess = RecalcSession::new(wb).unwrap();
        let a1 = PackedCellCoordinate::parse_a1("A1").unwrap();
        let b1 = PackedCellCoordinate::parse_a1("B1").unwrap();
        let c1 = PackedCellCoordinate::parse_a1("C1").unwrap();
        sess.apply_typed(&[
            TypedCellChange {
                sheet: "S".into(),
                coord: a1,
                raw: "123".into(),
                kind: None,
                phonetic: None,
            },
            TypedCellChange {
                sheet: "S".into(),
                coord: b1,
                raw: "=SUM(A1)".into(),
                kind: None,
                phonetic: None,
            },
            TypedCellChange {
                sheet: "S".into(),
                coord: c1,
                raw: "=ISTEXT(A1)".into(),
                kind: None,
                phonetic: None,
            },
        ])
        .unwrap();
        let first = sess.recalculate();
        assert!(!first.circular);

        // Flip A1 from number `123` -> explicit text `'123` (same `raw`!).
        sess.set_cell_typed("S", a1, "123", Some(CellValueKind::Text))
            .unwrap();
        let second = sess.recalculate();
        assert_eq!(second.changed.len(), 2);
        assert!(second
            .changed
            .iter()
            .any(|c| c.coord == b1 && c.value == EvaluatedCellValue::Number(0.0)));
        assert!(second
            .changed
            .iter()
            .any(|c| c.coord == c1 && c.value == EvaluatedCellValue::Boolean(true)));

        // Flip back to number `123`.
        sess.set_cell_typed("S", a1, "123", None).unwrap();
        let third = sess.recalculate();
        assert!(third
            .changed
            .iter()
            .any(|c| c.coord == b1 && c.value == EvaluatedCellValue::Number(123.0)));
        assert!(third
            .changed
            .iter()
            .any(|c| c.coord == c1 && c.value == EvaluatedCellValue::Boolean(false)));
    }

    #[test]
    #[ignore = "release measurement harness"]
    fn bench_cell_kind_sparse_vs_dense_100k() {
        let mut inputs: Vec<(PackedCellCoordinate, String, CellValueKind)> =
            Vec::with_capacity(100_000);
        for r in 1..=10_000u32 {
            for c in 0..10u32 {
                let coord = PackedCellCoordinate::new(r, c).unwrap();
                let (raw, kind) = match c {
                    0..=5 => (format!("{}", r * 10 + c), CellValueKind::Number),
                    6 => (format!("label-{r}"), CellValueKind::Text),
                    7 => ("TRUE".to_string(), CellValueKind::Boolean),
                    8 => ("=A1+1".to_string(), CellValueKind::Formula),
                    _ => {
                        if r % 200 == 0 {
                            ("00123".to_string(), CellValueKind::Text)
                        } else if r % 250 == 0 {
                            ("123".to_string(), CellValueKind::Text)
                        } else {
                            ("#N/A".to_string(), CellValueKind::Error)
                        }
                    }
                };
                inputs.push((coord, raw, kind));
            }
        }

        // Baseline: dense per-cell HashMap<(u32, u32), String> tag + full float parse on every cell.
        let t0 = Instant::now();
        let mut dense: HashMap<(u32, u32), String> = HashMap::with_capacity(inputs.len());
        let mut checksum_dense = 0u64;
        for (coord, raw, kind) in &inputs {
            let _ = raw.trim().parse::<f64>();
            dense.insert((coord.row(), coord.col()), kind.tag().to_string());
        }
        for (coord, _, _) in &inputs {
            if let Some(tag) = dense.get(&(coord.row(), coord.col())) {
                checksum_dense = checksum_dense.wrapping_add(u64::from(tag.as_bytes()[0]));
            }
        }
        let dense_us = t0.elapsed().as_micros();
        let dense_entries = dense.len();

        // Optimized: 1-byte prefix `CellValueKind::infer` + sparse `explicit_kinds` delta map.
        let t1 = Instant::now();
        let mut sparse: FastMap<PackedCellCoordinate, CellValueKind> = FastMap::default();
        let mut checksum_sparse = 0u64;
        for (coord, raw, kind) in &inputs {
            if let Some(explicit) = normalize_explicit_kind(raw, Some(*kind)) {
                sparse.insert(*coord, explicit);
            }
        }
        for (coord, raw, _) in &inputs {
            let k = sparse
                .get(coord)
                .copied()
                .unwrap_or_else(|| CellValueKind::infer(raw));
            checksum_sparse = checksum_sparse.wrapping_add(u64::from(k.tag().as_bytes()[0]));
        }
        let sparse_us = t1.elapsed().as_micros();

        assert_eq!(checksum_dense, checksum_sparse);
        println!(
            "PD-241 100k cell-type bench: dense={dense_us}us ({dense_entries} entries) -> sparse={sparse_us}us ({} entries), speedup={:.2}x",
            sparse.len(),
            dense_us as f64 / sparse_us.max(1) as f64
        );
    }
}

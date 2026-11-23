//! Staging single and batched cell edits (plain and explicit-kind) into [`RecalcSession`] (PD-241).

use super::graph::{collect_refs, Ref};
use super::{CellChange, RecalcError, RecalcSession, BULK_CHANGE_THRESHOLD};
use crate::dag::is_formula;
use crate::{CellValueKind, DetSet, PackedCellCoordinate, TypedCellChange};

impl RecalcSession {
    /// Stages one edit; it takes effect at the next [`recalculate`](Self::recalculate).
    pub fn set_cell(
        &mut self,
        sheet: &str,
        coord: PackedCellCoordinate,
        raw: &str,
    ) -> Result<(), RecalcError> {
        self.stage_one_typed(sheet, coord, raw, None)
    }

    pub(crate) fn stage_one_typed(
        &mut self,
        sheet: &str,
        coord: PackedCellCoordinate,
        raw: &str,
        kind: Option<CellValueKind>,
    ) -> Result<(), RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
        let idx = self.sheet_index(sheet)?;
        self.stage(idx, coord, raw, kind, None)
    }

    /// Stages a batch of edits. A large batch rebuilds the cell indexes once instead of patching
    /// them per edit.
    pub fn apply(&mut self, changes: &[CellChange]) -> Result<(), RecalcError> {
        self.apply_items(
            changes.len(),
            changes
                .iter()
                .map(|c| (c.sheet.as_str(), c.coord, c.raw.as_str(), None, None)),
        )
    }

    pub(crate) fn apply_typed_batch(
        &mut self,
        changes: &[TypedCellChange],
    ) -> Result<(), RecalcError> {
        self.apply_items(
            changes.len(),
            changes.iter().map(|c| {
                (
                    c.sheet.as_str(),
                    c.coord,
                    c.raw.as_str(),
                    c.kind,
                    c.phonetic.as_deref(),
                )
            }),
        )
    }

    fn apply_items<'a>(
        &mut self,
        len: usize,
        items: impl Iterator<
            Item = (
                &'a str,
                PackedCellCoordinate,
                &'a str,
                Option<CellValueKind>,
                Option<&'a str>,
            ),
        >,
    ) -> Result<(), RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
        let mut resolved = Vec::with_capacity(len);
        for (sheet_name, coord, raw, kind, ph) in items {
            resolved.push((self.sheet_index(sheet_name)?, coord, raw, kind, ph));
        }
        if resolved.len() > BULK_CHANGE_THRESHOLD {
            for &(idx, _, _, _, _) in &resolved {
                self.wb.sheets_mut()[usize::from(idx)].drop_indexes();
            }
        }
        if resolved.len() >= 8 {
            let mut batch_pairs: Vec<(Ref, u32)> = Vec::new();
            let mut leaving: Vec<(u32, u16, PackedCellCoordinate, usize)> = Vec::new();
            let mut seen_vertices = DetSet::default();
            for &(sheet, coord, raw, kind, ph) in &resolved {
                let engine = &self.wb.sheets()[usize::from(sheet)];
                let old_raw = engine.cells.get(&coord).map_or("", String::as_str);
                let norm_kind = crate::cell_kind::normalize_explicit_kind(raw, kind);
                let norm_ph = crate::cell_kind::normalize_phonetic(raw, norm_kind, ph);
                if old_raw == raw
                    && engine.explicit_cell_kind(coord) == norm_kind
                    && engine.cell_phonetic_coord(coord) == norm_ph
                {
                    continue;
                }
                if let Some(vertex) = self.graph.vertex_at(sheet, coord) {
                    if seen_vertices.insert(vertex) {
                        if let Some(Ok(ast)) = engine.compiled_asts.get(&coord) {
                            collect_refs(&self.wb, sheet, ast, &mut self.refs);
                            batch_pairs.extend(self.refs.iter().map(|&r| (r, vertex)));
                        }
                        leaving.push((vertex, sheet, coord, old_raw.len()));
                    }
                }
            }
            if !leaving.is_empty() {
                if !batch_pairs.is_empty() {
                    self.graph.unregister_batch(&mut batch_pairs);
                }
                for (vertex, sheet, coord, old_len) in leaving {
                    self.formula_bytes -= old_len;
                    Self::release_vertex(&mut self.graph, &mut self.dynamic, vertex);
                    self.dirty.remove(vertex);
                    self.cyclic.remove(vertex);
                    let sid = self.wb.sheets()[usize::from(sheet)].resolved_sheet_id();
                    self.memo.remove_at(sid, coord);
                }
            }
        }
        for (idx, coord, raw, kind, ph) in resolved {
            self.stage(idx, coord, raw, kind, ph)?;
        }
        Ok(())
    }

    pub(in crate::recalc) fn stage(
        &mut self,
        sheet: u16,
        coord: PackedCellCoordinate,
        raw: &str,
        kind: Option<CellValueKind>,
        phonetic: Option<&str>,
    ) -> Result<(), RecalcError> {
        let engine = &self.wb.sheets()[usize::from(sheet)];
        let old_raw = engine.cells.get(&coord).map_or("", String::as_str);
        let old_ph_len = engine.cell_phonetic_coord(coord).map_or(0, str::len);
        let norm_kind = crate::cell_kind::normalize_explicit_kind(raw, kind);
        let norm_ph = crate::cell_kind::normalize_phonetic(raw, norm_kind, phonetic);
        if old_raw == raw
            && engine.explicit_cell_kind(coord) == norm_kind
            && engine.cell_phonetic_coord(coord) == norm_ph
        {
            return Ok(());
        }
        let new_ph_len = norm_ph.map_or(0, str::len);
        self.raw_bytes = self.raw_bytes - old_raw.len() - old_ph_len + raw.len() + new_ph_len;
        if let Some(vertex) = self.graph.vertex_at(sheet, coord) {
            if let Some(Ok(ast)) = engine.compiled_asts.get(&coord) {
                collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.unregister(vertex, &self.refs);
            }
            self.formula_bytes -= old_raw.len();
            Self::release_vertex(&mut self.graph, &mut self.dynamic, vertex);
            self.dirty.remove(vertex);
            self.cyclic.remove(vertex);
            self.memo.remove_at(engine.resolved_sheet_id(), coord);
        }
        if self.wb.sheets()[usize::from(sheet)]
            .spill
            .area_entry(coord)
            .is_some()
        {
            crate::spill::release_anchor(
                &mut self.wb.sheets_mut()[usize::from(sheet)],
                sheet,
                &mut self.memo,
                coord,
                &mut self.released,
            );
        }
        self.wb.sheets_mut()[usize::from(sheet)]
            .set_cell_typed_with_phonetic_coord(coord, raw, norm_kind, norm_ph);
        self.name_bytes = self.wb.name_bytes();
        let engine = &self.wb.sheets()[usize::from(sheet)];
        if let Some(entry) = engine.compiled_asts.get(&coord) {
            if let (true, Ok(ast)) = (is_formula(Some(entry)), entry) {
                let vertex = self.graph.add_vertex(sheet, coord);
                let volatile = collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.register(vertex, &self.refs);
                self.graph.set_volatile(vertex, volatile);
                self.dirty.insert(vertex);
                self.formula_bytes += raw.len();
            }
        }
        self.edited.push((sheet, coord));
        if self.exceeds_budget() {
            self.over_budget = true;
            return Err(RecalcError::OverBudget);
        }
        Ok(())
    }
}

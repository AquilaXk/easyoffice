//! Defined names in a [`RecalcSession`] (PD-214): a change of the names compiles again the formulas
//! that use a changed name, moves them in the dependency graph and marks them dirty, so the next
//! [`recalculate`](RecalcSession::recalculate) computes them and what reads them.

use super::graph::Ref;
use super::{collect_refs, is_formula, RecalcError, RecalcSession};
use crate::NameIssue;
use easyoffice_formats::{DefinedName, EasyOfficeError, MAX_NAMES};

impl RecalcSession {
    /// Replaces the defined names (see [`WorkbookEngine::set_names`](crate::WorkbookEngine::set_names)).
    /// Fails, changing nothing, when a name is not valid. The dependency graph outgrowing the
    /// session's budget makes the session unusable ([`RecalcError::OverBudget`]).
    pub fn set_names(&mut self, defs: &[DefinedName]) -> Result<(), RecalcError> {
        self.apply_names(defs, true).map(drop)
    }

    /// [`set_names`](Self::set_names) for the list a page holds, which may keep names it got from a
    /// file: a name that cannot be used is left out or reads as an error, and the answer lists the
    /// definitions with a problem (position and kind). Fails, changing nothing, past
    /// [`MAX_NAMES`](easyoffice_formats::MAX_NAMES) names.
    pub fn set_names_lenient(
        &mut self,
        defs: &[DefinedName],
    ) -> Result<Vec<(usize, NameIssue)>, RecalcError> {
        if defs.len() > MAX_NAMES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "이름은 최대 {MAX_NAMES}개까지 정의할 수 있습니다."
            ))
            .into());
        }
        self.apply_names(defs, false)
    }

    fn apply_names(
        &mut self,
        defs: &[DefinedName],
        strict: bool,
    ) -> Result<Vec<(usize, NameIssue)>, RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
        let plan = self.wb.plan_names(defs, strict)?;
        let issues = plan.issues.clone();
        let affected = plan.affected.clone();
        self.swap_formulas(&affected, |wb| wb.commit_names(plan))?;
        Ok(issues)
    }

    /// Compiles the formulas at `affected` again with `commit` (which changes what they compile to: the defined
    /// names or the tables), moves them in the dependency graph and marks them dirty. All the formulas that change
    /// leave and enter the graph together, so a cell they all read (the one a name stands for) is rewritten once,
    /// not once per formula. The graph outgrowing the session's budget makes the session unusable.
    pub(super) fn swap_formulas(
        &mut self,
        affected: &[(usize, crate::PackedCellCoordinate)],
        commit: impl FnOnce(&mut crate::WorkbookEngine),
    ) -> Result<(), RecalcError> {
        let mut pairs: Vec<(Ref, u32)> = Vec::new();
        let mut leaving: Vec<u32> = Vec::new();
        for &(sheet, coord) in affected {
            let sheet = sheet as u16;
            let engine = &self.wb.sheets()[usize::from(sheet)];
            let Some(vertex) = self.graph.vertex_at(sheet, coord) else {
                continue;
            };
            if let Some(Ok(ast)) = engine.compiled_asts.get(&coord) {
                collect_refs(&self.wb, sheet, ast, &mut self.refs);
                pairs.extend(self.refs.iter().map(|&r| (r, vertex)));
            }
            leaving.push(vertex);
        }
        self.graph.unregister_batch(&mut pairs);
        for vertex in leaving {
            let (sheet, coord) = self.graph.site(vertex);
            Self::release_vertex(&mut self.graph, &mut self.dynamic, vertex);
            self.dirty.remove(vertex);
            self.cyclic.remove(vertex);
            let engine = &self.wb.sheets()[usize::from(sheet)];
            self.memo.remove_at(engine.resolved_sheet_id(), coord);
        }
        commit(&mut self.wb);
        self.name_bytes = self.wb.name_bytes();
        pairs.clear();
        for &(sheet, coord) in affected {
            let sheet = sheet as u16;
            let engine = &self.wb.sheets()[usize::from(sheet)];
            if let Some(entry @ Ok(ast)) = engine.compiled_asts.get(&coord) {
                if is_formula(Some(entry)) {
                    let vertex = self.graph.add_vertex(sheet, coord);
                    let volatile = collect_refs(&self.wb, sheet, ast, &mut self.refs);
                    pairs.extend(self.refs.iter().map(|&r| (r, vertex)));
                    self.graph.set_volatile(vertex, volatile);
                    self.dirty.insert(vertex);
                }
            }
            self.edited.push((sheet, coord));
        }
        self.graph.register_batch(&mut pairs);
        if self.exceeds_budget() {
            self.over_budget = true;
            return Err(RecalcError::OverBudget);
        }
        Ok(())
    }

    /// Estimated heap bytes of what the defined names add: their definitions, the trees they expand to and the
    /// nodes the formulas that use a name hold.
    pub(super) fn name_ast_bytes(&self) -> usize {
        self.name_bytes
    }
}

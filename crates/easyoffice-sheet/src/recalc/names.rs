//! Defined names in a [`RecalcSession`] (PD-214): a change of the names compiles again the formulas
//! that use a changed name, moves them in the dependency graph and marks them dirty, so the next
//! [`recalculate`](RecalcSession::recalculate) computes them and what reads them.

use super::{collect_refs, is_formula, RecalcError, RecalcSession};
use crate::NameIssue;
use easyoffice_formats::{DefinedName, EasyOfficeError, MAX_NAMES};

/// Heap bytes of one syntax node, for what the expanded names add to a session (a node holds an
/// operator, boxed children or a short string; measured on the benchmark of PD-214 and rounded up).
const NODE_BYTES: usize = 64;

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
        for &(sheet, coord) in &affected {
            let sheet = sheet as u16;
            let engine = &self.wb.sheets()[usize::from(sheet)];
            let Some(vertex) = self.graph.vertex_at(sheet, coord) else {
                continue;
            };
            if let Some(Ok(ast)) = engine.compiled_asts.get(&coord) {
                collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.unregister(vertex, &self.refs);
            }
            self.graph.remove_vertex(vertex);
            self.dirty.remove(vertex);
            self.cyclic.remove(vertex);
            self.memo.remove_at(engine.resolved_sheet_id(), coord);
        }
        self.wb.commit_names(plan);
        for &(sheet, coord) in &affected {
            let sheet = sheet as u16;
            let engine = &self.wb.sheets()[usize::from(sheet)];
            if let Some(entry @ Ok(ast)) = engine.compiled_asts.get(&coord) {
                if is_formula(Some(entry)) {
                    let vertex = self.graph.add_vertex(sheet, coord);
                    let volatile = collect_refs(&self.wb, sheet, ast, &mut self.refs);
                    self.graph.register(vertex, &self.refs);
                    self.graph.set_volatile(vertex, volatile);
                    self.dirty.insert(vertex);
                }
            }
            self.edited.push((sheet, coord));
        }
        if self.exceeds_budget() {
            self.over_budget = true;
            return Err(RecalcError::OverBudget);
        }
        Ok(issues)
    }

    /// Estimated heap bytes of what the defined names add: their definitions and the syntax nodes the
    /// formulas that use a name hold after the names are expanded.
    pub(super) fn name_ast_bytes(&self) -> usize {
        let nodes: usize = self.wb.sheets().iter().map(|s| s.name_nodes).sum();
        nodes.saturating_mul(NODE_BYTES).saturating_add(
            self.wb
                .name_text_bytes()
                .saturating_mul(super::AST_BYTES_PER_FORMULA_BYTE),
        )
    }
}

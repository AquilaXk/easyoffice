//! A change of the workbook's tables (PD-225): the tables a structured reference is resolved against are
//! replaced for one sheet, and the formulas that name a table that changed are compiled again.

use crate::table_index::TableIndex;
use crate::workbook::NamesPlan;
use crate::{PackedCellCoordinate, WorkbookEngine};
use easyoffice_formats::{CellBox, EasyOfficeError, SheetTable};
use std::sync::Arc;

/// A change of the tables worked out but not applied: the new index and the formulas (sheet index, cell)
/// that name a table whose shape changed, or a defined name that does.
pub(crate) struct TablesPlan {
    index: Arc<TableIndex>,
    names: Option<NamesPlan>,
    /// Every formula to compile again, in order.
    pub(crate) affected: Vec<(usize, PackedCellCoordinate)>,
}

impl WorkbookEngine {
    /// The tables of `sheet`.
    pub fn tables_of(&self, sheet: &str) -> Vec<SheetTable> {
        self.tables.tables_of(sheet)
    }

    /// Every table of the workbook with the name of its sheet.
    pub fn all_tables(&self) -> Vec<(String, SheetTable)> {
        self.tables
            .all()
            .map(|(sheet, table)| (sheet.to_string(), table.clone()))
            .collect()
    }

    pub(crate) fn plan_tables(
        &self,
        sheet: &str,
        tables: Vec<SheetTable>,
    ) -> Result<TablesPlan, EasyOfficeError> {
        let target = self.find_sheet_index(sheet).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {sheet}"))
        })?;
        if !TableIndex::within_bounds(&tables) {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "표 개수·열 개수·범위가 한도를 넘었습니다.".to_string(),
            ));
        }
        let order = self.sheet_names();
        let index = Arc::new(self.tables.with_sheet(
            &self.sheets()[target].name,
            tables,
            &order,
        )?);
        let changed = self.tables.changed_keys(&index);
        let mut affected: Vec<(usize, PackedCellCoordinate)> = Vec::new();
        let mut names = None;
        if !changed.is_empty() {
            for (si, engine) in self.sheets().iter().enumerate() {
                affected.extend(
                    engine
                        .name_uses
                        .iter()
                        .filter(|(_, u)| u.keys.iter().any(|k| changed.contains(k)))
                        .map(|(&coord, _)| (si, coord)),
                );
            }
            if self.names_use_tables() {
                let defs = self.names().to_vec();
                let plan = self.plan_names_with(&defs, false, &index)?;
                affected.extend(plan.affected.iter().copied());
                names = Some(plan);
            }
            affected.sort_unstable();
            affected.dedup();
        }
        Ok(TablesPlan {
            index,
            names,
            affected,
        })
    }

    /// Installs the plan's tables; the formulas that name a changed table are compiled again.
    pub(crate) fn commit_tables(&mut self, plan: TablesPlan) {
        self.tables = Arc::clone(&plan.index);
        for sheet in self.sheets_mut() {
            sheet.tables = Arc::clone(&plan.index);
        }
        if let Some(names) = plan.names {
            self.install_names(names.table);
        }
        for &(si, coord) in &plan.affected {
            self.sheets_mut()[si].recompile_cell(coord);
        }
    }

    /// Replaces the tables of `sheet`. Fails, changing nothing, when the sheet is unknown or a bound is
    /// broken. An array may not be placed in or across a table either (PD-224); the formulas that name a table
    /// whose range or columns changed are compiled again.
    pub fn set_tables(
        &mut self,
        sheet: &str,
        tables: Vec<SheetTable>,
    ) -> Result<(), EasyOfficeError> {
        let boxes: Vec<CellBox> = tables.iter().map(|t| t.range).collect();
        let plan = self.plan_tables(sheet, tables)?;
        self.commit_tables(plan);
        if let Some(engine) = self.sheet_mut(sheet) {
            engine.set_spill_tables(boxes);
        }
        Ok(())
    }
}

//! Compiling the text of a cell (a formula, a number or a boolean), with the defined names a
//! formula uses expanded (PD-214).

use crate::parser::Parsed;
use crate::table_index::TableCtx;
use crate::{Expr, FormulaError, PackedCellCoordinate, SheetEngine};

/// The defined names one formula uses (see [`SheetEngine::name_uses`]).
#[derive(Debug, Clone)]
pub(crate) struct NameUse {
    pub(crate) keys: Box<[crate::names::NameKey]>,
    /// Nodes the formula holds in memory because of its names (counted against the workbook's bound).
    pub(crate) nodes: u32,
}

/// A cell's raw text compiled: the syntax tree (formulas, numbers, booleans) and the names it uses.
pub(crate) struct Compiled {
    pub(crate) entry: Option<Result<Expr, FormulaError>>,
    pub(crate) uses: Option<NameUse>,
}

/// What a formula that names tables but no defined name remembers (nothing is held in memory for it).
fn keys_only(mut keys: Vec<crate::names::NameKey>) -> NameUse {
    keys.sort_unstable();
    keys.dedup();
    NameUse {
        keys: keys.into_boxed_slice(),
        nodes: 0,
    }
}

/// A compiled formula that uses names: `nodes` is what it holds in memory because of them.
fn names_used(
    keys: Vec<crate::names::NameKey>,
    nodes: usize,
    entry: Result<Expr, FormulaError>,
) -> Compiled {
    Compiled {
        entry: Some(entry),
        uses: Some(NameUse {
            keys: keys.into_boxed_slice(),
            nodes: u32::try_from(nodes).unwrap_or(u32::MAX),
        }),
    }
}

impl SheetEngine {
    /// Compiles the text of a cell: a formula (with the defined names it uses expanded), a number or a
    /// boolean. `None` entry for plain text.
    pub(crate) fn compile_cell_entry(&self, coord: PackedCellCoordinate, raw: &str) -> Compiled {
        let trimmed = raw.trim();
        let entry = if let Some(formula_body) = crate::parser::strip_formula_prefix(trimmed) {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                return Compiled {
                    entry: None,
                    uses: None,
                };
            }
            return self.compile_formula(Some(coord), expr_src);
        } else if crate::coercion::is_valid_decimal_syntax(trimmed) {
            if let Some(num) = crate::coercion::parse_strict_number(trimmed) {
                Some(Ok(Expr::Number(num)))
            } else {
                Some(Err(FormulaError::Num))
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            Some(Ok(Expr::Boolean(true)))
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            Some(Ok(Expr::Boolean(false)))
        } else {
            None
        };
        Compiled { entry, uses: None }
    }

    /// A formula body compiled in this sheet: the structured references in it are resolved against the tables
    /// and the defined names it uses are replaced by their definitions (see [`names`]).
    pub(crate) fn compile_formula(
        &self,
        coord: Option<PackedCellCoordinate>,
        expr_src: &str,
    ) -> Compiled {
        let ctx = TableCtx {
            index: &self.tables,
            sheet: &self.name,
            coord,
        };
        let Parsed {
            mut expr,
            saw_name,
            table_keys,
        } = match crate::parser::parse_formula_in(expr_src, Some(&ctx)) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Compiled {
                    entry: Some(Err(e)),
                    uses: None,
                }
            }
        };
        if !saw_name {
            let uses = (!table_keys.is_empty()).then(|| keys_only(table_keys));
            return Compiled {
                entry: Some(Ok(expr)),
                uses,
            };
        }
        let mut keys = table_keys;
        let held = match self
            .names
            .expand(&mut expr, &self.name, &self.tables, coord, &mut keys)
        {
            Ok((_, held)) => held,
            Err(e) => return names_used(keys, 0, Err(e)),
        };
        // The workbook's bound: past it a formula that uses a name reads `#VALUE!` (fail closed).
        let in_use = self.name_budget.load(std::sync::atomic::Ordering::Relaxed);
        if in_use.saturating_add(held) > crate::names::MAX_WORKBOOK_NAME_NODES {
            return names_used(keys, 0, Err(FormulaError::Value));
        }
        names_used(keys, held, Ok(expr))
    }

    /// Compiles the text of the cell at `coord` again (a defined name it uses changed).
    pub(crate) fn recompile_cell(&mut self, coord: PackedCellCoordinate) {
        let Some(raw) = self.cells.get(&coord).cloned() else {
            return;
        };
        self.set_cell_coord(coord, raw);
    }

    /// Records (or forgets) the names the formula at `coord` uses.
    pub(crate) fn set_name_uses(&mut self, coord: PackedCellCoordinate, uses: Option<NameUse>) {
        use std::sync::atomic::Ordering::Relaxed;
        if let Some(old) = self.name_uses.remove(&coord) {
            self.name_budget.fetch_sub(old.nodes as usize, Relaxed);
        }
        if let Some(uses) = uses {
            self.name_budget.fetch_add(uses.nodes as usize, Relaxed);
            self.name_uses.insert(coord, uses);
        }
    }
}

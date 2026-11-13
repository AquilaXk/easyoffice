//! Compiling the text of a cell (a formula, a number or a boolean), with the defined names a
//! formula uses expanded (PD-214).

use crate::{Expr, FormulaError, PackedCellCoordinate, SheetEngine};

/// The defined names one formula uses (see [`SheetEngine::name_uses`]).
#[derive(Debug, Clone)]
pub(crate) struct NameUse {
    pub(crate) keys: Box<[crate::names::NameKey]>,
    pub(crate) nodes: u32,
}

/// A cell's raw text compiled: the syntax tree (formulas, numbers, booleans) and the names it uses.
pub(crate) struct Compiled {
    pub(crate) entry: Option<Result<Expr, FormulaError>>,
    pub(crate) uses: Option<NameUse>,
}

impl SheetEngine {
    /// Compiles the text of a cell: a formula (with the defined names it uses expanded), a number or a
    /// boolean. `None` entry for plain text.
    pub(crate) fn compile_cell_entry(&self, raw: &str) -> Compiled {
        let trimmed = raw.trim();
        let entry = if let Some(formula_body) = crate::parser::strip_formula_prefix(trimmed) {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                return Compiled {
                    entry: None,
                    uses: None,
                };
            }
            return self.compile_formula(expr_src);
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

    /// A formula body compiled in this sheet: the defined names it uses are replaced by their
    /// definitions (see [`names`]).
    pub(crate) fn compile_formula(&self, expr_src: &str) -> Compiled {
        let (mut expr, saw_name) = match crate::parser::parse_formula(expr_src) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Compiled {
                    entry: Some(Err(e)),
                    uses: None,
                }
            }
        };
        if !saw_name {
            return Compiled {
                entry: Some(Ok(expr)),
                uses: None,
            };
        }
        let mut keys = Vec::new();
        match self.names.expand(&mut expr, &self.name, &mut keys) {
            Ok(nodes) => Compiled {
                entry: Some(Ok(expr)),
                uses: Some(NameUse {
                    keys: keys.into_boxed_slice(),
                    nodes: u32::try_from(nodes).unwrap_or(u32::MAX),
                }),
            },
            Err(e) => Compiled {
                entry: Some(Err(e)),
                uses: Some(NameUse {
                    keys: keys.into_boxed_slice(),
                    nodes: 0,
                }),
            },
        }
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
        if let Some(old) = self.name_uses.remove(&coord) {
            self.name_nodes -= old.nodes as usize;
        }
        if let Some(uses) = uses {
            self.name_nodes += uses.nodes as usize;
            self.name_uses.insert(coord, uses);
        }
    }
}

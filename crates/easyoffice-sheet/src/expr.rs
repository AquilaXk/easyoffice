//! Syntax tree nodes and expressions for spreadsheet formulas.

use crate::{CellRange, FormulaError, PackedCellCoordinate};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetCellExpr {
    pub sheet: String,
    pub coord: PackedCellCoordinate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetRangeExpr {
    pub sheet: String,
    pub range: CellRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NameExpr {
    pub scope: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncCallExpr {
    pub name: String,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpillRefExpr {
    pub sheet: Option<String>,
    pub coord: PackedCellCoordinate,
}

/// The values of an array constant, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayLit {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<crate::EvaluatedCellValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SharedExpr {
    pub expr: Expr,
    pub nodes: usize,
}

/// A parsed formula expression, constrained to <= 32 bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Boolean(bool),
    ErrorLiteral(FormulaError),
    Missing,
    Cell(PackedCellCoordinate),
    Range(CellRange),
    SheetCell(Box<SheetCellExpr>),
    SheetRange(Box<SheetRangeExpr>),
    Name(Box<NameExpr>),
    Shared(Arc<SharedExpr>),
    UnaryPlus(Box<Expr>),
    UnaryMinus(Box<Expr>),
    PostfixPercent(Box<Expr>),
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    FuncCall(Box<FuncCallExpr>),
    Array(Arc<ArrayLit>),
    Var(String),
    SpillRef(Box<SpillRefExpr>),
    Call(Box<CallExpr>),
}

impl Expr {
    /// The expression itself, looking through [`Expr::Shared`].
    pub fn peeled(&self) -> &Expr {
        let mut cur = self;
        while let Expr::Shared(shared) = cur {
            cur = &shared.expr;
        }
        cur
    }

    #[inline]
    pub fn func_call(name: impl Into<String>, args: Vec<Expr>) -> Self {
        Self::FuncCall(Box::new(FuncCallExpr {
            name: name.into(),
            args,
        }))
    }

    #[inline]
    pub fn call(callee: Box<Expr>, args: Vec<Expr>) -> Self {
        Self::Call(Box::new(CallExpr { callee, args }))
    }

    #[inline]
    pub fn sheet_cell(sheet: impl Into<String>, coord: PackedCellCoordinate) -> Self {
        Self::SheetCell(Box::new(SheetCellExpr {
            sheet: sheet.into(),
            coord,
        }))
    }

    #[inline]
    pub fn sheet_range(sheet: impl Into<String>, range: CellRange) -> Self {
        Self::SheetRange(Box::new(SheetRangeExpr {
            sheet: sheet.into(),
            range,
        }))
    }

    #[inline]
    pub fn name(scope: Option<String>, name: impl Into<String>) -> Self {
        Self::Name(Box::new(NameExpr {
            scope,
            name: name.into(),
        }))
    }

    #[inline]
    pub fn spill_ref(sheet: Option<String>, coord: PackedCellCoordinate) -> Self {
        Self::SpillRef(Box::new(SpillRefExpr { sheet, coord }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expr_size_is_at_most_32_bytes() {
        assert!(
            std::mem::size_of::<Expr>() <= 32,
            "Expr size was {} bytes, expected <= 32",
            std::mem::size_of::<Expr>()
        );
    }
}

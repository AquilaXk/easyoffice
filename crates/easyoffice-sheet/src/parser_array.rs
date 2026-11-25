//! Parsing the array-specific syntax (PD-217): array constants, the spill range operator and the
//! variables of `LET` and `LAMBDA`.

use crate::parser::{
    cell_endpoint_spec, classify_failed_a1_token, range_ref_spec, ArrayLit, Parser, RefSpec, Token,
};
use crate::{CellRange, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate};
use std::sync::Arc;

/// Elements of one array constant: the evaluation budget reads the grid whole, so the constant is
/// bounded by the token budget (`MAX_TOKENS`) in any case; this keeps it a small multiple of it.
const MAX_LITERAL_CELLS: usize = 4_096;

impl Parser<'_> {
    pub(crate) fn colon_starts_structured(&self) -> bool {
        matches!(self.tokens.get(self.pos), Some(Token::Colon))
            && (matches!(self.tokens.get(self.pos + 1), Some(Token::Structured(_)))
                || (matches!(self.tokens.get(self.pos + 1), Some(Token::Ident(_)))
                    && matches!(self.tokens.get(self.pos + 2), Some(Token::Structured(_)))))
    }

    fn is_static_range_pair(lhs_raw: &str, rhs_raw: &str) -> bool {
        if CellRange::parse_endpoints(lhs_raw, rhs_raw).is_ok() {
            return true;
        }
        let is_ref_shape = |raw: &str| {
            PackedCellCoordinate::parse_a1(raw).is_ok()
                || PackedCellCoordinate::parse_col_only(raw).is_ok()
                || PackedCellCoordinate::parse_row_only(raw).is_ok()
        };
        let is_bad_cell =
            |raw: &str| !is_ref_shape(raw) && classify_failed_a1_token(raw) == FormulaError::Ref;
        (is_bad_cell(lhs_raw) && (is_ref_shape(rhs_raw) || is_bad_cell(rhs_raw)))
            || ((is_ref_shape(lhs_raw) || is_bad_cell(lhs_raw)) && is_bad_cell(rhs_raw))
    }

    pub(crate) fn colon_has_static_endpoint(&self, lhs_raw: &str, lhs_sheet: Option<&str>) -> bool {
        if !matches!(self.tokens.get(self.pos), Some(Token::Colon))
            || self.colon_starts_structured()
        {
            return false;
        }
        let Some(tok1) = self.tokens.get(self.pos + 1) else {
            return false;
        };
        let tok2 = self.tokens.get(self.pos + 2);
        match tok1 {
            Token::Number(n) if n.fract() == 0.0 && *n >= 1.0 => {
                PackedCellCoordinate::parse_row_only(lhs_raw).is_ok()
                    && !matches!(tok2, Some(Token::Bang | Token::Hash | Token::LParen))
            }
            Token::Ident(rhs_raw) => match tok2 {
                Some(Token::LParen | Token::Structured(_) | Token::Hash) => false,
                Some(Token::Bang) => {
                    let Some(lhs_sh) = lhs_sheet else {
                        return false;
                    };
                    if matches!(
                        self.tokens.get(self.pos + 4),
                        Some(Token::LParen | Token::Structured(_) | Token::Hash)
                    ) {
                        return false;
                    }
                    let same = crate::WorkbookEngine::is_same_sheet_identity(rhs_raw, lhs_sh);
                    match self.tokens.get(self.pos + 3) {
                        Some(Token::Ident(end_raw)) => {
                            !same || Self::is_static_range_pair(lhs_raw, end_raw)
                        }
                        Some(Token::Number(_)) => true,
                        _ => !same,
                    }
                }
                _ => Self::is_static_range_pair(lhs_raw, rhs_raw),
            },
            _ => false,
        }
    }

    /// Consumes the `#` of `A1#` when it is next.
    pub(crate) fn take_hash(&mut self) -> bool {
        let hash = matches!(self.peek(), Some(Token::Hash));
        if hash {
            self.pos += 1;
        }
        hash
    }

    /// Parses the cell or range after a 3D sheet span prefix (`Start:End!`).
    pub(crate) fn parse_sheet_span_reference(
        &mut self,
        start_sheet: String,
        end_sheet: String,
        quoted_whole: Option<String>,
    ) -> Result<Expr, FormulaError> {
        let Some(cell_ident) = self.next_range_endpoint_string() else {
            return Err(FormulaError::Ref);
        };
        let make_span = |range| match quoted_whole {
            Some(whole) => Expr::sheet_span_quoted(whole, start_sheet, end_sheet, range),
            None => Expr::sheet_span(start_sheet, end_sheet, range),
        };
        if self.colon_has_static_endpoint(&cell_ident, None) {
            self.next();
            let Some(end_ident) = self.next_range_endpoint_string() else {
                return Err(FormulaError::Ref);
            };
            if matches!(self.peek(), Some(Token::Bang)) {
                return Ok(Expr::ErrorLiteral(FormulaError::Ref));
            }
            return match CellRange::parse_endpoints(&cell_ident, &end_ident) {
                Ok(range) => {
                    if self.ref_specs.is_some() {
                        let spec = range_ref_spec(&cell_ident, &end_ident);
                        self.record_ref(spec);
                    }
                    Ok(make_span(range))
                }
                Err(_) => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
            };
        }
        match PackedCellCoordinate::parse_a1(&cell_ident) {
            Ok(_) if self.take_hash() => Err(FormulaError::Ref),
            Ok(coord) => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&cell_ident, coord)));
                }
                Ok(make_span(CellRange {
                    start: coord,
                    end: coord,
                }))
            }
            Err(_) => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
        }
    }

    /// `Sheet!A1` or `Sheet!A1:B2` (also `Sheet!A1:Sheet!B2`) after the `!`.
    pub(crate) fn parse_sheet_reference(&mut self, name: String) -> Result<Expr, FormulaError> {
        let Some(cell_ident) = self.next_range_endpoint_string() else {
            return Err(FormulaError::Ref);
        };
        let quoted_span = crate::sheet_span::split_quoted_sheet_span(&name)
            .map(|(s, e)| (s.to_string(), e.to_string()));
        if self.colon_has_static_endpoint(&cell_ident, Some(&name)) {
            self.next();
            let Some(s) = self.next_range_endpoint_string() else {
                return Err(FormulaError::Ref);
            };
            let (end_ident, same_sheet, repeated_bang) = if matches!(self.peek(), Some(Token::Bang))
            {
                self.next();
                let Some(s2) = self.next_range_endpoint_string() else {
                    return Err(FormulaError::Ref);
                };
                (
                    s2,
                    crate::WorkbookEngine::is_same_sheet_identity(&s, &name),
                    true,
                )
            } else {
                (s, true, false)
            };
            if !same_sheet {
                return Ok(Expr::ErrorLiteral(FormulaError::Ref));
            }
            return match CellRange::parse_endpoints(&cell_ident, &end_ident) {
                Ok(range) => {
                    if self.ref_specs.is_some() {
                        let spec = range_ref_spec(&cell_ident, &end_ident);
                        self.record_ref(spec);
                    }
                    if !repeated_bang {
                        if let Some((s_sh, e_sh)) = quoted_span {
                            return Ok(Expr::sheet_span_quoted(name, s_sh, e_sh, range));
                        }
                    }
                    Ok(Expr::sheet_range(name, range))
                }
                Err(_) => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
            };
        }
        match PackedCellCoordinate::parse_a1(&cell_ident) {
            Ok(coord) if self.take_hash() => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&cell_ident, coord)));
                }
                Ok(Expr::spill_ref(Some(name), coord))
            }
            Ok(coord) => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&cell_ident, coord)));
                }
                if let Some((s_sh, e_sh)) = quoted_span {
                    return Ok(Expr::sheet_span_quoted(
                        name,
                        s_sh,
                        e_sh,
                        CellRange {
                            start: coord,
                            end: coord,
                        },
                    ));
                }
                Ok(Expr::sheet_cell(name, coord))
            }
            Err(_) => match self.name_or_error(Some(name), cell_ident) {
                named @ Expr::Name(_) => Ok(named),
                _ => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
            },
        }
    }

    /// An array constant after its `{`: rows split by `;`, elements by `,`, each a number (signed),
    /// a text, a boolean or an error.
    pub(crate) fn parse_array_literal(&mut self) -> Result<Expr, FormulaError> {
        let mut data = Vec::new();
        let mut cols = 0usize;
        let mut rows = 0usize;
        let mut in_row = 0usize;
        loop {
            data.push(self.literal_element()?);
            in_row += 1;
            if data.len() > MAX_LITERAL_CELLS {
                return Err(FormulaError::Value);
            }
            match self.peek() {
                Some(Token::Comma) => {
                    self.pos += 1;
                }
                Some(Token::Semicolon | Token::RBrace) => {
                    if rows == 0 {
                        cols = in_row;
                    } else if in_row != cols {
                        return Err(FormulaError::Value);
                    }
                    rows += 1;
                    in_row = 0;
                    if matches!(self.peek(), Some(Token::RBrace)) {
                        self.pos += 1;
                        break;
                    }
                    self.pos += 1;
                }
                _ => return Err(FormulaError::Value),
            }
        }
        Ok(Expr::Array(Arc::new(ArrayLit { rows, cols, data })))
    }

    fn literal_element(&mut self) -> Result<EvaluatedCellValue, FormulaError> {
        let sign = match self.peek() {
            Some(Token::Minus) => Some(-1.0),
            Some(Token::Plus) => Some(1.0),
            _ => None,
        };
        if sign.is_some() {
            self.pos += 1;
        }
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        match (token, sign) {
            (Some(Token::Number(n)), sign) => {
                Ok(EvaluatedCellValue::Number(n * sign.unwrap_or(1.0)))
            }
            (Some(Token::Text(t)), None) => Ok(EvaluatedCellValue::Text(t.into_owned())),
            (Some(Token::ErrorLiteral(e)), None) => Ok(EvaluatedCellValue::Error(e)),
            (Some(Token::Ident(w)), None) if w.eq_ignore_ascii_case("TRUE") => {
                Ok(EvaluatedCellValue::Boolean(true))
            }
            (Some(Token::Ident(w)), None) if w.eq_ignore_ascii_case("FALSE") => {
                Ok(EvaluatedCellValue::Boolean(false))
            }
            _ => Err(FormulaError::Value),
        }
    }
}

/// Tells the variables of `LET` and `LAMBDA` from defined names and cell functions: a name bound by an
/// enclosing `LET` or `LAMBDA` becomes [`Expr::Var`], a call of such a name becomes [`Expr::Call`].
pub(crate) fn resolve_scopes(root: &mut Expr) {
    let mut scope: Vec<String> = Vec::new();
    resolve(root, &mut scope);
}

fn bound(scope: &[String], name: &str) -> bool {
    scope.iter().any(|v| v.eq_ignore_ascii_case(name))
}

/// The variable an argument declares. A parameter written `[x]` reaches the parser as `_xlop.x`; it is
/// kept as the variable `?X` so that `LAMBDA` can tell it is optional.
fn declare(arg: &mut Expr) -> Option<String> {
    match arg {
        Expr::Name(n) if n.scope.is_none() => {
            let name = &n.name;
            let (optional, plain) = match name.strip_prefix("_xlop.") {
                Some(rest) => (true, rest),
                None => (false, name.as_str()),
            };
            let upper = plain.to_ascii_uppercase();
            *arg = Expr::Var(if optional {
                format!("?{upper}")
            } else {
                upper.clone()
            });
            Some(upper)
        }
        _ => None,
    }
}

fn resolve(expr: &mut Expr, scope: &mut Vec<String>) {
    match expr {
        Expr::Name(n) if n.scope.is_none() && bound(scope, &n.name) => {
            *expr = Expr::Var(n.name.to_ascii_uppercase());
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            resolve(inner, scope);
        }
        Expr::Binary { lhs, rhs, .. } => {
            resolve(lhs, scope);
            resolve(rhs, scope);
        }
        Expr::Call(c) => {
            resolve(&mut c.callee, scope);
            c.args.iter_mut().for_each(|a| resolve(a, scope));
        }
        Expr::FuncCall(f) if f.name == "LET" && f.args.len() >= 3 && f.args.len() % 2 == 1 => {
            let mark = scope.len();
            let last = f.args.len() - 1;
            for i in (0..last).step_by(2) {
                resolve(&mut f.args[i + 1], scope);
                if let Some(var) = declare(&mut f.args[i]) {
                    scope.push(var);
                }
            }
            resolve(&mut f.args[last], scope);
            scope.truncate(mark);
        }
        Expr::FuncCall(f) if f.name == "LAMBDA" && !f.args.is_empty() => {
            let mark = scope.len();
            let last = f.args.len() - 1;
            for arg in &mut f.args[..last] {
                if let Some(var) = declare(arg) {
                    scope.push(var);
                }
            }
            resolve(&mut f.args[last], scope);
            scope.truncate(mark);
        }
        Expr::FuncCall(f) if bound(scope, &f.name) => {
            let mut inner = std::mem::take(&mut f.args);
            inner.iter_mut().for_each(|a| resolve(a, scope));
            *expr = Expr::call(Box::new(Expr::Var(f.name.to_ascii_uppercase())), inner);
        }
        Expr::FuncCall(f) => f.args.iter_mut().for_each(|a| resolve(a, scope)),
        _ => {}
    }
}

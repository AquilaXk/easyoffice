//! Parsing the array-specific syntax (PD-217): array constants, the spill range operator and the
//! variables of `LET` and `LAMBDA`.

use crate::parser::{ArrayLit, Parser, Token};
use crate::{EvaluatedCellValue, Expr, FormulaError};
use std::sync::Arc;

/// Elements of one array constant: the evaluation budget reads the grid whole, so the constant is
/// bounded by the token budget (`MAX_TOKENS`) in any case; this keeps it a small multiple of it.
const MAX_LITERAL_CELLS: usize = 4_096;

impl Parser {
    /// Consumes the `#` of `A1#` when it is next.
    pub(crate) fn take_hash(&mut self) -> bool {
        let hash = matches!(self.peek(), Some(Token::Hash));
        if hash {
            self.pos += 1;
        }
        hash
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
            (Some(Token::Text(t)), None) => Ok(EvaluatedCellValue::Text(t)),
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
        Expr::Name { scope: None, name } => {
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
        Expr::Name { scope: None, name } if bound(scope, name) => {
            *expr = Expr::Var(name.to_ascii_uppercase());
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            resolve(inner, scope);
        }
        Expr::Binary { lhs, rhs, .. } => {
            resolve(lhs, scope);
            resolve(rhs, scope);
        }
        Expr::Call { callee, args } => {
            resolve(callee, scope);
            args.iter_mut().for_each(|a| resolve(a, scope));
        }
        Expr::FuncCall { name, args }
            if name == "LET" && args.len() >= 3 && args.len() % 2 == 1 =>
        {
            let mark = scope.len();
            let last = args.len() - 1;
            for i in (0..last).step_by(2) {
                resolve(&mut args[i + 1], scope);
                if let Some(var) = declare(&mut args[i]) {
                    scope.push(var);
                }
            }
            resolve(&mut args[last], scope);
            scope.truncate(mark);
        }
        Expr::FuncCall { name, args } if name == "LAMBDA" && !args.is_empty() => {
            let mark = scope.len();
            let last = args.len() - 1;
            for arg in &mut args[..last] {
                if let Some(var) = declare(arg) {
                    scope.push(var);
                }
            }
            resolve(&mut args[last], scope);
            scope.truncate(mark);
        }
        Expr::FuncCall { name, args } if bound(scope, name) => {
            let mut inner = std::mem::take(args);
            inner.iter_mut().for_each(|a| resolve(a, scope));
            *expr = Expr::Call {
                callee: Box::new(Expr::Var(name.to_ascii_uppercase())),
                args: inner,
            };
        }
        Expr::FuncCall { args, .. } => args.iter_mut().for_each(|a| resolve(a, scope)),
        _ => {}
    }
}

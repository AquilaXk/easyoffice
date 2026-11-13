//! Binary operators and the order of values (the scalar evaluator's arithmetic).

use crate::{BinaryOp, EvaluatedCellValue, FormulaError, SheetEngine};

impl SheetEngine {
    pub(crate) fn apply_binary_op(
        op: BinaryOp,
        l_val: EvaluatedCellValue,
        r_val: EvaluatedCellValue,
    ) -> EvaluatedCellValue {
        match op {
            BinaryOp::Concat => {
                let ls = l_val.to_display_string();
                let rs = r_val.to_display_string();
                if ls.len().saturating_add(rs.len()) > Self::MAX_CELL_TEXT_CHARS
                    && ls.chars().count().saturating_add(rs.chars().count())
                        > Self::MAX_CELL_TEXT_CHARS
                {
                    EvaluatedCellValue::Error(FormulaError::Value)
                } else {
                    EvaluatedCellValue::Text(format!("{ls}{rs}"))
                }
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
                let ln = match l_val {
                    EvaluatedCellValue::Empty => 0.0,
                    _ => match l_val.as_number() {
                        Some(v) => v,
                        None => return EvaluatedCellValue::Error(FormulaError::Value),
                    },
                };
                let rn = match r_val {
                    EvaluatedCellValue::Empty => 0.0,
                    _ => match r_val.as_number() {
                        Some(v) => v,
                        None => return EvaluatedCellValue::Error(FormulaError::Value),
                    },
                };
                let res = match op {
                    BinaryOp::Add => ln + rn,
                    BinaryOp::Sub => ln - rn,
                    BinaryOp::Mul => ln * rn,
                    BinaryOp::Div => {
                        if rn == 0.0 {
                            return EvaluatedCellValue::Error(FormulaError::DivByZero);
                        }
                        ln / rn
                    }
                    BinaryOp::Pow => ln.powf(rn),
                    _ => unreachable!(),
                };
                if res.is_finite() {
                    EvaluatedCellValue::Number(res)
                } else {
                    EvaluatedCellValue::Error(FormulaError::Num)
                }
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let Some(ordering) = Self::compare_excel_values(&l_val, &r_val) else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let flag = match op {
                    BinaryOp::Eq => ordering.is_eq(),
                    BinaryOp::Ne => ordering.is_ne(),
                    BinaryOp::Lt => ordering.is_lt(),
                    BinaryOp::Le => ordering.is_le(),
                    BinaryOp::Gt => ordering.is_gt(),
                    BinaryOp::Ge => ordering.is_ge(),
                    _ => unreachable!(),
                };
                EvaluatedCellValue::Boolean(flag)
            }
        }
    }

    pub(crate) fn compare_excel_values(
        l_val: &EvaluatedCellValue,
        r_val: &EvaluatedCellValue,
    ) -> Option<std::cmp::Ordering> {
        use EvaluatedCellValue as V;
        match (l_val, r_val) {
            (V::Error(_), _) | (_, V::Error(_)) => None,
            (V::Empty, V::Empty) => Some(std::cmp::Ordering::Equal),
            (V::Empty, V::Number(b)) => 0.0f64.partial_cmp(b),
            (V::Number(a), V::Empty) => a.partial_cmp(&0.0),
            (V::Empty, V::Text(b)) => Some("".cmp(b.as_str())),
            (V::Text(a), V::Empty) => Some(a.as_str().cmp("")),
            (V::Empty, V::Boolean(b)) => Some(false.cmp(b)),
            (V::Boolean(a), V::Empty) => Some(a.cmp(&false)),
            (V::Number(a), V::Number(b)) => a.partial_cmp(b),
            (V::Text(a), V::Text(b)) if a.is_ascii() && b.is_ascii() => Some(
                a.bytes()
                    .map(|c| c.to_ascii_lowercase())
                    .cmp(b.bytes().map(|c| c.to_ascii_lowercase())),
            ),
            (V::Text(a), V::Text(b)) => Some(
                a.chars()
                    .flat_map(char::to_lowercase)
                    .cmp(b.chars().flat_map(char::to_lowercase)),
            ),
            (V::Boolean(a), V::Boolean(b)) => Some(a.cmp(b)),
            (V::Number(_), _) | (V::Text(_), V::Boolean(_)) => Some(std::cmp::Ordering::Less),
            (_, V::Number(_)) | (V::Boolean(_), V::Text(_)) => Some(std::cmp::Ordering::Greater),
        }
    }
}

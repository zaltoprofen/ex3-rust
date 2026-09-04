use crate::cc::ast::{BinOp, Expr, ExprKind, ScalarType, UnOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Constant {
    pub value: u32,
    pub ty: ScalarType,
}

pub(super) fn evaluate(expression: &Expr) -> Result<Constant, String> {
    match &expression.kind {
        ExprKind::Integer(literal) => Ok(Constant {
            value: literal.value,
            ty: literal.ty(),
        }),
        ExprKind::Unary { op, operand } => {
            let operand = evaluate(operand)?;
            Ok(Constant {
                value: match op {
                    UnOp::BitNot => !operand.value,
                    UnOp::Not => (operand.value == 0) as u32,
                    UnOp::Plus => operand.value,
                    UnOp::Neg => 0u32.wrapping_sub(operand.value),
                },
                ty: if *op == UnOp::Not {
                    ScalarType::Int32
                } else {
                    operand.ty
                },
            })
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let lhs = evaluate(lhs)?;
            let rhs = evaluate(rhs)?;
            let unsigned = lhs.ty == ScalarType::UInt32 || rhs.ty == ScalarType::UInt32;
            let value = match op {
                BinOp::Add => lhs.value.wrapping_add(rhs.value),
                BinOp::Sub => lhs.value.wrapping_sub(rhs.value),
                BinOp::Mul => lhs.value.wrapping_mul(rhs.value),
                BinOp::Div | BinOp::Mod if rhs.value == 0 => {
                    return Err("division by zero in constant expression".into());
                }
                BinOp::Div if unsigned => lhs.value / rhs.value,
                BinOp::Div => (lhs.value as i32).wrapping_div(rhs.value as i32) as u32,
                BinOp::Mod if unsigned => lhs.value % rhs.value,
                BinOp::Mod => (lhs.value as i32).wrapping_rem(rhs.value as i32) as u32,
                BinOp::BitAnd => lhs.value & rhs.value,
                BinOp::BitXor => lhs.value ^ rhs.value,
                BinOp::BitOr => lhs.value | rhs.value,
                BinOp::Lt if unsigned => (lhs.value < rhs.value) as u32,
                BinOp::Lt => ((lhs.value as i32) < rhs.value as i32) as u32,
                BinOp::Le if unsigned => (lhs.value <= rhs.value) as u32,
                BinOp::Le => ((lhs.value as i32) <= rhs.value as i32) as u32,
                BinOp::Gt if unsigned => (lhs.value > rhs.value) as u32,
                BinOp::Gt => ((lhs.value as i32) > rhs.value as i32) as u32,
                BinOp::Ge if unsigned => (lhs.value >= rhs.value) as u32,
                BinOp::Ge => ((lhs.value as i32) >= rhs.value as i32) as u32,
                BinOp::Eq => (lhs.value == rhs.value) as u32,
                BinOp::Ne => (lhs.value != rhs.value) as u32,
                BinOp::And => ((lhs.value != 0) && (rhs.value != 0)) as u32,
                BinOp::Or => ((lhs.value != 0) || (rhs.value != 0)) as u32,
            };
            let comparison = matches!(
                op,
                BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::Eq
                    | BinOp::Ne
                    | BinOp::And
                    | BinOp::Or
            );
            Ok(Constant {
                value,
                ty: if comparison || !unsigned {
                    ScalarType::Int32
                } else {
                    ScalarType::UInt32
                },
            })
        }
        _ => Err("initializer/case is not an integer constant expression".into()),
    }
}

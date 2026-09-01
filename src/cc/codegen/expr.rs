use super::{
    emitter::{BranchCondition, LabelKind, StackBinaryOp},
    frame::EvalContext,
    function::FunctionGenerator,
};
use crate::cc::{
    ast::{BinOp, ScalarType, UnOp},
    sema::{ResolvedExpr, ResolvedExprKind},
};

impl FunctionGenerator<'_> {
    pub(super) fn generate_expression(&mut self, expression: &ResolvedExpr, context: EvalContext) {
        match &expression.kind {
            ResolvedExprKind::Number(value) => self.load_constant(*value),
            ResolvedExprKind::Load(variable) => self.load_variable(variable, context.adjustment()),
            ResolvedExprKind::Assign { target, value } => {
                self.generate_expression(value, context);
                self.store_variable(target, context.adjustment());
            }
            ResolvedExprKind::Unary { op, operand } => self.generate_unary(*op, operand, context),
            ResolvedExprKind::Binary { op, lhs, rhs, .. }
                if matches!(op, BinOp::And | BinOp::Or) =>
            {
                self.generate_logical_expression(*op, lhs, rhs, context)
            }
            ResolvedExprKind::Binary {
                op,
                lhs,
                rhs,
                operand_type,
            } => self.generate_binary_expression(*op, lhs, rhs, *operand_type, context),
            ResolvedExprKind::Call { function, args } => {
                for (pushed, argument) in args.iter().rev().enumerate() {
                    self.generate_expression(argument, context.after_pushes(pushed));
                    self.emitter.push();
                }
                self.uses_runtime |= function.needs_runtime;
                self.emitter.call(&function.assembly_name);
                if !args.is_empty() {
                    self.emitter.adjust_sp(args.len() as isize);
                }
            }
        }
    }

    fn generate_unary(&mut self, operator: UnOp, operand: &ResolvedExpr, context: EvalContext) {
        self.generate_expression(operand, context);
        match operator {
            UnOp::Plus => {}
            UnOp::BitNot => self.emitter.complement(),
            UnOp::Neg => {
                let temporary = self.temporary_offset(context);
                self.emitter.store_sp(temporary);
                self.emitter.clear();
                self.emitter
                    .stack_binary(StackBinaryOp::Subtract, temporary);
            }
            UnOp::Not => {
                let yes = self.fresh_label(LabelKind::Not);
                let end = self.fresh_label(LabelKind::NotEnd);
                self.emitter.compare_zero();
                self.emitter.branch(BranchCondition::Equal, yes);
                self.emitter.load_immediate(0);
                self.emitter.jump(end);
                self.emitter.label(yes);
                self.emitter.load_immediate(1);
                self.emitter.label(end);
            }
        }
    }

    fn generate_binary_expression(
        &mut self,
        operator: BinOp,
        lhs: &ResolvedExpr,
        rhs: &ResolvedExpr,
        operand_type: ScalarType,
        context: EvalContext,
    ) {
        self.generate_expression(lhs, context);
        let left = self.temporary_offset(context);
        self.emitter.store_sp(left);
        self.generate_expression(rhs, context.next_temp());
        let right = self.temporary_offset(context.next_temp());
        self.emitter.store_sp(right);
        match operator {
            BinOp::Add | BinOp::Sub | BinOp::BitAnd | BinOp::BitXor | BinOp::BitOr => {
                self.emitter.load_sp(left);
                let operation = match operator {
                    BinOp::Add => StackBinaryOp::Add,
                    BinOp::Sub => StackBinaryOp::Subtract,
                    BinOp::BitAnd => StackBinaryOp::And,
                    BinOp::BitXor => StackBinaryOp::Xor,
                    BinOp::BitOr => StackBinaryOp::Or,
                    _ => unreachable!(),
                };
                self.emitter.stack_binary(operation, right);
            }
            BinOp::Mul | BinOp::Div | BinOp::Mod => {
                self.uses_runtime = true;
                self.emitter.load_sp(right);
                self.emitter.push();
                self.emitter
                    .load_sp(self.temporary_offset(context.after_push()));
                self.emitter.push();
                let operation = match operator {
                    BinOp::Mul => "mul",
                    BinOp::Div => "div",
                    BinOp::Mod => "mod",
                    _ => unreachable!(),
                };
                let signedness = if operand_type == ScalarType::UInt32 {
                    "u32"
                } else {
                    "i32"
                };
                self.emitter
                    .call(&format!("__ex3_{operation}_{signedness}"));
                self.emitter.adjust_sp(2);
            }
            _ => {
                self.emitter.load_sp(left);
                self.emitter.compare_sp(right);
                self.emit_boolean_branch(comparison_condition(operator, operand_type));
            }
        }
    }

    fn generate_logical_expression(
        &mut self,
        operator: BinOp,
        lhs: &ResolvedExpr,
        rhs: &ResolvedExpr,
        context: EvalContext,
    ) {
        let short = self.fresh_label(LabelKind::Logic);
        let end = self.fresh_label(LabelKind::LogicEnd);
        let short_condition = if operator == BinOp::And {
            BranchCondition::Equal
        } else {
            BranchCondition::NotEqual
        };
        self.generate_expression(lhs, context);
        self.emitter.compare_zero();
        self.emitter.branch(short_condition, short);
        self.generate_expression(rhs, context);
        self.emitter.compare_zero();
        self.emitter.branch(short_condition, short);
        self.emitter
            .load_immediate(if operator == BinOp::And { 1 } else { 0 });
        self.emitter.jump(end);
        self.emitter.label(short);
        self.emitter
            .load_immediate(if operator == BinOp::And { 0 } else { 1 });
        self.emitter.label(end);
    }
}

fn comparison_condition(operator: BinOp, operand_type: ScalarType) -> BranchCondition {
    let unsigned = operand_type == ScalarType::UInt32;
    match operator {
        BinOp::Eq => BranchCondition::Equal,
        BinOp::Ne => BranchCondition::NotEqual,
        BinOp::Lt if unsigned => BranchCondition::UnsignedLess,
        BinOp::Lt => BranchCondition::SignedLess,
        BinOp::Le if unsigned => BranchCondition::UnsignedLessEqual,
        BinOp::Le => BranchCondition::SignedLessEqual,
        BinOp::Gt if unsigned => BranchCondition::UnsignedGreater,
        BinOp::Gt => BranchCondition::SignedGreater,
        BinOp::Ge if unsigned => BranchCondition::UnsignedGreaterEqual,
        BinOp::Ge => BranchCondition::SignedGreaterEqual,
        _ => unreachable!(),
    }
}

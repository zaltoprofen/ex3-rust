use super::{
    builtins,
    emitter::{BranchCondition, LabelKind, StackBinaryOp},
    frame::EvalContext,
    function::FunctionGenerator,
};
use crate::cc::{
    ast::{BinOp, ScalarType, UnOp},
    sema::{ResolvedCallee, ResolvedExpr, ResolvedExprKind},
    DynamicStackSlotKind, TemporaryRole,
};

impl FunctionGenerator<'_> {
    pub(super) fn generate_expression(&mut self, expression: &ResolvedExpr, context: EvalContext) {
        self.sync_debug(&context);
        match &expression.kind {
            ResolvedExprKind::Number(value) => self.load_constant(*value),
            ResolvedExprKind::Load(variable) => self.load_variable(variable, context.adjustment()),
            ResolvedExprKind::Assign { target, value } => {
                self.generate_expression(value, context.clone());
                self.sync_debug(&context);
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
            ResolvedExprKind::Call { callee, args } => {
                let base_context = context.clone();
                let mut call_context = context;
                let callee_name = match callee {
                    ResolvedCallee::User(name) => name.clone(),
                    ResolvedCallee::Builtin(id) => builtins::lookup(*id).assembly_name.into(),
                };
                for (argument_index, argument) in args.iter().enumerate().rev() {
                    self.generate_expression(argument, call_context.clone());
                    self.sync_debug(&call_context);
                    self.emitter.push();
                    let (kind, parameter_name, ty) = match callee {
                        ResolvedCallee::User(name) => {
                            let parameter = self
                                .all_debug_symbols
                                .iter()
                                .find(|function| function.name == *name)
                                .and_then(|function| function.parameters.get(argument_index));
                            (
                                DynamicStackSlotKind::OutgoingArgument {
                                    callee: name.clone(),
                                    argument_index: u16::try_from(argument_index)
                                        .expect("argument index exceeds u16"),
                                    parameter_name: parameter
                                        .map(|parameter| parameter.name.clone()),
                                },
                                parameter.map(|parameter| parameter.name.clone()),
                                parameter
                                    .map(|parameter| parameter.ty)
                                    .or(argument.ty.scalar()),
                            )
                        }
                        ResolvedCallee::Builtin(_) => (
                            DynamicStackSlotKind::RuntimeArgument {
                                helper: callee_name.clone(),
                                argument_index: u16::try_from(argument_index)
                                    .expect("argument index exceeds u16"),
                            },
                            None,
                            argument.ty.scalar(),
                        ),
                    };
                    let display_name = parameter_name.map_or_else(
                        || format!("argument {argument_index} for {callee_name}"),
                        |name| format!("argument {argument_index} ({name}) for {callee_name}"),
                    );
                    call_context = call_context.with_dynamic_slot(kind, display_name, ty);
                }
                self.sync_debug(&call_context);
                match callee {
                    ResolvedCallee::User(name) => self.emitter.call(name),
                    ResolvedCallee::Builtin(id) => {
                        let builtin = builtins::lookup(*id);
                        self.uses_runtime |= builtin.needs_runtime;
                        self.emitter.call(builtin.assembly_name);
                    }
                }
                if !args.is_empty() {
                    self.emitter.adjust_sp(args.len() as isize);
                }
                self.sync_debug(&base_context);
            }
        }
    }

    fn generate_unary(&mut self, operator: UnOp, operand: &ResolvedExpr, context: EvalContext) {
        self.generate_expression(operand, context.clone());
        self.sync_debug(&context);
        match operator {
            UnOp::Plus => {}
            UnOp::BitNot => self.emitter.complement(),
            UnOp::Neg => {
                let temporary = self.temporary_offset(&context);
                self.emitter.store_sp(temporary);
                let active = context.with_active_temporary(
                    TemporaryRole::UnaryOperand,
                    format!("operand of -: {}", self.describe_expression(operand)),
                );
                self.sync_debug(&active);
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
        self.generate_expression(lhs, context.clone());
        self.sync_debug(&context);
        let left = self.temporary_offset(&context);
        self.emitter.store_sp(left);
        let comparison = matches!(
            operator,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne
        );
        let left_context = context.with_active_temporary(
            if comparison {
                TemporaryRole::ComparisonLeft
            } else {
                TemporaryRole::BinaryLeft
            },
            format!(
                "lhs of {}: {}",
                binary_display(operator),
                self.describe_expression(lhs)
            ),
        );
        let right_slot_context = left_context.clone().next_temp();
        self.generate_expression(rhs, right_slot_context.clone());
        self.sync_debug(&left_context);
        let right = self.temporary_offset(&right_slot_context);
        self.emitter.store_sp(right);
        let both_context = right_slot_context.with_active_temporary(
            if comparison {
                TemporaryRole::ComparisonRight
            } else {
                TemporaryRole::BinaryRight
            },
            format!(
                "rhs of {}: {}",
                binary_display(operator),
                self.describe_expression(rhs)
            ),
        );
        self.sync_debug(&both_context);
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
                let helper = format!("__ex3_{operation}_{signedness}");
                let mut runtime_context = both_context.with_dynamic_slot(
                    DynamicStackSlotKind::RuntimeArgument {
                        helper: helper.clone(),
                        argument_index: 1,
                    },
                    format!("runtime argument 1 for {helper}"),
                    Some(operand_type),
                );
                self.sync_debug(&runtime_context);
                self.emitter.load_sp(
                    self.frame
                        .temporary_offset(left_context.temporary(), runtime_context.adjustment()),
                );
                self.emitter.push();
                runtime_context = runtime_context.with_dynamic_slot(
                    DynamicStackSlotKind::RuntimeArgument {
                        helper: helper.clone(),
                        argument_index: 0,
                    },
                    format!("runtime argument 0 for {helper}"),
                    Some(operand_type),
                );
                self.sync_debug(&runtime_context);
                self.emitter.call(&helper);
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
        self.generate_expression(lhs, context.clone());
        self.sync_debug(&context);
        self.emitter.compare_zero();
        self.emitter.branch(short_condition, short);
        self.generate_expression(rhs, context.clone());
        self.sync_debug(&context);
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

fn binary_display(operator: BinOp) -> &'static str {
    match operator {
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::BitAnd => "&",
        BinOp::BitXor => "^",
        BinOp::BitOr => "|",
        BinOp::And => "&&",
        BinOp::Or => "||",
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

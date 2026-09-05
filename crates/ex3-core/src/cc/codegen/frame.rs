use crate::cc::{
    ast::{BinOp, UnOp},
    sema::{
        LocalSlot, ParamIndex, ResolvedExpr, ResolvedExprKind, ResolvedFunction, ResolvedStmt,
        ResolvedSwitchPart,
    },
};
use std::fmt;

#[derive(Clone, Copy)]
pub(super) struct TempSlot(pub usize);

#[derive(Clone, Copy)]
pub(super) struct StackOffset(pub usize);

#[derive(Clone, Copy)]
pub(super) struct StackAdjustment(pub usize);

impl fmt::Display for StackOffset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy)]
pub(super) struct FrameLayout {
    local_count: usize,
    temporary_count: usize,
    parameter_count: usize,
}

impl FrameLayout {
    pub(super) fn plan(function: &ResolvedFunction) -> Self {
        Self {
            local_count: function.local_count,
            temporary_count: temporary_count_statement(&function.body),
            parameter_count: function.parameter_count,
        }
    }

    pub(super) fn size(self) -> usize {
        self.local_count + self.temporary_count
    }

    pub(super) fn validate(self, function: &ResolvedFunction) -> Result<(), &'static str> {
        const MAX_SIGNED_IMMEDIATE: usize = i16::MAX as usize;

        let dynamic_slots = maximum_dynamic_slots_statement(&function.body);
        let stack_extent = self
            .size()
            .checked_add(1) // return address
            .and_then(|size| size.checked_add(self.parameter_count))
            .and_then(|size| size.checked_add(dynamic_slots))
            .ok_or("function stack frame is too large")?;

        // ADJSP uses a signed 16-bit immediate. Stack-relative offsets use the
        // same range, so an extent of 32768 slots has a highest offset of 32767.
        if self.size() > MAX_SIGNED_IMMEDIATE
            || dynamic_slots > MAX_SIGNED_IMMEDIATE
            || stack_extent > MAX_SIGNED_IMMEDIATE + 1
        {
            return Err("function stack frame is too large");
        }
        Ok(())
    }

    pub(super) fn local_offset(self, slot: LocalSlot, adjustment: StackAdjustment) -> StackOffset {
        debug_assert!(slot.0 < self.local_count);
        StackOffset(slot.0 + adjustment.0)
    }

    pub(super) fn temporary_offset(
        self,
        slot: TempSlot,
        adjustment: StackAdjustment,
    ) -> StackOffset {
        debug_assert!(slot.0 < self.temporary_count);
        StackOffset(self.local_count + slot.0 + adjustment.0)
    }

    pub(super) fn parameter_offset(
        self,
        index: ParamIndex,
        adjustment: StackAdjustment,
    ) -> StackOffset {
        debug_assert!(index.0 < self.parameter_count);
        StackOffset(self.size() + 1 + index.0 + adjustment.0)
    }
}

fn temporary_count_expression(expression: &ResolvedExpr) -> usize {
    match &expression.kind {
        ResolvedExprKind::Number(_) | ResolvedExprKind::Load(_) => 0,
        ResolvedExprKind::Assign { value, .. } => temporary_count_expression(value),
        ResolvedExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } => temporary_count_expression(operand).max(1),
        ResolvedExprKind::Unary { operand, .. } => temporary_count_expression(operand),
        ResolvedExprKind::Call { args, .. } => args
            .iter()
            .map(temporary_count_expression)
            .max()
            .unwrap_or(0),
        ResolvedExprKind::Binary {
            op: BinOp::And | BinOp::Or,
            lhs,
            rhs,
            ..
        } => temporary_count_expression(lhs).max(temporary_count_expression(rhs)),
        ResolvedExprKind::Binary { lhs, rhs, .. } => temporary_count_expression(lhs)
            .max(1 + temporary_count_expression(rhs))
            .max(2),
    }
}

fn maximum_dynamic_slots_expression(expression: &ResolvedExpr) -> usize {
    match &expression.kind {
        ResolvedExprKind::Number(_) | ResolvedExprKind::Load(_) => 0,
        ResolvedExprKind::Assign { value, .. } | ResolvedExprKind::Unary { operand: value, .. } => {
            maximum_dynamic_slots_expression(value)
        }
        ResolvedExprKind::Call { args, .. } => args
            .iter()
            .rev()
            .enumerate()
            .map(|(pushed, argument)| pushed + maximum_dynamic_slots_expression(argument))
            .chain(std::iter::once(args.len()))
            .max()
            .unwrap_or(0),
        ResolvedExprKind::Binary { op, lhs, rhs, .. } => {
            let runtime_arguments =
                usize::from(matches!(op, BinOp::Mul | BinOp::Div | BinOp::Mod)) * 2;
            maximum_dynamic_slots_expression(lhs)
                .max(maximum_dynamic_slots_expression(rhs))
                .max(runtime_arguments)
        }
    }
}

fn temporary_count_statement(statement: &ResolvedStmt) -> usize {
    match statement {
        ResolvedStmt::Empty
        | ResolvedStmt::Break
        | ResolvedStmt::Continue
        | ResolvedStmt::Goto(_) => 0,
        ResolvedStmt::Expr(expression) | ResolvedStmt::Return(Some(expression)) => {
            temporary_count_expression(expression)
        }
        ResolvedStmt::Return(None) => 0,
        ResolvedStmt::Decl { init, .. } => {
            init.as_ref().map(temporary_count_expression).unwrap_or(0)
        }
        ResolvedStmt::Block(statements) => statements
            .iter()
            .map(temporary_count_statement)
            .max()
            .unwrap_or(0),
        ResolvedStmt::If {
            condition,
            then_stmt,
            else_stmt,
        } => temporary_count_expression(condition)
            .max(temporary_count_statement(then_stmt))
            .max(
                else_stmt
                    .as_deref()
                    .map(temporary_count_statement)
                    .unwrap_or(0),
            ),
        ResolvedStmt::While { condition, body } => {
            temporary_count_expression(condition).max(temporary_count_statement(body))
        }
        ResolvedStmt::Switch { expression, parts } => parts
            .iter()
            .filter_map(|part| match part {
                ResolvedSwitchPart::Stmt(statement) => Some(temporary_count_statement(statement)),
                ResolvedSwitchPart::Case(_) | ResolvedSwitchPart::Default => None,
            })
            .max()
            .unwrap_or(0)
            .max(temporary_count_expression(expression))
            .max(1),
        ResolvedStmt::Label(_, body) => temporary_count_statement(body),
    }
}

fn maximum_dynamic_slots_statement(statement: &ResolvedStmt) -> usize {
    match statement {
        ResolvedStmt::Empty
        | ResolvedStmt::Break
        | ResolvedStmt::Continue
        | ResolvedStmt::Goto(_)
        | ResolvedStmt::Return(None) => 0,
        ResolvedStmt::Expr(expression) | ResolvedStmt::Return(Some(expression)) => {
            maximum_dynamic_slots_expression(expression)
        }
        ResolvedStmt::Decl { init, .. } => init
            .as_ref()
            .map(maximum_dynamic_slots_expression)
            .unwrap_or(0),
        ResolvedStmt::Block(statements) => statements
            .iter()
            .map(maximum_dynamic_slots_statement)
            .max()
            .unwrap_or(0),
        ResolvedStmt::If {
            condition,
            then_stmt,
            else_stmt,
        } => maximum_dynamic_slots_expression(condition)
            .max(maximum_dynamic_slots_statement(then_stmt))
            .max(
                else_stmt
                    .as_deref()
                    .map(maximum_dynamic_slots_statement)
                    .unwrap_or(0),
            ),
        ResolvedStmt::While { condition, body } => {
            maximum_dynamic_slots_expression(condition).max(maximum_dynamic_slots_statement(body))
        }
        ResolvedStmt::Switch { expression, parts } => maximum_dynamic_slots_expression(expression)
            .max(
                parts
                    .iter()
                    .filter_map(|part| match part {
                        ResolvedSwitchPart::Stmt(statement) => {
                            Some(maximum_dynamic_slots_statement(statement))
                        }
                        ResolvedSwitchPart::Case(_) | ResolvedSwitchPart::Default => None,
                    })
                    .max()
                    .unwrap_or(0),
            ),
        ResolvedStmt::Label(_, body) => maximum_dynamic_slots_statement(body),
    }
}

#[derive(Clone, Copy)]
pub(super) struct EvalContext {
    temporary: TempSlot,
    adjustment: StackAdjustment,
}

impl EvalContext {
    pub(super) fn root() -> Self {
        Self {
            temporary: TempSlot(0),
            adjustment: StackAdjustment(0),
        }
    }

    pub(super) fn next_temp(self) -> Self {
        Self {
            temporary: TempSlot(self.temporary.0 + 1),
            ..self
        }
    }

    pub(super) fn after_push(self) -> Self {
        Self {
            adjustment: StackAdjustment(self.adjustment.0 + 1),
            ..self
        }
    }

    pub(super) fn after_pushes(mut self, count: usize) -> Self {
        for _ in 0..count {
            self = self.after_push();
        }
        self
    }

    pub(super) fn temporary(self) -> TempSlot {
        self.temporary
    }

    pub(super) fn adjustment(self) -> StackAdjustment {
        self.adjustment
    }
}

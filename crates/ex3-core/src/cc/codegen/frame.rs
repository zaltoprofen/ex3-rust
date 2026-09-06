use crate::cc::{
    ast::{BinOp, UnOp},
    sema::{
        LocalSlot, ParamIndex, ResolvedExpr, ResolvedExprKind, ResolvedFunction, ResolvedStmt,
        ResolvedSwitchPart,
    },
    ActiveTemporaryDebugInfo, DynamicStackSlotDebugInfo, DynamicStackSlotKind, EmitDebugContext,
    FixedFrameState, FunctionDebugId, FunctionDebugSymbols, FunctionFrameDebugInfo,
    LocalSlotDebugInfo, ParameterSlotDebugInfo, ReturnAddressSlotDebugInfo, ScalarType,
    TemporaryRole,
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

    pub(super) fn local_count(self) -> usize {
        self.local_count
    }

    pub(super) fn temporary_count(self) -> usize {
        self.temporary_count
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

    pub(super) fn return_address_offset(self, adjustment: StackAdjustment) -> StackOffset {
        StackOffset(self.size() + adjustment.0)
    }

    pub(super) fn debug_info(
        self,
        symbols: &FunctionDebugSymbols,
    ) -> Result<FunctionFrameDebugInfo, &'static str> {
        if symbols.locals.len() != self.local_count()
            || symbols.parameters.len() != self.parameter_count
            || symbols
                .locals
                .iter()
                .any(|local| local.slot >= self.local_count())
            || symbols
                .parameters
                .iter()
                .any(|parameter| parameter.index >= self.parameter_count)
        {
            return Err("debug symbols do not match the planned frame");
        }
        let root = StackAdjustment(0);
        Ok(FunctionFrameDebugInfo {
            function_id: symbols.id,
            name: symbols.name.clone(),
            frame_size: to_u16(self.size())?,
            parameters: symbols
                .parameters
                .iter()
                .map(|parameter| {
                    Ok(ParameterSlotDebugInfo {
                        index: to_u16(parameter.index)?,
                        name: parameter.name.clone(),
                        ty: parameter.ty,
                        frame_offset: to_i32(
                            self.parameter_offset(ParamIndex(parameter.index), root).0,
                        )?,
                    })
                })
                .collect::<Result<_, &'static str>>()?,
            return_address: ReturnAddressSlotDebugInfo {
                frame_offset: to_i32(self.return_address_offset(root).0)?,
            },
            locals: symbols
                .locals
                .iter()
                .map(|local| {
                    Ok(LocalSlotDebugInfo {
                        slot: to_u16(local.slot)?,
                        name: local.name.clone(),
                        ty: local.ty,
                        frame_offset: to_i32(self.local_offset(LocalSlot(local.slot), root).0)?,
                    })
                })
                .collect::<Result<_, &'static str>>()?,
            temporary_count: to_u16(self.temporary_count())?,
        })
    }
}

fn to_u16(value: usize) -> Result<u16, &'static str> {
    u16::try_from(value).map_err(|_| "debug frame value exceeds u16")
}

fn to_i32(value: usize) -> Result<i32, &'static str> {
    i32::try_from(value).map_err(|_| "debug frame offset exceeds i32")
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

#[derive(Clone)]
pub(super) struct EvalContext {
    temporary: TempSlot,
    adjustment: StackAdjustment,
    active_temporaries: Vec<ActiveTemporaryDebugInfo>,
    dynamic_stack_slots: Vec<DynamicStackSlotDebugInfo>,
}

impl EvalContext {
    pub(super) fn root() -> Self {
        Self {
            temporary: TempSlot(0),
            adjustment: StackAdjustment(0),
            active_temporaries: Vec::new(),
            dynamic_stack_slots: Vec::new(),
        }
    }

    pub(super) fn next_temp(mut self) -> Self {
        self.temporary = TempSlot(self.temporary.0 + 1);
        self
    }

    pub(super) fn with_active_temporary(
        mut self,
        role: TemporaryRole,
        display_name: String,
    ) -> Self {
        self.active_temporaries.push(ActiveTemporaryDebugInfo {
            slot: u16::try_from(self.temporary.0).expect("temporary slot exceeds u16"),
            role,
            display_name,
        });
        self
    }

    pub(super) fn with_dynamic_slot(
        mut self,
        kind: DynamicStackSlotKind,
        display_name: String,
        ty: Option<ScalarType>,
    ) -> Self {
        self.adjustment = StackAdjustment(self.adjustment.0 + 1);
        self.dynamic_stack_slots.push(DynamicStackSlotDebugInfo {
            frame_offset: -i32::try_from(self.adjustment.0)
                .expect("dynamic stack offset exceeds i32"),
            kind,
            display_name,
            ty,
        });
        self
    }

    pub(super) fn emit_debug_context(&self, function_id: FunctionDebugId) -> EmitDebugContext {
        EmitDebugContext {
            function_id,
            // A PUSH decrements SP, so the canonical frame SP is this many
            // words above the current SP until the dynamic slots are cleaned up.
            frame_base_delta: i32::try_from(self.adjustment.0)
                .expect("stack adjustment exceeds i32"),
            fixed_frame_state: FixedFrameState::Allocated,
            active_temporaries: self.active_temporaries.clone(),
            dynamic_stack_slots: self.dynamic_stack_slots.clone(),
        }
    }

    pub(super) fn temporary(&self) -> TempSlot {
        self.temporary
    }

    pub(super) fn adjustment(&self) -> StackAdjustment {
        self.adjustment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::{
        FunctionDebugId, FunctionDebugSymbols, LocalDebugSymbol, ParameterDebugSymbol, ScalarType,
    };

    #[test]
    fn debug_offsets_are_derived_from_frame_layout_operations() {
        let layout = FrameLayout {
            local_count: 2,
            temporary_count: 3,
            parameter_count: 2,
        };
        let symbols = FunctionDebugSymbols {
            id: FunctionDebugId::new(7),
            name: "f".into(),
            parameters: vec![
                ParameterDebugSymbol {
                    index: 0,
                    name: "a".into(),
                    ty: ScalarType::Int32,
                },
                ParameterDebugSymbol {
                    index: 1,
                    name: "b".into(),
                    ty: ScalarType::UInt32,
                },
            ],
            locals: vec![
                LocalDebugSymbol {
                    slot: 0,
                    name: "x".into(),
                    ty: ScalarType::Int32,
                },
                LocalDebugSymbol {
                    slot: 1,
                    name: "y".into(),
                    ty: ScalarType::UInt32,
                },
            ],
        };
        let debug = layout.debug_info(&symbols).unwrap();
        let root = StackAdjustment(0);

        assert_eq!(layout.local_count(), 2);
        assert_eq!(layout.temporary_count(), 3);
        assert_eq!(debug.frame_size, layout.size() as u16);
        assert_eq!(
            debug.locals[1].frame_offset,
            layout.local_offset(LocalSlot(1), root).0 as i32
        );
        assert_eq!(
            layout.temporary_offset(TempSlot(0), root).0,
            layout.local_count()
        );
        assert_eq!(
            debug.return_address.frame_offset,
            layout.return_address_offset(root).0 as i32
        );
        assert_eq!(
            debug.parameters[1].frame_offset,
            layout.parameter_offset(ParamIndex(1), root).0 as i32
        );
    }

    #[test]
    fn debug_frame_conversions_are_checked() {
        let empty_symbols = FunctionDebugSymbols {
            id: FunctionDebugId::new(0),
            name: "large".into(),
            parameters: Vec::new(),
            locals: Vec::new(),
        };
        let largest_valid_layout = FrameLayout {
            local_count: 0,
            temporary_count: i16::MAX as usize,
            parameter_count: 0,
        };

        let debug = largest_valid_layout.debug_info(&empty_symbols).unwrap();
        assert_eq!(debug.frame_size, i16::MAX as u16);
        assert_eq!(debug.return_address.frame_offset, i32::from(i16::MAX));
        assert!(to_u16(usize::MAX).is_err());
        assert!(to_i32(usize::MAX).is_err());
    }
}

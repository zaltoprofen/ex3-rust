use crate::cc::sema::{LocalSlot, ParamIndex, StackAdjustment, StackOffset, TempSlot};
use std::fmt;

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
    pub(super) fn new(local_count: usize, temporary_count: usize, parameter_count: usize) -> Self {
        Self {
            local_count,
            temporary_count,
            parameter_count,
        }
    }

    pub(super) fn size(self) -> usize {
        self.local_count + self.temporary_count
    }

    pub(super) fn local_offset(self, slot: LocalSlot, adjustment: StackAdjustment) -> StackOffset {
        StackOffset(slot.0 + adjustment.0)
    }

    pub(super) fn temporary_offset(
        self,
        slot: TempSlot,
        adjustment: StackAdjustment,
    ) -> StackOffset {
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

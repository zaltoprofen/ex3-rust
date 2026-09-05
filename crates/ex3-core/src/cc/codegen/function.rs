use super::{
    emitter::{BranchCondition, Emitter, Label, LabelFactory, LabelKind},
    frame::{EvalContext, FrameLayout, StackAdjustment, StackOffset},
};
use crate::cc::sema::{ResolvedFunction, ResolvedVariable};

pub(super) struct GeneratedFunction {
    pub assembly: String,
    pub uses_runtime: bool,
}

pub(super) struct FunctionGenerator<'a> {
    pub(super) function: &'a ResolvedFunction,
    pub(super) emitter: Emitter,
    pub(super) labels: &'a mut LabelFactory,
    pub(super) frame: FrameLayout,
    pub(super) return_label: Label,
    pub(super) breaks: Vec<Label>,
    pub(super) continues: Vec<Label>,
    pub(super) uses_runtime: bool,
}

impl<'a> FunctionGenerator<'a> {
    pub(super) fn new(
        function: &'a ResolvedFunction,
        frame: FrameLayout,
        labels: &'a mut LabelFactory,
    ) -> Self {
        let return_label = labels.fresh(LabelKind::Return);
        let mut emitter = Emitter::default();
        emitter.symbol_label(&function.name);
        emitter.adjust_sp(-(frame.size() as isize));
        Self {
            function,
            emitter,
            labels,
            frame,
            return_label,
            breaks: Vec::new(),
            continues: Vec::new(),
            uses_runtime: false,
        }
    }

    pub(super) fn generate(mut self) -> GeneratedFunction {
        self.generate_statement(&self.function.body);
        self.emitter.label(self.return_label);
        self.emitter.adjust_sp(self.frame.size() as isize);
        self.emitter.ret();
        GeneratedFunction {
            assembly: self.emitter.finish(),
            uses_runtime: self.uses_runtime,
        }
    }

    pub(super) fn fresh_label(&mut self, kind: LabelKind) -> Label {
        self.labels.fresh(kind)
    }

    pub(super) fn temporary_offset(&self, context: EvalContext) -> StackOffset {
        self.frame
            .temporary_offset(context.temporary(), context.adjustment())
    }

    pub(super) fn load_variable(
        &mut self,
        variable: &ResolvedVariable,
        adjustment: StackAdjustment,
    ) {
        match variable {
            ResolvedVariable::Local(slot) => self
                .emitter
                .load_sp(self.frame.local_offset(*slot, adjustment)),
            ResolvedVariable::Parameter(index) => self
                .emitter
                .load_sp(self.frame.parameter_offset(*index, adjustment)),
            ResolvedVariable::Global(symbol) => self.emitter.load_global(&symbol.0),
        }
    }

    pub(super) fn store_variable(
        &mut self,
        variable: &ResolvedVariable,
        adjustment: StackAdjustment,
    ) {
        match variable {
            ResolvedVariable::Local(slot) => self
                .emitter
                .store_sp(self.frame.local_offset(*slot, adjustment)),
            ResolvedVariable::Parameter(index) => self
                .emitter
                .store_sp(self.frame.parameter_offset(*index, adjustment)),
            ResolvedVariable::Global(symbol) => self.emitter.store_global(&symbol.0),
        }
    }

    pub(super) fn load_constant(&mut self, value: u32) {
        let signed = value as i32;
        if (-32768..=32767).contains(&signed) {
            self.emitter.load_immediate(signed);
        } else {
            self.emitter.clear();
            self.emitter.load_high((value >> 16) as u16);
            self.emitter.load_low((value & 0xffff) as u16);
        }
    }

    pub(super) fn emit_boolean_branch(&mut self, branch: BranchCondition) {
        let yes = self.fresh_label(LabelKind::True);
        let end = self.fresh_label(LabelKind::BoolEnd);
        self.emitter.branch(branch, yes);
        self.emitter.load_immediate(0);
        self.emitter.jump(end);
        self.emitter.label(yes);
        self.emitter.load_immediate(1);
        self.emitter.label(end);
    }
}

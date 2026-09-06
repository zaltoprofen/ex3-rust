use super::{
    builtins,
    emitter::{BranchCondition, Emitter, Label, LabelFactory, LabelKind},
    frame::{EvalContext, FrameLayout, StackAdjustment, StackOffset},
};
use crate::cc::{
    ast::{BinOp, UnOp},
    sema::{ResolvedCallee, ResolvedExpr, ResolvedExprKind, ResolvedFunction, ResolvedVariable},
    EmitDebugContext, FixedFrameState, FunctionDebugSymbols, GeneratedAssembly,
};

pub(super) struct GeneratedFunction {
    pub assembly: GeneratedAssembly,
    pub uses_runtime: bool,
}

pub(super) struct FunctionGenerator<'a> {
    pub(super) function: &'a ResolvedFunction,
    pub(super) debug_symbols: &'a FunctionDebugSymbols,
    pub(super) all_debug_symbols: &'a [FunctionDebugSymbols],
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
        debug_symbols: &'a FunctionDebugSymbols,
        all_debug_symbols: &'a [FunctionDebugSymbols],
        labels: &'a mut LabelFactory,
    ) -> Self {
        let return_label = labels.fresh(LabelKind::Return);
        let mut emitter = Emitter::default();
        emitter.symbol_label(&function.name);
        emitter.set_debug_context(EmitDebugContext {
            function_id: debug_symbols.id,
            frame_base_delta: -i32::try_from(frame.size()).expect("frame size exceeds i32"),
            fixed_frame_state: if frame.size() == 0 {
                FixedFrameState::Allocated
            } else {
                FixedFrameState::NotAllocated
            },
            active_temporaries: Vec::new(),
            dynamic_stack_slots: Vec::new(),
        });
        emitter.adjust_sp(-(frame.size() as isize));
        Self {
            function,
            debug_symbols,
            all_debug_symbols,
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
        self.sync_debug(&EvalContext::root());
        self.generate_statement(&self.function.body);
        self.emitter.label(self.return_label);
        self.sync_debug(&EvalContext::root());
        self.emitter.adjust_sp(self.frame.size() as isize);
        self.emitter.set_debug_context(EmitDebugContext {
            function_id: self.debug_symbols.id,
            frame_base_delta: -i32::try_from(self.frame.size()).expect("frame size exceeds i32"),
            fixed_frame_state: if self.frame.size() == 0 {
                FixedFrameState::Allocated
            } else {
                FixedFrameState::Released
            },
            active_temporaries: Vec::new(),
            dynamic_stack_slots: Vec::new(),
        });
        self.emitter.ret();
        GeneratedFunction {
            assembly: self.emitter.finish(),
            uses_runtime: self.uses_runtime,
        }
    }

    pub(super) fn fresh_label(&mut self, kind: LabelKind) -> Label {
        self.labels.fresh(kind)
    }

    pub(super) fn sync_debug(&mut self, context: &EvalContext) {
        self.emitter
            .set_debug_context(context.emit_debug_context(self.debug_symbols.id));
    }

    pub(super) fn temporary_offset(&self, context: &EvalContext) -> StackOffset {
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

    pub(super) fn describe_expression(&self, expression: &ResolvedExpr) -> String {
        match &expression.kind {
            ResolvedExprKind::Number(value) => value.to_string(),
            ResolvedExprKind::Load(variable) => self.variable_name(variable),
            ResolvedExprKind::Assign { target, value } => {
                format!(
                    "({} = {})",
                    self.variable_name(target),
                    self.describe_expression(value)
                )
            }
            ResolvedExprKind::Call { callee, args } => {
                let name = match callee {
                    ResolvedCallee::User(name) => name.as_str(),
                    ResolvedCallee::Builtin(id) => builtins::lookup(*id).assembly_name,
                };
                let arguments = args
                    .iter()
                    .map(|argument| self.describe_expression(argument))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}({arguments})")
            }
            ResolvedExprKind::Unary { op, operand } => {
                format!(
                    "({}{})",
                    unary_symbol(*op),
                    self.describe_expression(operand)
                )
            }
            ResolvedExprKind::Binary { op, lhs, rhs, .. } => format!(
                "({} {} {})",
                self.describe_expression(lhs),
                binary_symbol(*op),
                self.describe_expression(rhs)
            ),
        }
    }

    fn variable_name(&self, variable: &ResolvedVariable) -> String {
        match variable {
            ResolvedVariable::Local(slot) => self
                .debug_symbols
                .locals
                .iter()
                .find(|local| local.slot == slot.0)
                .map(|local| local.name.clone())
                .unwrap_or_else(|| format!("local #{}", slot.0)),
            ResolvedVariable::Parameter(index) => self
                .debug_symbols
                .parameters
                .iter()
                .find(|parameter| parameter.index == index.0)
                .map(|parameter| parameter.name.clone())
                .unwrap_or_else(|| format!("parameter #{}", index.0)),
            ResolvedVariable::Global(symbol) => symbol.0.clone(),
        }
    }
}

fn unary_symbol(operator: UnOp) -> &'static str {
    match operator {
        UnOp::BitNot => "~",
        UnOp::Not => "!",
        UnOp::Plus => "+",
        UnOp::Neg => "-",
    }
}

fn binary_symbol(operator: BinOp) -> &'static str {
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

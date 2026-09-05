use super::{
    emitter::{BranchCondition, Label, LabelKind},
    frame::{EvalContext, StackAdjustment, StackOffset, TempSlot},
    function::FunctionGenerator,
};
use crate::cc::sema::{ResolvedExpr, ResolvedStmt, ResolvedSwitchPart};

impl FunctionGenerator<'_> {
    pub(super) fn generate_statement(&mut self, statement: &ResolvedStmt) {
        match statement {
            ResolvedStmt::Empty => {}
            ResolvedStmt::Expr(expression) => {
                self.generate_expression(expression, EvalContext::root())
            }
            ResolvedStmt::Block(statements) => self.generate_block(statements),
            ResolvedStmt::Decl { slot, init } => self.generate_declaration(*slot, init.as_ref()),
            ResolvedStmt::If {
                condition,
                then_stmt,
                else_stmt,
            } => self.generate_if(condition, then_stmt, else_stmt.as_deref()),
            ResolvedStmt::While { condition, body } => self.generate_while(condition, body),
            ResolvedStmt::Switch { expression, parts } => self.generate_switch(expression, parts),
            ResolvedStmt::Break => self.emitter.jump(*self.breaks.last().unwrap()),
            ResolvedStmt::Continue => self.emitter.jump(*self.continues.last().unwrap()),
            ResolvedStmt::Goto(label) => self.emitter.jump_user(&self.function.name, label),
            ResolvedStmt::Label(label, body) => {
                self.emitter.user_label(&self.function.name, label);
                self.generate_statement(body);
            }
            ResolvedStmt::Return(expression) => self.generate_return(expression.as_ref()),
        }
    }

    fn generate_block(&mut self, statements: &[ResolvedStmt]) {
        for statement in statements {
            self.generate_statement(statement);
        }
    }

    fn generate_declaration(
        &mut self,
        slot: crate::cc::sema::LocalSlot,
        initializer: Option<&ResolvedExpr>,
    ) {
        if let Some(initializer) = initializer {
            self.generate_expression(initializer, EvalContext::root());
            self.emitter
                .store_sp(self.frame.local_offset(slot, StackAdjustment(0)));
        }
    }

    fn generate_if(
        &mut self,
        condition: &ResolvedExpr,
        then_stmt: &ResolvedStmt,
        else_stmt: Option<&ResolvedStmt>,
    ) {
        let else_label = self.fresh_label(LabelKind::Else);
        let end = self.fresh_label(LabelKind::IfEnd);
        self.generate_expression(condition, EvalContext::root());
        self.emitter.compare_zero();
        self.emitter.branch(BranchCondition::Equal, else_label);
        self.generate_statement(then_stmt);
        self.emitter.jump(end);
        self.emitter.label(else_label);
        if let Some(statement) = else_stmt {
            self.generate_statement(statement);
        }
        self.emitter.label(end);
    }

    fn generate_while(&mut self, condition: &ResolvedExpr, body: &ResolvedStmt) {
        let top = self.fresh_label(LabelKind::While);
        let end = self.fresh_label(LabelKind::WhileEnd);
        self.emitter.label(top);
        self.generate_expression(condition, EvalContext::root());
        self.emitter.compare_zero();
        self.emitter.branch(BranchCondition::Equal, end);
        self.breaks.push(end);
        self.continues.push(top);
        self.generate_statement(body);
        self.continues.pop();
        self.breaks.pop();
        self.emitter.jump(top);
        self.emitter.label(end);
    }

    fn generate_return(&mut self, expression: Option<&ResolvedExpr>) {
        if let Some(expression) = expression {
            self.generate_expression(expression, EvalContext::root());
        }
        self.emitter.jump(self.return_label);
    }

    fn generate_switch(&mut self, expression: &ResolvedExpr, parts: &[ResolvedSwitchPart]) {
        self.generate_expression(expression, EvalContext::root());
        let slot = self.frame.temporary_offset(TempSlot(0), StackAdjustment(0));
        self.emitter.store_sp(slot);
        let end = self.fresh_label(LabelKind::SwitchEnd);
        let mut labels = Vec::with_capacity(parts.len());
        let mut default = None;
        for part in parts {
            let label = match part {
                ResolvedSwitchPart::Case(_) => Some(self.fresh_label(LabelKind::Case)),
                ResolvedSwitchPart::Default => {
                    let label = self.fresh_label(LabelKind::Default);
                    default = Some(label);
                    Some(label)
                }
                ResolvedSwitchPart::Stmt(_) => None,
            };
            labels.push(label);
        }
        self.emit_switch_dispatch(parts, &labels, slot, default.unwrap_or(end));
        self.breaks.push(end);
        self.emit_switch_body(parts, labels);
        self.breaks.pop();
        self.emitter.label(end);
    }

    fn emit_switch_dispatch(
        &mut self,
        parts: &[ResolvedSwitchPart],
        labels: &[Option<Label>],
        slot: StackOffset,
        fallback: Label,
    ) {
        for (part, label) in parts.iter().zip(labels) {
            if let ResolvedSwitchPart::Case(value) = part {
                self.load_constant(*value);
                self.emitter.compare_sp(slot);
                self.emitter.branch(BranchCondition::Equal, label.unwrap());
            }
        }
        self.emitter.jump(fallback);
    }

    fn emit_switch_body(&mut self, parts: &[ResolvedSwitchPart], labels: Vec<Option<Label>>) {
        for (part, label) in parts.iter().zip(labels) {
            if let Some(label) = label {
                self.emitter.label(label);
            }
            if let ResolvedSwitchPart::Stmt(statement) = part {
                self.generate_statement(statement);
            }
        }
    }
}

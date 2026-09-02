use super::{ResolvedExpr, ResolvedExprKind, ResolvedStmt, ResolvedSwitchPart, UserLabel};
use crate::cc::ast::{BinOp, ScalarType, UnOp};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NodeId(usize);

/// Returns whether control can reach the synthetic node after the function body.
/// Return statements deliberately have no successor.
pub(super) fn may_reach_function_end(statement: &ResolvedStmt) -> bool {
    let mut builder = CfgBuilder::new();
    let entry = builder.statement(statement, builder.exit, None, None);
    builder.resolve_gotos();
    builder.reachable(entry).contains(&builder.exit)
}

struct CfgBuilder {
    edges: Vec<Vec<NodeId>>,
    exit: NodeId,
    labels: HashMap<UserLabel, NodeId>,
    gotos: Vec<(NodeId, UserLabel)>,
}

impl CfgBuilder {
    fn new() -> Self {
        Self {
            edges: vec![Vec::new()],
            exit: NodeId(0),
            labels: HashMap::new(),
            gotos: Vec::new(),
        }
    }

    fn node(&mut self) -> NodeId {
        let id = NodeId(self.edges.len());
        self.edges.push(Vec::new());
        id
    }

    fn node_with_edges(&mut self, successors: impl IntoIterator<Item = NodeId>) -> NodeId {
        let node = self.node();
        self.edges[node.0].extend(successors);
        node
    }

    fn statement(
        &mut self,
        statement: &ResolvedStmt,
        next: NodeId,
        break_target: Option<NodeId>,
        continue_target: Option<NodeId>,
    ) -> NodeId {
        match statement {
            ResolvedStmt::Empty | ResolvedStmt::Expr(_) | ResolvedStmt::Decl { .. } => {
                self.node_with_edges([next])
            }
            ResolvedStmt::Return(_) => self.node(),
            ResolvedStmt::Goto(label) => {
                let node = self.node();
                self.gotos.push((node, *label));
                node
            }
            ResolvedStmt::Break => self.node_with_edges([
                break_target.expect("break target was validated during resolution")
            ]),
            ResolvedStmt::Continue => self.node_with_edges([
                continue_target.expect("continue target was validated during resolution")
            ]),
            ResolvedStmt::Label(label, body) => {
                let entry = self.statement(body, next, break_target, continue_target);
                self.labels.insert(*label, entry);
                entry
            }
            ResolvedStmt::Block(statements) => {
                statements
                    .iter()
                    .rev()
                    .fold(next, |continuation, statement| {
                        self.statement(statement, continuation, break_target, continue_target)
                    })
            }
            ResolvedStmt::If {
                condition: _,
                then_stmt,
                else_stmt,
            } => {
                let then_entry = self.statement(then_stmt, next, break_target, continue_target);
                let else_entry = else_stmt.as_deref().map_or(next, |branch| {
                    self.statement(branch, next, break_target, continue_target)
                });
                self.node_with_edges([then_entry, else_entry])
            }
            ResolvedStmt::While { condition, body } => {
                let condition_node = self.node();
                let body_entry =
                    self.statement(body, condition_node, Some(next), Some(condition_node));
                match constant_value(condition) {
                    Some(0) => self.edges[condition_node.0].push(next),
                    Some(_) => self.edges[condition_node.0].push(body_entry),
                    None => self.edges[condition_node.0].extend([body_entry, next]),
                }
                condition_node
            }
            ResolvedStmt::Switch { parts, .. } => self.switch(parts, next, continue_target),
        }
    }

    fn switch(
        &mut self,
        parts: &[ResolvedSwitchPart],
        next: NodeId,
        continue_target: Option<NodeId>,
    ) -> NodeId {
        let mut continuation = next;
        let mut cases = Vec::new();
        let mut default = None;
        for part in parts.iter().rev() {
            match part {
                ResolvedSwitchPart::Stmt(statement) => {
                    continuation =
                        self.statement(statement, continuation, Some(next), continue_target);
                }
                ResolvedSwitchPart::Case(_) => cases.push(continuation),
                ResolvedSwitchPart::Default => default = Some(continuation),
            }
        }
        cases.reverse();
        if let Some(default) = default {
            cases.push(default);
        } else {
            cases.push(next);
        }
        self.node_with_edges(cases)
    }

    fn resolve_gotos(&mut self) {
        for (node, label) in &self.gotos {
            let target = self
                .labels
                .get(label)
                .copied()
                .expect("goto target was validated during resolution");
            self.edges[node.0].push(target);
        }
    }

    fn reachable(&self, entry: NodeId) -> HashSet<NodeId> {
        let mut reachable = HashSet::new();
        let mut work = vec![entry];
        while let Some(node) = work.pop() {
            if reachable.insert(node) {
                work.extend(self.edges[node.0].iter().copied());
            }
        }
        reachable
    }
}

fn constant_value(expression: &ResolvedExpr) -> Option<u32> {
    match &expression.kind {
        ResolvedExprKind::Number(value) => Some(*value),
        ResolvedExprKind::Unary { op, operand } => {
            let value = constant_value(operand)?;
            Some(match op {
                UnOp::BitNot => !value,
                UnOp::Not => (value == 0) as u32,
                UnOp::Plus => value,
                UnOp::Neg => 0u32.wrapping_sub(value),
            })
        }
        ResolvedExprKind::Binary {
            op,
            lhs,
            rhs,
            operand_type,
        } => {
            let lhs = constant_value(lhs)?;
            let rhs = constant_value(rhs)?;
            let unsigned = *operand_type == ScalarType::UInt32;
            Some(match op {
                BinOp::Add => lhs.wrapping_add(rhs),
                BinOp::Sub => lhs.wrapping_sub(rhs),
                BinOp::Mul => lhs.wrapping_mul(rhs),
                BinOp::Div | BinOp::Mod if rhs == 0 => return None,
                BinOp::Div if unsigned => lhs / rhs,
                BinOp::Div => (lhs as i32).wrapping_div(rhs as i32) as u32,
                BinOp::Mod if unsigned => lhs % rhs,
                BinOp::Mod => (lhs as i32).wrapping_rem(rhs as i32) as u32,
                BinOp::BitAnd => lhs & rhs,
                BinOp::BitXor => lhs ^ rhs,
                BinOp::BitOr => lhs | rhs,
                BinOp::Lt if unsigned => (lhs < rhs) as u32,
                BinOp::Lt => ((lhs as i32) < rhs as i32) as u32,
                BinOp::Le if unsigned => (lhs <= rhs) as u32,
                BinOp::Le => ((lhs as i32) <= rhs as i32) as u32,
                BinOp::Gt if unsigned => (lhs > rhs) as u32,
                BinOp::Gt => ((lhs as i32) > rhs as i32) as u32,
                BinOp::Ge if unsigned => (lhs >= rhs) as u32,
                BinOp::Ge => ((lhs as i32) >= rhs as i32) as u32,
                BinOp::Eq => (lhs == rhs) as u32,
                BinOp::Ne => (lhs != rhs) as u32,
                BinOp::And => (lhs != 0 && rhs != 0) as u32,
                BinOp::Or => (lhs != 0 || rhs != 0) as u32,
            })
        }
        ResolvedExprKind::Load(_)
        | ResolvedExprKind::Assign { .. }
        | ResolvedExprKind::Call { .. } => None,
    }
}

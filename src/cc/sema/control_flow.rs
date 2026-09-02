use super::const_eval;
use crate::cc::ast::{Stmt, StmtKind, SwitchPart};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NodeId(usize);

/// A function definitely returns when its synthetic exit node is unreachable
/// from the function entry. Return statements deliberately have no successor.
pub(super) fn definitely_returns(statement: &Stmt) -> bool {
    let mut builder = CfgBuilder::new();
    let entry = builder.statement(statement, builder.exit, None, None);
    builder.resolve_gotos();
    !builder.reachable(entry).contains(&builder.exit)
}

struct CfgBuilder {
    edges: Vec<Vec<NodeId>>,
    exit: NodeId,
    labels: HashMap<String, NodeId>,
    gotos: Vec<(NodeId, String)>,
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
        statement: &Stmt,
        next: NodeId,
        break_target: Option<NodeId>,
        continue_target: Option<NodeId>,
    ) -> NodeId {
        match &statement.kind {
            StmtKind::Empty | StmtKind::Expression { .. } | StmtKind::Declaration { .. } => {
                self.node_with_edges([next])
            }
            StmtKind::Return { .. } => self.node(),
            StmtKind::Goto { label } => {
                let node = self.node();
                self.gotos.push((node, label.clone()));
                node
            }
            StmtKind::Break => self.node_with_edges([break_target.unwrap_or(self.exit)]),
            StmtKind::Continue => self.node_with_edges([continue_target.unwrap_or(self.exit)]),
            StmtKind::Label { name, body } => {
                let entry = self.statement(body, next, break_target, continue_target);
                self.labels.insert(name.clone(), entry);
                entry
            }
            StmtKind::Block { statements } => {
                statements
                    .iter()
                    .rev()
                    .fold(next, |continuation, statement| {
                        self.statement(statement, continuation, break_target, continue_target)
                    })
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let then_entry = self.statement(then_branch, next, break_target, continue_target);
                let else_entry = else_branch.as_deref().map_or(next, |branch| {
                    self.statement(branch, next, break_target, continue_target)
                });
                let _ = condition;
                self.node_with_edges([then_entry, else_entry])
            }
            StmtKind::While { condition, body } => {
                let condition_node = self.node();
                let body_entry =
                    self.statement(body, condition_node, Some(next), Some(condition_node));
                match const_eval::evaluate(condition).map(|constant| constant.value) {
                    Ok(0) => self.edges[condition_node.0].push(next),
                    Ok(_) => self.edges[condition_node.0].push(body_entry),
                    Err(_) => self.edges[condition_node.0].extend([body_entry, next]),
                }
                condition_node
            }
            StmtKind::Switch { parts, .. } => self.switch(parts, next, continue_target),
        }
    }

    fn switch(
        &mut self,
        parts: &[SwitchPart],
        next: NodeId,
        continue_target: Option<NodeId>,
    ) -> NodeId {
        let mut continuation = next;
        let mut cases = Vec::new();
        let mut default = None;
        for part in parts.iter().rev() {
            match part {
                SwitchPart::Statement { statement } => {
                    continuation =
                        self.statement(statement, continuation, Some(next), continue_target);
                }
                SwitchPart::Case { .. } => cases.push(continuation),
                SwitchPart::Default { .. } => default = Some(continuation),
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
            let target = self.labels.get(label).copied().unwrap_or(self.exit);
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

use super::const_eval;
use crate::cc::ast::{Stmt, StmtKind, SwitchPart};

/// Returns true only when every path that reaches `statement` returns a value.
pub(super) fn definitely_returns(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return { .. } => true,
        StmtKind::Block { statements } => statements.iter().any(definitely_returns),
        StmtKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => definitely_returns(then_branch) && definitely_returns(else_branch),
        StmtKind::Label { body, .. } => definitely_returns(body),
        StmtKind::Switch { parts, .. } => switch_definitely_returns(parts),
        StmtKind::While { condition, body }
            if const_eval::evaluate(condition).is_ok_and(|constant| constant.value != 0) =>
        {
            definitely_returns(body)
        }
        StmtKind::Empty
        | StmtKind::Expression { .. }
        | StmtKind::Declaration { .. }
        | StmtKind::If {
            else_branch: None, ..
        }
        | StmtKind::While { .. }
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Goto { .. } => false,
    }
}

fn switch_definitely_returns(parts: &[SwitchPart]) -> bool {
    let mut has_default = false;
    let mut suffix_returns = false;
    let mut every_entry_returns = true;

    for part in parts.iter().rev() {
        match part {
            SwitchPart::Statement { statement } => {
                if definitely_returns(statement) {
                    suffix_returns = true;
                } else if matches!(statement.kind, StmtKind::Break) {
                    suffix_returns = false;
                }
            }
            SwitchPart::Case { .. } => every_entry_returns &= suffix_returns,
            SwitchPart::Default { .. } => {
                has_default = true;
                every_entry_returns &= suffix_returns;
            }
        }
    }

    has_default && every_entry_returns
}

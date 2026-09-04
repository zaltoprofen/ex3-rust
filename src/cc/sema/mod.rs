use super::{ast::Program, CcError};

mod builtins;
mod const_eval;
mod control_flow;
mod ir;
mod resolver;
mod symbols;
pub(crate) use ir::*;

pub(crate) fn analyze(ast: Program) -> Result<AnalyzedProgram, Vec<CcError>> {
    let symbols = symbols::collect(&ast)?;
    resolver::resolve(ast, &symbols.globals, &symbols.functions).map_err(|error| vec![error])
}

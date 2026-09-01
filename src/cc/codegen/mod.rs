mod emitter;
mod expr;
mod frame;
mod function;
mod stmt;

use super::{sema::AnalyzedProgram, CcError};
use emitter::{Emitter, LabelFactory};
use function::FunctionGenerator;

const RUNTIME: &str = include_str!("../runtime.asm");

pub(crate) fn generate(program: &AnalyzedProgram) -> Result<String, Vec<CcError>> {
    let mut emitter = Emitter::default();
    emitter.generated_header();
    let mut labels = LabelFactory::default();
    let mut uses_runtime = false;
    for function in &program.functions {
        let generated = FunctionGenerator::new(function, &mut labels).generate(function);
        uses_runtime |= generated.uses_runtime;
        emitter.append_assembly(&generated.assembly);
    }
    if uses_runtime {
        emitter.append_assembly(RUNTIME);
    }
    for global in &program.globals {
        emitter.global(&global.name, global.value);
    }
    emitter.end();
    Ok(emitter.finish())
}

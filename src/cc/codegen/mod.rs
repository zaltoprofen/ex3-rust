mod builtins;
mod emitter;
mod expr;
mod frame;
mod function;
mod stmt;

use super::{sema::AnalyzedProgram, CcError};
use emitter::{Emitter, LabelFactory};
use frame::FrameLayout;
use function::FunctionGenerator;

const RUNTIME: &str = include_str!("../runtime.asm");

pub(crate) struct FramePlan {
    frames: Vec<FrameLayout>,
}

pub(crate) fn plan(program: &AnalyzedProgram) -> Result<FramePlan, Vec<CcError>> {
    let mut frames = Vec::with_capacity(program.functions.len());
    let mut errors = Vec::new();
    for function in &program.functions {
        let frame = FrameLayout::plan(function);
        if let Err(message) = frame.validate(function) {
            errors.push(CcError::new(function.span, message));
        }
        frames.push(frame);
    }
    if errors.is_empty() {
        Ok(FramePlan { frames })
    } else {
        Err(errors)
    }
}

pub(crate) fn generate(program: &AnalyzedProgram, plan: &FramePlan) -> String {
    let mut emitter = Emitter::default();
    emitter.generated_header();
    let mut labels = LabelFactory::default();
    let mut uses_runtime = false;
    for (function, frame) in program.functions.iter().zip(&plan.frames) {
        let generated = FunctionGenerator::new(function, *frame, &mut labels).generate();
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
    emitter.finish()
}

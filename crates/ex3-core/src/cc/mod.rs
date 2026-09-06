//! Compiler for the pointerless EX3 C v0.1 subset.
mod ast;
mod codegen;
mod debug;
mod diagnostic;
mod lexer;
mod parser;
mod sema;

pub use ast::ScalarType;
pub use debug::{
    ActiveTemporaryDebugInfo, AssemblyLineDebugInfo, Compilation, CompilerDebugInfo,
    DynamicStackSlotDebugInfo, DynamicStackSlotKind, EmitDebugContext, FixedFrameState,
    FunctionDebugId, FunctionDebugSymbols, FunctionFrameDebugInfo, GeneratedAssembly,
    LocalDebugSymbol, LocalSlotDebugInfo, ParameterDebugSymbol, ParameterSlotDebugInfo,
    ReturnAddressSlotDebugInfo, TemporaryRole,
};
pub use diagnostic::{CcError, CcErrors, Span};

pub(crate) fn is_implementation_reserved(name: &str) -> bool {
    name.starts_with("__cc_") || name.starts_with("__ex3_")
}

pub fn compile(source: &str) -> Result<String, CcErrors> {
    Ok(compile_with_debug_info(source)?.assembly)
}

pub fn compile_with_debug_info(source: &str) -> Result<Compilation, CcErrors> {
    let tokens = lexer::lex(source).map_err(CcErrors)?;
    let ast = parser::parse(tokens).map_err(CcErrors)?;
    let program = sema::analyze(ast).map_err(CcErrors)?;
    let plan = codegen::plan(&program).map_err(CcErrors)?;
    let generated = codegen::generate(&program, &plan);
    let frames = plan.into_debug_frames();
    let mut debug_info = program.debug_info;
    debug_info.frames = frames;
    debug_info.assembly_lines = generated.debug_lines;
    Ok(Compilation {
        assembly: generated.text,
        debug_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assembler::Assembler,
        emulator::{ArrayMemory, Cpu, DeterministicIoBus, IoKind, NullIoBus},
    };

    fn run(source: &str) -> u32 {
        let asm = compile(source).unwrap_or_else(|e| panic!("{e}"));
        let image = Assembler::new()
            .assemble(&asm)
            .unwrap_or_else(|e| panic!("{e}\n{asm}"));
        let mut cpu = Cpu::new();
        let mut memory = ArrayMemory::from_image(&image.image);
        cpu.run(&mut memory, &mut NullIoBus, 1_000_000).unwrap();
        assert!(cpu.state().halted);
        cpu.state().ac
    }

    fn diagnostic_messages(source: &str) -> Vec<String> {
        compile(source)
            .expect_err("source should be rejected")
            .0
            .into_iter()
            .map(|error| error.message)
            .collect()
    }

    fn assembly_line(compilation: &Compilation, line: u32) -> &str {
        compilation
            .assembly
            .lines()
            .nth(line as usize - 1)
            .expect("debug metadata referenced a missing assembly line")
            .trim()
    }

    fn states_for_instruction<'a>(
        compilation: &'a Compilation,
        instruction: &str,
    ) -> Vec<&'a AssemblyLineDebugInfo> {
        compilation
            .debug_info
            .assembly_lines
            .iter()
            .filter(|state| assembly_line(compilation, state.assembly_line) == instruction)
            .collect()
    }

    #[test]
    fn debug_symbols_preserve_parameter_local_names_types_and_shadowing() {
        let compilation = compile_with_debug_info(
            r#"
                int add(int lhs, unsigned int rhs) {
                    int value;
                    int result;
                    value = lhs;
                    {
                        unsigned int value;
                        value = rhs;
                    }
                    result = value;
                    return result;
                }
                int main(void) { return add(1, 2u); }
            "#,
        )
        .unwrap();
        let add = compilation
            .debug_info
            .functions
            .iter()
            .find(|function| function.name == "add")
            .unwrap();

        assert_eq!(add.id.index(), 0);
        assert_eq!(
            add.parameters,
            [
                ParameterDebugSymbol {
                    index: 0,
                    name: "lhs".into(),
                    ty: ScalarType::Int32,
                },
                ParameterDebugSymbol {
                    index: 1,
                    name: "rhs".into(),
                    ty: ScalarType::UInt32,
                },
            ]
        );
        assert_eq!(
            add.locals,
            [
                LocalDebugSymbol {
                    slot: 0,
                    name: "value".into(),
                    ty: ScalarType::Int32,
                },
                LocalDebugSymbol {
                    slot: 1,
                    name: "result".into(),
                    ty: ScalarType::Int32,
                },
                LocalDebugSymbol {
                    slot: 2,
                    name: "value".into(),
                    ty: ScalarType::UInt32,
                },
            ]
        );
    }

    #[test]
    fn debug_symbols_include_definitions_but_not_prototypes_or_builtins() {
        let compilation = compile_with_debug_info(
            r#"
                void putchar(int c);
                int identity(int value);
                int identity(int value) { return value; }
                int main(void) { putchar(65); return identity(7); }
            "#,
        )
        .unwrap();
        let functions = &compilation.debug_info.functions;

        assert_eq!(
            functions
                .iter()
                .map(|function| (function.id.index(), function.name.as_str()))
                .collect::<Vec<_>>(),
            [(0, "identity"), (1, "main")]
        );
    }

    #[test]
    fn compile_wrapper_matches_debug_compile() {
        for source in [
            "int main(void) { return 42; }",
            "int add(int a, int b) { return a + b; } int main(void) { return add(3, 4); }",
            "int main(void) { unsigned int value; value = 0xffffffffu; return value / 3u; }",
        ] {
            let assembly = compile(source).unwrap();
            let with_debug = compile_with_debug_info(source).unwrap();
            assert_eq!(assembly, with_debug.assembly, "source: {source}");

            let plain_image = Assembler::new().assemble(&assembly).unwrap().image;
            let debug_image = Assembler::new()
                .assemble(&with_debug.assembly)
                .unwrap()
                .image;
            assert_eq!(plain_image, debug_image, "source: {source}");
        }
    }

    #[test]
    fn debug_frame_layout_uses_codegen_slot_offsets() {
        let compilation = compile_with_debug_info(
            r#"
                int calculate(int lhs, unsigned int rhs) {
                    int value;
                    value = lhs + rhs;
                    return value;
                }
                int main(void) { return calculate(3, 4u); }
            "#,
        )
        .unwrap();
        let frame = compilation
            .debug_info
            .frames
            .iter()
            .find(|frame| frame.name == "calculate")
            .unwrap();

        assert_eq!(frame.function_id.index(), 0);
        assert_eq!(frame.frame_size, 3);
        assert_eq!(frame.temporary_count, 2);
        assert_eq!(frame.return_address.frame_offset, 3);
        assert_eq!(frame.locals[0].slot, 0);
        assert_eq!(frame.locals[0].frame_offset, 0);
        assert_eq!(frame.parameters[0].index, 0);
        assert_eq!(frame.parameters[0].frame_offset, 4);
        assert_eq!(frame.parameters[1].index, 1);
        assert_eq!(frame.parameters[1].frame_offset, 5);
    }

    #[test]
    fn empty_function_frame_still_describes_the_return_address() {
        let compilation = compile_with_debug_info("int main(void) { return 42; }").unwrap();
        let frame = &compilation.debug_info.frames[0];

        assert_eq!(frame.name, "main");
        assert_eq!(frame.frame_size, 0);
        assert_eq!(frame.temporary_count, 0);
        assert!(frame.parameters.is_empty());
        assert!(frame.locals.is_empty());
        assert_eq!(frame.return_address.frame_offset, 0);
    }

    #[test]
    fn assembly_debug_lines_cover_only_c_function_instructions() {
        let compilation = compile_with_debug_info(
            r#"
                int data = 7;
                int main(void) { return 2 * 3; }
            "#,
        )
        .unwrap();
        let debug_lines = &compilation.debug_info.assembly_lines;
        assert!(!debug_lines.is_empty());

        for state in debug_lines {
            let line = assembly_line(&compilation, state.assembly_line);
            assert!(!line.is_empty());
            assert!(!line.starts_with(';'));
            assert!(!line.starts_with("ORG "));
            assert!(!line.starts_with("HEX ") && !line.contains(": HEX "));
            assert!(!line.ends_with(':'));
            assert!(state.function_id.index() < compilation.debug_info.functions.len());
        }

        let mapped_lines = debug_lines
            .iter()
            .map(|state| state.assembly_line)
            .collect::<std::collections::HashSet<_>>();
        let source_lines = compilation.assembly.lines().collect::<Vec<_>>();
        let startup_call = source_lines
            .iter()
            .position(|line| line.trim() == "CALL main")
            .unwrap() as u32
            + 1;
        let runtime_label = source_lines
            .iter()
            .position(|line| line.trim() == "__ex3_mul_i32:")
            .unwrap() as u32
            + 1;
        assert!(!mapped_lines.contains(&startup_call));
        assert!(!mapped_lines.contains(&(startup_call + 1))); // startup HLT
        assert!(!mapped_lines.contains(&(runtime_label + 1)));
        assert_eq!(states_for_instruction(&compilation, "PUSH").len(), 2);
    }

    #[test]
    fn instruction_states_track_frame_deltas_and_outgoing_arguments() {
        let compilation = compile_with_debug_info(
            r#"
                int id(int value) { return value; }
                int sum3(int first, int second, int third) {
                    return first + second + third;
                }
                int main(void) { return sum3(id(1), 2, 3); }
            "#,
        )
        .unwrap();
        let sum = compilation
            .debug_info
            .functions
            .iter()
            .find(|function| function.name == "sum3")
            .unwrap();
        let sum_states = compilation
            .debug_info
            .assembly_lines
            .iter()
            .filter(|state| state.function_id == sum.id)
            .collect::<Vec<_>>();
        let prologue = sum_states
            .iter()
            .find(|state| assembly_line(&compilation, state.assembly_line) == "ADJSP -2")
            .unwrap();
        assert_eq!(prologue.frame_base_delta, -2);
        assert!(sum_states.iter().any(|state| state.frame_base_delta == 0));
        let epilogue = sum_states
            .iter()
            .find(|state| assembly_line(&compilation, state.assembly_line) == "ADJSP 2")
            .unwrap();
        assert_eq!(epilogue.frame_base_delta, 0);
        let ret = sum_states
            .iter()
            .find(|state| assembly_line(&compilation, state.assembly_line) == "RET")
            .unwrap();
        assert_eq!(ret.frame_base_delta, -2);

        let call_sum = states_for_instruction(&compilation, "CALL sum3")
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(call_sum.frame_base_delta, 3);
        assert_eq!(call_sum.dynamic_stack_slots.len(), 3);
        let outgoing = call_sum
            .dynamic_stack_slots
            .iter()
            .map(|slot| match &slot.kind {
                DynamicStackSlotKind::OutgoingArgument {
                    callee,
                    argument_index,
                    parameter_name,
                } => (
                    slot.frame_offset,
                    callee.as_str(),
                    *argument_index,
                    parameter_name.as_deref(),
                    slot.ty,
                ),
                kind => panic!("unexpected dynamic slot kind: {kind:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            outgoing,
            [
                (-1, "sum3", 2, Some("third"), Some(ScalarType::Int32)),
                (-2, "sum3", 1, Some("second"), Some(ScalarType::Int32)),
                (-3, "sum3", 0, Some("first"), Some(ScalarType::Int32)),
            ]
        );

        let call_id = states_for_instruction(&compilation, "CALL id")
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(call_id.frame_base_delta, 3);
        assert_eq!(call_id.dynamic_stack_slots.len(), 3);
        assert!(call_id.dynamic_stack_slots.iter().any(|slot| matches!(
            slot.kind,
            DynamicStackSlotKind::OutgoingArgument {
                argument_index: 0,
                ref parameter_name,
                ..
            } if parameter_name.as_deref() == Some("value")
        )));
        assert!(compilation
            .debug_info
            .assembly_lines
            .iter()
            .any(|state| { state.frame_base_delta == 1 && !state.dynamic_stack_slots.is_empty() }));
        assert!(compilation
            .debug_info
            .assembly_lines
            .iter()
            .any(|state| { state.frame_base_delta == 2 && state.dynamic_stack_slots.len() == 2 }));
    }

    #[test]
    fn instruction_states_track_runtime_arguments_and_temporary_roles() {
        let compilation = compile_with_debug_info(
            r#"
                int main(void) {
                    int value;
                    value = -1;
                    switch (value + 2) {
                    case 1: return (value == -1) * (3 + 4);
                    default: return 0;
                    }
                }
            "#,
        )
        .unwrap();
        let runtime_call = states_for_instruction(&compilation, "CALL __ex3_mul_i32")
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(runtime_call.frame_base_delta, 2);
        assert_eq!(runtime_call.dynamic_stack_slots.len(), 2);
        assert_eq!(
            runtime_call
                .dynamic_stack_slots
                .iter()
                .map(|slot| (slot.frame_offset, &slot.kind))
                .collect::<Vec<_>>(),
            [
                (
                    -1,
                    &DynamicStackSlotKind::RuntimeArgument {
                        helper: "__ex3_mul_i32".into(),
                        argument_index: 1,
                    }
                ),
                (
                    -2,
                    &DynamicStackSlotKind::RuntimeArgument {
                        helper: "__ex3_mul_i32".into(),
                        argument_index: 0,
                    }
                ),
            ]
        );

        let active = compilation
            .debug_info
            .assembly_lines
            .iter()
            .flat_map(|state| &state.active_temporaries)
            .collect::<Vec<_>>();
        for role in [
            TemporaryRole::UnaryOperand,
            TemporaryRole::BinaryLeft,
            TemporaryRole::BinaryRight,
            TemporaryRole::ComparisonLeft,
            TemporaryRole::ComparisonRight,
            TemporaryRole::SwitchValue,
        ] {
            assert!(active.iter().any(|temporary| temporary.role == role));
        }
        let slot_zero_descriptions = active
            .iter()
            .filter(|temporary| temporary.slot == 0)
            .map(|temporary| temporary.display_name.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(slot_zero_descriptions.len() > 1);
        assert!(compilation
            .debug_info
            .assembly_lines
            .iter()
            .any(|state| state.active_temporaries.is_empty()));
    }

    #[test]
    fn functions_locals_globals_and_arithmetic_runtime() {
        assert_eq!(
            run(r#"
            int bias = 2;
            int fact(int n) {
                int result;
                result = 1;
                while (n > 1) { result = result * n; n = n - 1; }
                return result;
            }
            int main(void) { return fact(5) + bias; }
        "#),
            122
        );
    }

    #[test]
    fn recursion_preserves_stack_frames() {
        assert_eq!(
            run(r#"
                int fact(int n) {
                    if (n <= 1) return 1;
                    return n * fact(n - 1);
                }
                int main(void) { return fact(6); }
            "#),
            720
        );
    }

    #[test]
    fn unary_negation_reserves_temporary_storage() {
        assert_eq!(run("int main(void) { return -1; }"), u32::MAX);
        assert_eq!(run("int main(void) { return -(-(-3)); }"), (-3i32) as u32);
    }

    #[test]
    fn unary_negation_inside_call_argument_preserves_stack() {
        assert_eq!(
            run(r#"
                int id(int x) { return x; }
                int main(void) { return id(-7); }
            "#),
            (-7i32) as u32
        );
    }

    #[test]
    fn deeply_nested_expression_uses_the_planned_temporary_slots() {
        assert_eq!(
            run("int main(void) { return 1 + (2 + (3 + (4 + (5 + 6)))); }"),
            21
        );
    }

    #[test]
    fn oversized_backend_frame_produces_a_compiler_diagnostic() {
        let parameters = (0..=i16::MAX as usize)
            .map(|index| format!("int p{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let source =
            format!("int oversized({parameters}) {{ return 0; }} int main(void) {{ return 0; }}");
        let messages = diagnostic_messages(&source);
        assert!(
            messages
                .iter()
                .any(|message| message == "function stack frame is too large"),
            "diagnostics: {messages:?}"
        );
    }

    #[test]
    fn signed_division_modulo_and_nested_calls() {
        assert_eq!(
            run(r#"
            int add(int a, int b) { return a + b; }
            int main(void) { return add(-7 / 3, add(-7 % 3, 10)); }
        "#),
            7
        );
    }

    #[test]
    fn full_width_unsigned_runtime_is_bounded() {
        assert_eq!(
            run("int main(void) { return 0xffffffffU / 3u; }"),
            0x5555_5555
        );
        assert_eq!(run("int main(void) { return 0xffffffffU % 65537u; }"), 0);
        assert_eq!(
            run("int main(void) { return 0xffffffffU * 3u; }"),
            0xffff_fffd
        );
    }

    #[test]
    fn switch_goto_unsigned_and_short_circuit() {
        assert_eq!(
            run(r#"
            int side;
            int main(void) {
                unsigned int x;
                x = 0xffffffffU;
                if (0 && (side = 9)) side = 20;
                if (1 || (side = 8)) side = side + 1;
                switch (x) {
                case 1: side = 30; break;
                case 0xffffffffU: goto done;
                default: side = 40;
                }
            done:
                return side + (x > 1u);
            }
        "#),
            2
        );
    }

    #[test]
    fn diagnostics_missing_return_and_bad_break() {
        assert!(compile("int main(void) { int x; x = 1; }").is_err());
        assert!(compile("int main(void) { if (1) return 1; }").is_err());
        assert!(compile("int main(void) { if (1) return 1; else return 2; }").is_ok());
        assert!(compile("int main(void) { while (1) { return 1; } }").is_ok());
        assert!(compile("int main(void) { switch (1) { case 1: return 1; } }").is_err());
        assert!(
            compile("int main(void) { switch (1) { case 1: return 1; default: return 2; } }")
                .is_ok()
        );
        assert!(compile("int main(void) { goto end; return 1; end: ; }").is_err());
        assert!(compile(
            "int main(void) { int x; switch (x) { case 1: goto end; default: return 1; } end: ; }"
        )
        .is_err());
        assert!(compile("int main(void) { goto end; end: return 1; }").is_ok());
        assert!(compile("int main(void) { break; return 0; }").is_err());
        assert!(compile("int main(void) { unsigned x; return 0; }").is_err());
        assert!(compile("void putchar(int c); int main(void) { putchar(65); return 0; }").is_ok());
        assert!(compile("void f(void) int x; int main(void) { return 0; }").is_err());
    }

    #[test]
    fn invalid_control_flow_is_reported_before_fallthrough_analysis() {
        for (source, expected) in [
            (
                "int main(void) { goto missing; }",
                "undefined label `missing`",
            ),
            (
                "int main(void) { break; }",
                "`break` is not inside while or switch",
            ),
            (
                "int main(void) { continue; }",
                "`continue` is not inside while",
            ),
        ] {
            let messages = diagnostic_messages(source);
            assert_eq!(messages, [expected], "source: {source}");
        }
    }

    #[test]
    fn constant_infinite_loop_cannot_reach_function_end() {
        assert!(compile("int main(void) { while (1 + 1) { } }").is_ok());
    }

    #[test]
    fn scope_and_label_diagnostics_preserve_resolver_invariants() {
        for (source, expected) in [
            (
                "int f(int x, int x) { return x; } int main(void) { return 0; }",
                "duplicate parameter `x`",
            ),
            (
                "int main(void) { int x; int x; return 0; }",
                "redeclaration of `x`",
            ),
            (
                "int main(void) { here: ; here: return 0; }",
                "duplicate label `here`",
            ),
            (
                "int f(void) return 1; int main(void) { return 0; }",
                "function body must be a compound statement",
            ),
        ] {
            let messages = diagnostic_messages(source);
            assert!(
                messages.iter().any(|message| message == expected),
                "source: {source}; diagnostics: {messages:?}"
            );
        }

        assert_eq!(
            run(r#"
                int main(void) {
                    int value;
                    value = 1;
                    { int value; value = 2; }
                    return value;
                }
            "#),
            1
        );
    }

    #[test]
    fn serial_builtins_follow_the_runtime_contract() {
        let source = r#"
            void putchar(int c);
            int getchar(void);
            int main(void) {
                int c;
                c = getchar();
                putchar(c);
                return c;
            }
        "#;
        let asm = compile(source).unwrap();
        let image = Assembler::new().assemble(&asm).unwrap();
        let mut cpu = Cpu::new();
        let mut memory = ArrayMemory::from_image(&image.image);
        let mut io = DeterministicIoBus::default();
        io.push_input(IoKind::Serial, 0xa5);
        cpu.run(&mut memory, &mut io, 10_000).unwrap();
        assert_eq!(cpu.state().ac, 0xa5);
        assert_eq!(io.output(IoKind::Serial), &[0xa5]);
    }

    #[test]
    fn nested_switch_and_while_target_the_innermost_control_flow() {
        assert_eq!(
            run(r#"
                int main(void) {
                    int i;
                    int sum;
                    i = 0;
                    sum = 0;
                    while (i < 4) {
                        i = i + 1;
                        switch (i) {
                        case 1: continue;
                        case 2: sum = sum + 10; break;
                        default: sum = sum + 1;
                        }
                        sum = sum + 100;
                    }
                    return sum;
                }
            "#),
            312
        );
    }

    #[test]
    fn pushed_arguments_preserve_nested_call_local_and_parameter_offsets() {
        assert_eq!(
            run(r#"
                int add3(int a, int b, int c) { return a + b + c; }
                int id(int x) { return x; }
                int probe(int parameter) {
                    int local;
                    local = 7;
                    return add3(local = 9, id(parameter), local + parameter);
                }
                int main(void) { return probe(5); }
            "#),
            26
        );
    }

    #[test]
    fn user_labels_are_mangled_per_function() {
        assert_eq!(
            run(r#"
                int first(void) { goto done; done: return 1; }
                int second(void) { goto done; done: return 2; }
                int main(void) { return first() + second(); }
            "#),
            3
        );
    }
}

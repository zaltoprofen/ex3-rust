//! Compiler for the pointerless EX3 C v0.1 subset.
mod ast;
mod codegen;
mod diagnostic;
mod lexer;
mod parser;
mod sema;

pub use diagnostic::{CcError, CcErrors, Span};

pub(crate) fn is_implementation_reserved(name: &str) -> bool {
    name.starts_with("__cc_") || name.starts_with("__ex3_")
}

pub fn compile(source: &str) -> Result<String, CcErrors> {
    let tokens = lexer::lex(source).map_err(CcErrors)?;
    let ast = parser::parse(tokens).map_err(CcErrors)?;
    let program = sema::analyze(ast).map_err(CcErrors)?;
    let plan = codegen::plan(&program).map_err(CcErrors)?;
    Ok(codegen::generate(&program, &plan))
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

//! Compiler for the pointerless EX3 C v0.1 subset.
mod codegen;
mod lexer;
mod parser;

use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CcError {
    pub span: Span,
    pub message: String,
}
impl CcError {
    fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CcErrors(pub Vec<CcError>);
impl fmt::Display for CcErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?
            }
            write!(f, "{}:{}: {}", e.span.line, e.span.column, e.message)?
        }
        Ok(())
    }
}
impl Error for CcErrors {}

pub fn compile(source: &str) -> Result<String, CcErrors> {
    let tokens = lexer::lex(source).map_err(CcErrors)?;
    let program = parser::parse(tokens).map_err(CcErrors)?;
    codegen::generate(&program).map_err(CcErrors)
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
        assert!(compile("int main(void) { break; return 0; }").is_err());
        assert!(compile("int main(void) { unsigned x; return 0; }").is_err());
        assert!(compile("void putchar(int c); int main(void) { putchar(65); return 0; }").is_ok());
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
}

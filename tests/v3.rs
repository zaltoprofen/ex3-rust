use ex3::{
    assembler::Assembler,
    emulator::{ArrayMemory, Cpu, NullIoBus},
    output::{format_mem, format_probe},
};
use std::process::Command;

#[test]
fn sample_golden_image_and_execution() {
    let source = include_str!("../examples/sample.asm");
    let assembled = Assembler::new().assemble(source).unwrap();
    assert_eq!(
        format_mem(&assembled.image),
        "@0010 05000016\n@0011 20000001\n@0012 06000016\n@0013 62000015\n@0014 60000010\n@0015 84000000\n@0016 ffffffff\n"
    );
    assert_eq!(format_probe(&assembled.image), "0016ffff\nf0000000\n");

    let mut cpu = Cpu::default();
    let mut memory = ArrayMemory::from_image(&assembled.image);
    cpu.run(&mut memory, &mut NullIoBus, 20).unwrap();
    assert_eq!(cpu.state().ac, 0);
    assert_eq!(cpu.state().executed_instructions, 5);
    assert!(cpu.state().zero());
}

#[test]
fn cli_runs_v3_program() {
    let output = Command::new(env!("CARGO_BIN_EXE_ex3"))
        .args(["run", "examples/halt.asm", "--max-steps", "20"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("halted=true"));
}

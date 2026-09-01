use ex3::{
    assembler::Assembler,
    emulator::{ArrayMemory, Cpu, Memory, NullIoBus},
    output::{format_mem, format_probe},
};
use std::process::Command;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

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

#[test]
fn run_displays_serial_output() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "ex3-serial-output-{}-{unique}.asm",
        std::process::id()
    ));
    fs::write(
        &path,
        "ORG 0x0010\nSIO\nLDA CHARACTER\nOUT\nHLT\nCHARACTER, DEC 65\nEND\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ex3"))
        .arg("run")
        .arg(&path)
        .arg("--max-steps")
        .arg("20")
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.starts_with(b"A"));
}

#[test]
fn stack_call_example_uses_the_v3_abi() {
    let assembled = Assembler::new()
        .assemble(include_str!("../examples/stack_call.asm"))
        .unwrap();
    let result_address = assembled.symbols["RESULT"];
    let mut cpu = Cpu::default();
    let mut memory = ArrayMemory::from_image(&assembled.image);

    cpu.run(&mut memory, &mut NullIoBus, 100).unwrap();

    assert_eq!(cpu.state().ac, 42);
    assert_eq!(memory.read(result_address), 42);
    assert_eq!(cpu.state().sp.get(), 0);
}

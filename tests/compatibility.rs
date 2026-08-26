use ex3::{
    assembler::Assembler,
    emulator::{ArrayMemory, Cpu, NullIoBus},
    output::{format_mem, format_probe},
};

#[test]
fn sample_golden_image_and_execution() {
    let source = include_str!("../examples/sample.asm");
    let assembled = Assembler::new().assemble(source).unwrap();
    assert_eq!(
        format_mem(&assembled.image),
        "@010 00020016\n@011 c1000001\n@012 00040016\n@013 00400015\n@014 00080010\n@015 80100000\n@016 ffffffff\n"
    );
    assert_eq!(format_probe(&assembled.image), "0016ffff\nf0000000\n");

    let mut cpu = Cpu::default();
    let mut memory = ArrayMemory::from_image(&assembled.image);
    cpu.run(&mut memory, &mut NullIoBus, 20).unwrap();
    assert_eq!(cpu.state().ac, 0);
    assert_eq!(cpu.state().executed_instructions, 5);
}

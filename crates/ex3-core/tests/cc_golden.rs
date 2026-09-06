use ex3_core::{assembler::Assembler, cc};

struct GoldenFixture {
    name: &'static str,
    source: &'static str,
    assembly: &'static str,
}

const FIXTURES: &[GoldenFixture] = &[
    GoldenFixture {
        name: "basics",
        source: include_str!("fixtures/cc-golden/basics.c"),
        assembly: include_str!("fixtures/cc-golden/basics.asm"),
    },
    GoldenFixture {
        name: "recursion-runtime",
        source: include_str!("fixtures/cc-golden/recursion-runtime.c"),
        assembly: include_str!("fixtures/cc-golden/recursion-runtime.asm"),
    },
    GoldenFixture {
        name: "control-flow",
        source: include_str!("fixtures/cc-golden/control-flow.c"),
        assembly: include_str!("fixtures/cc-golden/control-flow.asm"),
    },
];

#[test]
fn compiler_output_matches_v0_1_assembly_golden_files() {
    for fixture in FIXTURES {
        let compilation = cc::compile_with_debug_info(fixture.source)
            .unwrap_or_else(|error| panic!("{} failed to compile: {error}", fixture.name));

        assert_eq!(
            compilation.assembly.as_bytes(),
            fixture.assembly.as_bytes(),
            "{} assembly changed; update the golden only for an intentional codegen change",
            fixture.name
        );
    }
}

#[test]
fn compiler_output_matches_v0_1_machine_image_and_symbols() {
    for fixture in FIXTURES {
        let actual_assembly = cc::compile_with_debug_info(fixture.source)
            .unwrap_or_else(|error| panic!("{} failed to compile: {error}", fixture.name))
            .assembly;
        let expected = Assembler::new()
            .assemble(fixture.assembly)
            .unwrap_or_else(|error| panic!("{} golden assembly is invalid: {error}", fixture.name));
        let actual = Assembler::new()
            .assemble(&actual_assembly)
            .unwrap_or_else(|error| {
                panic!("{} generated assembly is invalid: {error}", fixture.name)
            });

        assert_eq!(
            actual.image, expected.image,
            "{} machine image changed",
            fixture.name
        );
        assert_eq!(
            actual.symbols, expected.symbols,
            "{} symbols changed",
            fixture.name
        );
    }
}

use ex3::{
    assembler::{Assembler, CellKind},
    debugger::{format_current, format_registers, Debugger, RunStop},
    emulator::{
        ArrayMemory, Cpu, IoBus, IoKind, IoTickContext, LegacyIoBus, Memory, NullIoBus, StepOutcome,
    },
    isa::{decode, Address},
    output::{format_mem, format_probe, parse_mem},
};
use std::{
    env, error::Error, fs, io::{self, Write}, path::{Path, PathBuf}, process::ExitCode,
};

// CLI層は引数とファイルI/Oだけを担当し、命令処理はlibraryへ委譲する。
fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
fn real_main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };
    let rest: Vec<String> = args.collect();
    match command.as_str() {
        "assemble" => assemble_cmd(&rest),
        "cc" => cc_cmd(&rest),
        "check" => check_cmd(&rest),
        "run" => run_cmd(&rest),
        "debug" => debug_cmd(&rest),
        "disasm" => disasm_cmd(&rest),
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        "--version" | "-V" => {
            println!("ex3 {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => Err(format!("unknown command `{command}` (try `ex3 help`)").into()),
    }
}
fn print_help() {
    println!("EX3 v3.0 toolchain\n\nUsage:\n  ex3 cc <file.c> [-S] [-o output]\n  ex3 assemble <file.asm> [-o file.mem] [--probe file.prb]\n  ex3 check <file.asm>\n  ex3 run <file.asm|file.mem> [--io null|legacy] [--seed N] [--max-steps N] [--trace] [--break ADDR]\n  ex3 debug <file.asm|file.mem> [--io null|legacy] [--seed N]\n  ex3 disasm --word <WORD>\n  ex3 disasm --file <file.mem>")
}

// 外部crateに依存せず、小規模な`--option value`形式だけを扱う。
fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}
fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|x| x == name)
}
fn input_arg(args: &[String]) -> Result<&str, Box<dyn Error>> {
    let value_options = [
        "-o",
        "--output",
        "--probe",
        "--max-steps",
        "--break",
        "--seed",
        "--io",
        "--word",
        "--file",
    ];
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
        } else if value_options.contains(&arg.as_str()) {
            skip_value = true;
        } else if !arg.starts_with('-') {
            return Ok(arg);
        }
    }
    Err("missing input file".into())
}
enum RuntimeIoBus {
    Null(NullIoBus),
    Legacy(LegacyIoBus),
}

impl RuntimeIoBus {
    fn output(&self, k: IoKind) -> &[u8] {
        match self {
            Self::Null(_) => &[],
            Self::Legacy(bus) => bus.output(k),
        }
    }
}

impl IoBus for RuntimeIoBus {
    fn tick(&mut self, context: IoTickContext) {
        match self {
            Self::Null(bus) => bus.tick(context),
            Self::Legacy(bus) => bus.tick(context),
        }
    }

    fn interrupt_pending(&self) -> bool {
        match self {
            Self::Null(bus) => bus.interrupt_pending(),
            Self::Legacy(bus) => bus.interrupt_pending(),
        }
    }

    fn read_input(&mut self, kind: IoKind) -> Option<u8> {
        match self {
            Self::Null(bus) => bus.read_input(kind),
            Self::Legacy(bus) => bus.read_input(kind),
        }
    }

    fn write_output(&mut self, kind: IoKind, value: u8) {
        match self {
            Self::Null(bus) => bus.write_output(kind, value),
            Self::Legacy(bus) => bus.write_output(kind, value),
        }
    }

    fn input_ready(&self, kind: IoKind) -> bool {
        match self {
            Self::Null(bus) => bus.input_ready(kind),
            Self::Legacy(bus) => bus.input_ready(kind),
        }
    }

    fn output_ready(&self, kind: IoKind) -> bool {
        match self {
            Self::Null(bus) => bus.output_ready(kind),
            Self::Legacy(bus) => bus.output_ready(kind),
        }
    }
}

fn io_bus(args: &[String]) -> Result<RuntimeIoBus, Box<dyn Error>> {
    let seed = option(args, "--seed")
        .map(|value| parse_u64(&value))
        .transpose()?
        .unwrap_or(0);
    match option(args, "--io").as_deref() {
        None | Some("legacy") => Ok(RuntimeIoBus::Legacy(LegacyIoBus::new(seed))),
        Some("null") => Ok(RuntimeIoBus::Null(NullIoBus)),
        Some(value) => {
            Err(format!("invalid I/O backend `{value}`; expected null or legacy").into())
        }
    }
}
fn assemble_source(path: &str) -> Result<ex3::assembler::AssemblyResult, Box<dyn Error>> {
    let source = fs::read_to_string(path)?;
    Ok(Assembler::new().assemble(&source)?)
}
fn assemble_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let input = input_arg(args)?;
    let result = assemble_source(input)?;
    let base = Path::new(input);
    let output = option(args, "-o")
        .or_else(|| option(args, "--output"))
        .map(PathBuf::from)
        .unwrap_or_else(|| base.with_extension("mem"));
    let probe = option(args, "--probe")
        .map(PathBuf::from)
        .unwrap_or_else(|| base.with_extension("prb"));
    fs::write(&output, format_mem(&result.image))?;
    fs::write(&probe, format_probe(&result.image))?;
    println!(
        "wrote {} and {} ({} words)",
        output.display(),
        probe.display(),
        result.image.cells.len()
    );
    Ok(())
}
fn cc_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let input = input_arg(args)?;
    let source = fs::read_to_string(input)?;
    let assembly = ex3::cc::compile(&source)?;
    let base = Path::new(input);
    if flag(args, "-S") || flag(args, "--assembly") {
        let output = option(args, "-o")
            .or_else(|| option(args, "--output"))
            .map(PathBuf::from)
            .unwrap_or_else(|| base.with_extension("s"));
        fs::write(&output, assembly)?;
        println!("wrote {}", output.display());
    } else {
        let result = Assembler::new().assemble(&assembly)?;
        let output = option(args, "-o")
            .or_else(|| option(args, "--output"))
            .map(PathBuf::from)
            .unwrap_or_else(|| base.with_extension("mem"));
        let probe = output.with_extension("prb");
        fs::write(&output, format_mem(&result.image))?;
        fs::write(&probe, format_probe(&result.image))?;
        println!(
            "wrote {} and {} ({} words)",
            output.display(),
            probe.display(),
            result.image.cells.len()
        );
    }
    Ok(())
}
fn check_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let input = input_arg(args)?;
    let result = assemble_source(input)?;
    println!(
        "ok: {} words, {} symbols",
        result.image.cells.len(),
        result.symbols.len()
    );
    Ok(())
}
fn load(path: &str) -> Result<ArrayMemory, Box<dyn Error>> {
    // 拡張子でアセンブリソースと既存メモリイメージを区別する。
    if Path::new(path)
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("mem"))
    {
        Ok(ArrayMemory::from_cells(&parse_mem(&fs::read_to_string(
            path,
        )?)?))
    } else {
        Ok(ArrayMemory::from_image(&assemble_source(path)?.image))
    }
}
fn parse_u64(s: &str) -> Result<u64, Box<dyn Error>> {
    Ok(if let Some(x) = s.strip_prefix("0x") {
        u64::from_str_radix(x, 16)?
    } else {
        s.parse()?
    })
}
fn parse_address(s: &str) -> Result<Address, Box<dyn Error>> {
    let v = if let Some(x) = s.strip_prefix("0x") {
        u16::from_str_radix(x, 16)?
    } else {
        u16::from_str_radix(s, 16)?
    };
    Ok(Address::new(v)?)
}
fn run_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let input = input_arg(args)?;
    let mut memory = load(input)?;
    let mut cpu = Cpu::new();
    let mut io = io_bus(args)?;
    let max = option(args, "--max-steps")
        .map(|x| parse_u64(&x))
        .transpose()?
        .unwrap_or(10_000_000);
    let mut dbg = Debugger::new();
    if let Some(b) = option(args, "--break") {
        dbg.add_breakpoint(parse_address(&b)?)
    }
    let trace = flag(args, "--trace");
    let mut steps = 0;
    let stop = loop {
        if cpu.state().halted {
            break RunStop::Halted;
        }
        if dbg.breakpoints().contains(&cpu.state().pc) {
            break RunStop::Breakpoint(cpu.state().pc);
        }
        if steps >= max {
            break RunStop::StepLimit;
        }
        if trace {
            println!("{}", format_current(&cpu, &memory))
        }
        let before = cpu.state().executed_instructions;
        cpu.step(&mut memory, &mut io)?;
        steps += cpu.state().executed_instructions - before;
    };
    let sout = io.output(IoKind::Serial);
    if sout.len() > 0 {
        println!("===== Serial Output =====");
        println!("{}", String::from_utf8_lossy(sout));
        println!("=========================");
    }
    println!("{}", format_registers(&cpu));
    match stop {
        RunStop::Halted => Ok(()),
        RunStop::Breakpoint(a) => {
            println!("stopped at breakpoint @{a}");
            Ok(())
        }
        RunStop::StepLimit => Err(format!("step limit exceeded ({max})").into()),
    }
}
fn disasm_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    if let Some(w) =
        option(args, "--word").or_else(|| args.first().filter(|x| !x.starts_with('-')).cloned())
    {
        let w = parse_u64(&w)? as u32;
        println!("{w:08x}  {}", decode(w)?);
        return Ok(());
    }
    if let Some(path) = option(args, "--file") {
        for (a, w) in parse_mem(&fs::read_to_string(path)?)? {
            match decode(w) {
                Ok(i) => println!("@{a} {w:08x}  {i}"),
                Err(_) => println!("@{a} {w:08x}  .word 0x{w:08x}"),
            }
        }
        return Ok(());
    }
    Err("disasm requires --word WORD or --file FILE".into())
}
fn debug_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let input = input_arg(args)?;
    let data_cells = if Path::new(input)
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("mem"))
    {
        Vec::new()
    } else {
        assemble_source(input)?
            .image
            .cells
            .into_iter()
            .filter(|cell| cell.kind != CellKind::Instruction)
            .collect::<Vec<_>>()
    };
    let mut memory = load(input)?;
    let mut cpu = Cpu::new();
    let mut bus = io_bus(args)?;
    let mut dbg = Debugger::new();
    println!("EX3 debugger; commands: s, r, b ADDR, regs, mem ADDR [N], data, disasm, q");
    let stdin = io::stdin();
    loop {
        print!("ex3> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let p: Vec<_> = line.split_whitespace().collect();
        match p.first().copied() {
            Some("s" | "step") => {
                match dbg.step(&mut cpu, &mut memory, &mut bus)? {
                    StepOutcome::Executed {
                        pc_before,
                        instruction,
                    } => println!("@{pc_before} {instruction}"),
                    x => println!("{x:?}"),
                }
                println!("{}", format_registers(&cpu))
            }
            Some("r" | "run") => println!(
                "{:?}",
                dbg.run(&mut cpu, &mut memory, &mut bus, 10_000_000)?
            ),
            Some("b" | "break") => {
                let a = parse_address(p.get(1).ok_or("break requires an address")?)?;
                println!(
                    "breakpoint @{a} {}",
                    if dbg.toggle_breakpoint(a) {
                        "set"
                    } else {
                        "removed"
                    }
                )
            }
            Some("regs") => println!("{}", format_registers(&cpu)),
            Some("disasm") => println!("{}", format_current(&cpu, &memory)),
            Some("data") => {
                if data_cells.is_empty() {
                    println!("no assembler data metadata (or no data cells)")
                } else {
                    for cell in &data_cells {
                        println!("@{} {:08x}", cell.address, memory.read(cell.address))
                    }
                }
            }
            Some("mem") => {
                let a = parse_address(p.get(1).copied().unwrap_or("000"))?;
                let n = p.get(2).map(|x| x.parse()).transpose()?.unwrap_or(16);
                for x in 0..n {
                    let at = a.wrapping_add(x);
                    println!("@{at} {:08x}", memory.read(at))
                }
            }
            Some("q" | "quit") => break,
            Some("help" | "h") => {
                println!("s | r | b ADDR | regs | mem ADDR [N] | data | disasm | q")
            }
            Some(x) => println!("unknown command `{x}`"),
            None => {}
        }
    }
    Ok(())
}

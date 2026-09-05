use crate::{
    dto::{
        AssemblySourceMapRow, CompileResult, CpuSnapshot, DisassemblyRow, MemoryRow,
        RunChunkResult, RunStatus, StepOutcomeDto, StepResult, SymbolEntry,
    },
    error::Ex3Error,
};
use ex3_core::{
    assembler::{Assembler, AssemblySourceMapEntry, CellKind},
    cc,
    debugger::{Debugger, RunStop},
    emulator::{ArrayMemory, Cpu, DeterministicIoBus, IoKind, Memory, StepOutcome},
    isa::{decode, Address},
};
use std::collections::BTreeMap;

const MAX_RANGE_WORDS: u32 = 256;

pub struct SessionCore {
    cpu: Cpu,
    memory: ArrayMemory,
    initial_memory: ArrayMemory,
    io: DeterministicIoBus,
    debugger: Debugger,
    assembly: String,
    source_map: Vec<AssemblySourceMapEntry>,
    symbols: BTreeMap<String, Address>,
    loaded: bool,
}

impl Default for SessionCore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCore {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            memory: ArrayMemory::default(),
            initial_memory: ArrayMemory::default(),
            io: DeterministicIoBus::default(),
            debugger: Debugger::new(),
            assembly: String::new(),
            source_map: Vec::new(),
            symbols: BTreeMap::new(),
            loaded: false,
        }
    }

    pub fn compile_and_load(&mut self, source: &str) -> Result<CompileResult, Ex3Error> {
        let assembly = cc::compile(source).map_err(Ex3Error::from)?;
        let assembled = Assembler::new()
            .assemble(&assembly)
            .map_err(Ex3Error::from)?;
        let loaded_words = u32::try_from(assembled.image.cells.len())
            .map_err(|_| Ex3Error::session("loaded word count exceeds u32"))?;
        let source_map_rows = assembled
            .source_map
            .iter()
            .map(|entry| AssemblySourceMapRow {
                address: entry.address.get(),
                line: entry.span.line,
                executable: assembled.image.cells.iter().any(|cell| {
                    cell.address == entry.address && cell.kind == CellKind::Instruction
                }),
            })
            .collect();
        let memory = ArrayMemory::from_image(&assembled.image);
        let symbols = assembled.symbols;
        let source_map = assembled.source_map;

        self.cpu.reset();
        self.memory = memory.clone();
        self.initial_memory = memory;
        self.io = DeterministicIoBus::default();
        self.assembly = assembly.clone();
        self.source_map = source_map;
        self.symbols = symbols;
        self.loaded = true;

        Ok(CompileResult {
            assembly,
            symbols: self.symbol_entries(),
            source_map: source_map_rows,
            loaded_words,
            snapshot: self.snapshot()?,
        })
    }

    pub fn reset(&mut self) -> Result<CpuSnapshot, Ex3Error> {
        self.ensure_loaded()?;
        self.cpu.reset();
        self.memory = self.initial_memory.clone();
        self.io = DeterministicIoBus::default();
        self.snapshot()
    }

    pub fn step(&mut self) -> Result<StepResult, Ex3Error> {
        self.ensure_loaded()?;
        let outcome = self
            .debugger
            .step(&mut self.cpu, &mut self.memory, &mut self.io)
            .map_err(Ex3Error::from)?;
        let (outcome, pc_before, instruction) = match outcome {
            StepOutcome::Executed {
                pc_before,
                instruction,
            } => (
                StepOutcomeDto::Executed,
                Some(pc_before.get()),
                Some(instruction.to_string()),
            ),
            StepOutcome::Interrupted => (StepOutcomeDto::Interrupted, None, None),
            StepOutcome::Halted => (StepOutcomeDto::Halted, None, None),
        };
        Ok(StepResult {
            outcome,
            pc_before,
            instruction,
            snapshot: self.snapshot()?,
        })
    }

    pub fn run_chunk(&mut self, max_instructions: u32) -> Result<RunChunkResult, Ex3Error> {
        self.ensure_loaded()?;
        let before = self.cpu.state().executed_instructions;
        let stop = self
            .debugger
            .run(
                &mut self.cpu,
                &mut self.memory,
                &mut self.io,
                u64::from(max_instructions),
            )
            .map_err(Ex3Error::from)?;
        let executed = self.cpu.state().executed_instructions.wrapping_sub(before) as u32;
        let (status, breakpoint_address) = match stop {
            RunStop::StepLimit => (RunStatus::Running, None),
            RunStop::Halted => (RunStatus::Halted, None),
            RunStop::Breakpoint(address) => (RunStatus::Breakpoint, Some(address.get())),
        };
        Ok(RunChunkResult {
            status,
            executed,
            breakpoint_address,
            snapshot: self.snapshot()?,
        })
    }

    pub fn snapshot(&self) -> Result<CpuSnapshot, Ex3Error> {
        self.ensure_loaded()?;
        let state = self.cpu.state();
        let io = self.cpu.io_state();
        Ok(CpuSnapshot {
            pc: state.pc.get(),
            sp: state.sp.get(),
            ac: state.ac,
            ir: state.ir,
            psr: state.psr,
            ien: state.interrupt_enabled(),
            negative: state.negative(),
            zero: state.zero(),
            carry: state.carry(),
            overflow: state.overflow(),
            halted: state.halted,
            interrupt_pending: state.interrupt_pending,
            executed_instructions: state.executed_instructions,
            serial_selected: io.serial_selected,
            interrupt_mask: io.interrupt_mask,
            input_register: io.input_register,
            assembly_line: self.source_line(state.pc),
        })
    }

    pub fn memory_range(&self, start: u16, count: u32) -> Result<Vec<MemoryRow>, Ex3Error> {
        self.ensure_loaded()?;
        Self::validate_count(count)?;
        let start = Address::from_low16(u32::from(start));
        Ok((0..count)
            .map(|offset| {
                let address = start.wrapping_add(offset as u16);
                MemoryRow {
                    address: address.get(),
                    word: self.memory.read(address),
                }
            })
            .collect())
    }

    pub fn disassembly_range(
        &self,
        start: u16,
        count: u32,
    ) -> Result<Vec<DisassemblyRow>, Ex3Error> {
        self.ensure_loaded()?;
        Self::validate_count(count)?;
        let start = Address::from_low16(u32::from(start));
        Ok((0..count)
            .map(|offset| {
                let address = start.wrapping_add(offset as u16);
                let word = self.memory.read(address);
                let decoded = decode(word);
                let (instruction, valid) = match decoded {
                    Ok(instruction) => (instruction.to_string(), true),
                    Err(_) => (format!(".word 0x{word:08x}"), false),
                };
                DisassemblyRow {
                    address: address.get(),
                    word,
                    instruction,
                    valid,
                    source_line: self.source_line(address),
                    labels: self.labels_at(address),
                }
            })
            .collect())
    }

    pub fn toggle_breakpoint(&mut self, address: u16) -> bool {
        self.debugger
            .toggle_breakpoint(Address::from_low16(u32::from(address)))
    }

    pub fn clear_breakpoints(&mut self) {
        let addresses: Vec<_> = self.debugger.breakpoints().iter().copied().collect();
        for address in addresses {
            self.debugger.remove_breakpoint(address);
        }
    }

    pub fn breakpoints(&self) -> Vec<u16> {
        self.debugger
            .breakpoints()
            .iter()
            .map(|address| address.get())
            .collect()
    }

    pub fn serial_output(&self) -> String {
        String::from_utf8_lossy(self.io.output(IoKind::Serial)).into_owned()
    }

    fn ensure_loaded(&self) -> Result<(), Ex3Error> {
        if self.loaded {
            Ok(())
        } else {
            Err(Ex3Error::session("no program is loaded"))
        }
    }

    fn validate_count(count: u32) -> Result<(), Ex3Error> {
        if count <= MAX_RANGE_WORDS {
            Ok(())
        } else {
            Err(Ex3Error::session(format!(
                "range count {count} exceeds maximum {MAX_RANGE_WORDS}"
            )))
        }
    }

    fn source_line(&self, address: Address) -> Option<usize> {
        self.source_map
            .iter()
            .find(|entry| entry.address == address)
            .map(|entry| entry.span.line)
    }

    fn labels_at(&self, address: Address) -> Vec<String> {
        self.symbols
            .iter()
            .filter(|(_, symbol_address)| **symbol_address == address)
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn symbol_entries(&self) -> Vec<SymbolEntry> {
        self.symbols
            .iter()
            .map(|(name, address)| SymbolEntry {
                name: name.clone(),
                address: address.get(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dto::{RunStatus, StepOutcomeDto},
        error::ErrorStage,
    };
    use ex3_core::emulator::IoBus;

    const RETURN_42: &str = "int main(void) { return 42; }";

    #[test]
    fn compile_load_and_run_return_42() {
        let mut session = SessionCore::new();
        let compiled = session.compile_and_load(RETURN_42).unwrap();
        assert!(!compiled.assembly.is_empty());
        assert!(compiled.loaded_words > 0);
        assert!(!compiled.source_map.is_empty());
        assert!(compiled.source_map.iter().any(|entry| entry.executable));
        assert_eq!(compiled.snapshot.pc, Address::RESET.get());

        let result = session.run_chunk(1_000_000).unwrap();
        assert_eq!(result.status, RunStatus::Halted);
        assert_eq!(result.snapshot.ac, 42);
        assert!(result.snapshot.halted);
    }

    #[test]
    fn source_map_distinguishes_data_from_instructions() {
        let mut session = SessionCore::new();
        let compiled = session
            .compile_and_load("int value = 7; int main(void) { return value; }")
            .unwrap();

        assert!(compiled.source_map.iter().any(|entry| entry.executable));
        assert!(compiled.source_map.iter().any(|entry| !entry.executable));
    }

    #[test]
    fn failed_compile_does_not_replace_loaded_program() {
        let mut session = SessionCore::new();
        session.compile_and_load(RETURN_42).unwrap();
        let error = session.compile_and_load("int main( {").unwrap_err();
        assert_eq!(error.stage, ErrorStage::Compiler);
        assert!(!error.diagnostics.is_empty());

        let result = session.run_chunk(1_000_000).unwrap();
        assert_eq!(result.status, RunStatus::Halted);
        assert_eq!(result.snapshot.ac, 42);
    }

    #[test]
    fn reset_restores_cpu_memory_and_io_but_preserves_breakpoints() {
        let mut session = SessionCore::new();
        session.compile_and_load(RETURN_42).unwrap();
        let initial_word = session.memory.read(Address::RESET);
        session.memory.write(Address::RESET, 0);
        session.io.write_output(IoKind::Serial, b'X');
        session.toggle_breakpoint(Address::RESET.get());
        session.cpu.state_mut().ac = 99;

        let snapshot = session.reset().unwrap();
        assert_eq!(snapshot.ac, 0);
        assert_eq!(session.memory.read(Address::RESET), initial_word);
        assert_eq!(session.serial_output(), "");
        assert_eq!(session.breakpoints(), vec![Address::RESET.get()]);
        session.clear_breakpoints();
        assert!(session.breakpoints().is_empty());
    }

    #[test]
    fn step_and_breakpoint_report_fetch_state() {
        let mut session = SessionCore::new();
        session.compile_and_load(RETURN_42).unwrap();
        session.toggle_breakpoint(Address::RESET.get());

        let stopped = session.run_chunk(10).unwrap();
        assert_eq!(stopped.status, RunStatus::Breakpoint);
        assert_eq!(stopped.breakpoint_address, Some(Address::RESET.get()));
        assert_eq!(stopped.executed, 0);

        let stepped = session.step().unwrap();
        assert_eq!(stepped.outcome, StepOutcomeDto::Executed);
        assert_eq!(stepped.pc_before, Some(Address::RESET.get()));
        assert_eq!(stepped.snapshot.executed_instructions, 1);
    }

    #[test]
    fn zero_length_chunk_returns_running_without_execution() {
        let mut session = SessionCore::new();
        session.compile_and_load(RETURN_42).unwrap();

        let result = session.run_chunk(0).unwrap();
        assert_eq!(result.status, RunStatus::Running);
        assert_eq!(result.executed, 0);
        assert_eq!(result.snapshot.executed_instructions, 0);
    }

    #[test]
    fn memory_range_wraps_and_enforces_the_limit() {
        let mut session = SessionCore::new();
        session.compile_and_load(RETURN_42).unwrap();
        session.memory.write(Address::from_low16(0xffff), 1);
        session.memory.write(Address::ZERO, 2);

        let rows = session.memory_range(0xffff, 2).unwrap();
        assert_eq!(
            rows[0],
            MemoryRow {
                address: 0xffff,
                word: 1
            }
        );
        assert_eq!(
            rows[1],
            MemoryRow {
                address: 0,
                word: 2
            }
        );
        assert!(session.memory_range(0, 257).is_err());
    }

    #[test]
    fn disassembly_is_loss_tolerant_and_includes_metadata() {
        let mut session = SessionCore::new();
        session.compile_and_load(RETURN_42).unwrap();
        let (label, address) = session
            .symbols
            .iter()
            .find(|(_, address)| session.source_line(**address).is_some())
            .map(|(label, address)| (label.clone(), *address))
            .unwrap();
        session.memory.write(address, 0xe000_0000);

        let rows = session.disassembly_range(address.get(), 1).unwrap();
        assert!(!rows[0].valid);
        assert_eq!(rows[0].instruction, ".word 0xe0000000");
        assert!(rows[0].source_line.is_some());
        assert!(rows[0].labels.contains(&label));
    }

    #[test]
    fn serial_output_is_returned_lossily_and_cleared_by_reset() {
        let mut session = SessionCore::new();
        session
            .compile_and_load("void putchar(int c); int main(void) { putchar(65); return 0; }")
            .unwrap();
        let result = session.run_chunk(1_000_000).unwrap();
        assert_eq!(result.status, RunStatus::Halted);
        assert_eq!(session.serial_output(), "A");
        session.reset().unwrap();
        assert_eq!(session.serial_output(), "");
    }

    #[test]
    fn operations_require_a_loaded_program() {
        let mut session = SessionCore::new();
        assert_eq!(session.snapshot().unwrap_err().stage, ErrorStage::Session);
        assert_eq!(session.step().unwrap_err().stage, ErrorStage::Session);
        assert_eq!(session.run_chunk(1).unwrap_err().stage, ErrorStage::Session);
    }
}

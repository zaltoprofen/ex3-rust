//! Links compiler-side assembly-line metadata to assembled machine addresses.

use crate::{
    assembler::{AssemblyResult, CellKind},
    cc::{
        ActiveTemporaryDebugInfo, CompilerDebugInfo, DynamicStackSlotDebugInfo, FixedFrameState,
        FunctionDebugId, LocalSlotDebugInfo, ParameterSlotDebugInfo, ReturnAddressSlotDebugInfo,
    },
    isa::{Address, Instruction, SpRelativeOp},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    fmt,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramDebugInfo {
    pub functions: Vec<LinkedFunctionDebugInfo>,
    pub instructions: BTreeMap<Address, InstructionDebugInfo>,
    symbols: BTreeMap<String, Address>,
    linked_symbols: Vec<LinkedSymbolInfo>,
    executable_addresses: BTreeSet<Address>,
}

impl ProgramDebugInfo {
    pub fn instruction(&self, address: Address) -> Option<&InstructionDebugInfo> {
        self.instructions.get(&address)
    }

    pub fn function(&self, id: FunctionDebugId) -> Option<&LinkedFunctionDebugInfo> {
        self.functions.iter().find(|function| function.id == id)
    }

    pub fn symbol_address(&self, name: &str) -> Option<Address> {
        self.symbols.get(name).copied()
    }

    /// Finds the closest preceding symbol for display only. This does not mean
    /// that `address` belongs to the returned symbol's range.
    pub fn nearest_symbol(&self, address: Address) -> Option<NearestSymbol> {
        self.symbols
            .iter()
            .filter(|(_, symbol_address)| **symbol_address <= address)
            .max_by_key(|(_, symbol_address)| **symbol_address)
            .map(|(name, symbol_address)| NearestSymbol {
                name: name.clone(),
                address: *symbol_address,
                offset: address.get().wrapping_sub(symbol_address.get()),
            })
    }

    /// Returns a symbol only when `address` lies in its linked cell range.
    pub fn symbol_at(&self, address: Address) -> Option<&LinkedSymbolInfo> {
        self.linked_symbols
            .iter()
            .filter(|symbol| symbol.contains(address))
            .max_by_key(|symbol| symbol_kind_priority(symbol.kind))
    }

    pub fn is_executable(&self, address: Address) -> bool {
        self.executable_addresses.contains(&address)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NearestSymbol {
    pub name: String,
    pub address: Address,
    pub offset: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkedSymbolKind {
    CFunction,
    Runtime,
    Assembly,
    Data,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedSymbolInfo {
    pub name: String,
    pub address_start: Address,
    pub address_end_exclusive: Address,
    pub kind: LinkedSymbolKind,
}

impl LinkedSymbolInfo {
    pub fn contains(&self, address: Address) -> bool {
        FunctionAddressRange {
            start: self.address_start,
            end_exclusive: self.address_end_exclusive,
        }
        .contains(address)
    }
}

const fn symbol_kind_priority(kind: LinkedSymbolKind) -> u8 {
    match kind {
        LinkedSymbolKind::Runtime => 5,
        LinkedSymbolKind::CFunction => 4,
        LinkedSymbolKind::Assembly => 3,
        LinkedSymbolKind::Data => 2,
        LinkedSymbolKind::Other => 1,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionDebugInfo {
    pub function_id: FunctionDebugId,
    /// Signed word delta satisfying
    /// `canonical_frame_sp = current_sp + frame_base_delta` immediately before
    /// the instruction executes.
    pub frame_base_delta: i32,
    pub fixed_frame_state: FixedFrameState,
    pub active_temporaries: Vec<ActiveTemporaryDebugInfo>,
    pub dynamic_stack_slots: Vec<DynamicStackSlotDebugInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionAddressRange {
    pub start: Address,
    pub end_exclusive: Address,
}

impl FunctionAddressRange {
    pub fn contains(self, address: Address) -> bool {
        if self.start.get() < self.end_exclusive.get() {
            self.start <= address && address < self.end_exclusive
        } else {
            address >= self.start || address < self.end_exclusive
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedFunctionDebugInfo {
    pub id: FunctionDebugId,
    pub name: String,
    pub address_start: Address,
    pub address_end_exclusive: Address,
    /// Normally one range. A list keeps the representation usable if codegen
    /// later emits cold or otherwise non-contiguous function regions.
    pub address_ranges: Vec<FunctionAddressRange>,
    pub frame_size: u16,
    pub parameters: Vec<ParameterSlotDebugInfo>,
    pub return_address: ReturnAddressSlotDebugInfo,
    pub locals: Vec<LocalSlotDebugInfo>,
    pub temporary_count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DebugInfoLinkError {
    DuplicateAssemblyLine(u32),
    AssemblyLineOutOfRange(usize),
    MissingExecutableAssemblyLine(u32),
    UnknownFunction(FunctionDebugId),
    MissingFrame(FunctionDebugId),
    MissingFunctionSymbol(String),
    FunctionHasNoInstructions(String),
    FunctionSymbolMismatch {
        function: String,
        symbol: Address,
        first_instruction: Address,
    },
}

impl fmt::Display for DebugInfoLinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateAssemblyLine(line) => {
                write!(formatter, "duplicate compiler debug metadata for assembly line {line}")
            }
            Self::AssemblyLineOutOfRange(line) => {
                write!(formatter, "assembly source line {line} exceeds u32")
            }
            Self::MissingExecutableAssemblyLine(line) => write!(
                formatter,
                "compiler debug metadata at assembly line {line} has no executable machine word"
            ),
            Self::UnknownFunction(id) => {
                write!(formatter, "instruction metadata references unknown function {id:?}")
            }
            Self::MissingFrame(id) => {
                write!(formatter, "function {id:?} has no fixed frame metadata")
            }
            Self::MissingFunctionSymbol(name) => {
                write!(formatter, "assembled program has no symbol for function `{name}`")
            }
            Self::FunctionHasNoInstructions(name) => {
                write!(formatter, "function `{name}` has no linked instructions")
            }
            Self::FunctionSymbolMismatch {
                function,
                symbol,
                first_instruction,
            } => write!(
                formatter,
                "function `{function}` symbol {symbol} does not match first instruction {first_instruction}"
            ),
        }
    }
}

impl Error for DebugInfoLinkError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugInfoLinkErrors(pub Vec<DebugInfoLinkError>);

impl fmt::Display for DebugInfoLinkErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index != 0 {
                writeln!(formatter)?;
            }
            error.fmt(formatter)?;
        }
        Ok(())
    }
}

impl Error for DebugInfoLinkErrors {}

pub fn link_program_debug_info(
    compiler: &CompilerDebugInfo,
    assembled: &AssemblyResult,
) -> Result<ProgramDebugInfo, DebugInfoLinkErrors> {
    let mut errors = Vec::new();
    let known_functions = compiler
        .functions
        .iter()
        .map(|function| function.id)
        .collect::<HashSet<_>>();
    let mut states_by_line = BTreeMap::new();
    for state in &compiler.assembly_lines {
        if !known_functions.contains(&state.function_id) {
            errors.push(DebugInfoLinkError::UnknownFunction(state.function_id));
        }
        if states_by_line.insert(state.assembly_line, state).is_some() {
            errors.push(DebugInfoLinkError::DuplicateAssemblyLine(
                state.assembly_line,
            ));
        }
    }

    let executable_addresses = assembled
        .image
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Instruction)
        .map(|cell| cell.address)
        .collect::<HashSet<_>>();
    let mut linked_lines = HashSet::new();
    let mut instructions = BTreeMap::new();
    for source in &assembled.source_map {
        if !executable_addresses.contains(&source.address) {
            continue;
        }
        let Ok(line) = u32::try_from(source.span.line) else {
            errors.push(DebugInfoLinkError::AssemblyLineOutOfRange(source.span.line));
            continue;
        };
        let Some(state) = states_by_line.get(&line).copied() else {
            continue;
        };
        let post_statement_state = states_by_line
            .range((std::ops::Bound::Excluded(line), std::ops::Bound::Unbounded))
            .map(|(_, candidate)| *candidate)
            .find(|candidate| candidate.function_id == state.function_id);
        let mut frame_base_delta = state.frame_base_delta;
        let mut active_temporaries = state.active_temporaries.clone();
        let mut dynamic_stack_slots = state.dynamic_stack_slots.clone();
        if is_push_store(source.expansion_index, source.instruction) {
            frame_base_delta = frame_base_delta
                .checked_add(1)
                .expect("compiler stack adjustment exceeds i32");
            if let Some(post) = post_statement_state {
                active_temporaries = post.active_temporaries.clone();
                dynamic_stack_slots = post.dynamic_stack_slots.clone();
            }
        }
        linked_lines.insert(line);
        instructions.insert(
            source.address,
            InstructionDebugInfo {
                function_id: state.function_id,
                frame_base_delta,
                fixed_frame_state: state.fixed_frame_state,
                active_temporaries,
                dynamic_stack_slots,
            },
        );
    }
    for line in states_by_line.keys() {
        if !linked_lines.contains(line) {
            errors.push(DebugInfoLinkError::MissingExecutableAssemblyLine(*line));
        }
    }

    let mut functions = Vec::with_capacity(compiler.functions.len());
    for symbols in &compiler.functions {
        let Some(frame) = compiler
            .frames
            .iter()
            .find(|frame| frame.function_id == symbols.id)
        else {
            errors.push(DebugInfoLinkError::MissingFrame(symbols.id));
            continue;
        };
        let Some(symbol_address) = assembled.symbols.get(&symbols.name).copied() else {
            errors.push(DebugInfoLinkError::MissingFunctionSymbol(
                symbols.name.clone(),
            ));
            continue;
        };
        let addresses = instructions
            .iter()
            .filter_map(|(address, instruction)| {
                (instruction.function_id == symbols.id).then_some(*address)
            })
            .collect::<BTreeSet<_>>();
        if addresses.is_empty() {
            errors.push(DebugInfoLinkError::FunctionHasNoInstructions(
                symbols.name.clone(),
            ));
            continue;
        }
        let address_ranges = contiguous_ranges(&addresses);
        let address_start = address_ranges[0].start;
        let address_end_exclusive = address_ranges[address_ranges.len() - 1].end_exclusive;
        if symbol_address != address_start {
            errors.push(DebugInfoLinkError::FunctionSymbolMismatch {
                function: symbols.name.clone(),
                symbol: symbol_address,
                first_instruction: address_start,
            });
        }
        functions.push(LinkedFunctionDebugInfo {
            id: symbols.id,
            name: symbols.name.clone(),
            address_start,
            address_end_exclusive,
            address_ranges,
            frame_size: frame.frame_size,
            parameters: frame.parameters.clone(),
            return_address: frame.return_address,
            locals: frame.locals.clone(),
            temporary_count: frame.temporary_count,
        });
    }

    let linked_symbols = link_symbols(compiler, assembled, &instructions);
    if errors.is_empty() {
        Ok(ProgramDebugInfo {
            functions,
            instructions,
            symbols: assembled.symbols.clone(),
            linked_symbols,
            executable_addresses: executable_addresses.into_iter().collect(),
        })
    } else {
        Err(DebugInfoLinkErrors(errors))
    }
}

fn is_push_store(expansion_index: u16, instruction: Option<Instruction>) -> bool {
    expansion_index > 0
        && matches!(
            instruction,
            Some(Instruction::SpRelative {
                op: SpRelativeOp::Stsp,
                offset,
            }) if offset.as_i16() == 0
        )
}

fn link_symbols(
    compiler: &CompilerDebugInfo,
    assembled: &AssemblyResult,
    instructions: &BTreeMap<Address, InstructionDebugInfo>,
) -> Vec<LinkedSymbolInfo> {
    let cells = assembled
        .image
        .cells
        .iter()
        .map(|cell| (cell.address, cell.kind))
        .collect::<BTreeMap<_, _>>();
    let c_function_names = compiler
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    let distinct_starts = assembled.symbols.values().copied().collect::<BTreeSet<_>>();
    assembled
        .symbols
        .iter()
        .filter_map(|(name, start)| {
            let cell_kind = cells.get(start).copied()?;
            let instruction_cells = cell_kind == CellKind::Instruction;
            let next_start = distinct_starts
                .range((
                    std::ops::Bound::Excluded(*start),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .copied();
            let mut last = *start;
            while let Some(next_value) = last.get().checked_add(1) {
                let next = Address::from_low16(u32::from(next_value));
                if next_start.is_some_and(|limit| next >= limit)
                    || cells
                        .get(&next)
                        .is_none_or(|kind| (*kind == CellKind::Instruction) != instruction_cells)
                {
                    break;
                }
                last = next;
            }
            let kind = if !instruction_cells {
                LinkedSymbolKind::Data
            } else if name.starts_with("__ex3_") {
                LinkedSymbolKind::Runtime
            } else if c_function_names.contains(name.as_str()) || instructions.contains_key(start) {
                LinkedSymbolKind::CFunction
            } else {
                LinkedSymbolKind::Assembly
            };
            Some(LinkedSymbolInfo {
                name: name.clone(),
                address_start: *start,
                address_end_exclusive: last.wrapping_add(1),
                kind,
            })
        })
        .collect()
}

fn contiguous_ranges(addresses: &BTreeSet<Address>) -> Vec<FunctionAddressRange> {
    let mut addresses = addresses.iter().copied();
    let Some(mut start) = addresses.next() else {
        return Vec::new();
    };
    let mut previous = start;
    let mut ranges = Vec::new();
    for address in addresses {
        if previous.get().checked_add(1) != Some(address.get()) {
            ranges.push(FunctionAddressRange {
                start,
                end_exclusive: previous.wrapping_add(1),
            });
            start = address;
        }
        previous = address;
    }
    ranges.push(FunctionAddressRange {
        start,
        end_exclusive: previous.wrapping_add(1),
    });
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assembler::Assembler, cc};

    fn compile_assemble_link(source: &str) -> (cc::Compilation, AssemblyResult, ProgramDebugInfo) {
        let compilation = cc::compile_with_debug_info(source).unwrap();
        let assembled = Assembler::new().assemble(&compilation.assembly).unwrap();
        let linked = link_program_debug_info(&compilation.debug_info, &assembled).unwrap();
        (compilation, assembled, linked)
    }

    fn assembly_line(source: &str, needle: &str) -> u32 {
        source
            .lines()
            .position(|line| line.trim() == needle)
            .map(|index| u32::try_from(index + 1).unwrap())
            .unwrap()
    }

    #[test]
    fn links_one_based_assembly_lines_to_machine_addresses() {
        let (compilation, assembled, linked) = compile_assemble_link(
            r#"
                int negate(int value) { return -value; }
                int main(void) { return negate(7); }
            "#,
        );
        let call_line = assembly_line(&compilation.assembly, "CALL negate");
        let call_address = assembled
            .source_map
            .iter()
            .find(|entry| entry.span.line == call_line as usize)
            .unwrap()
            .address;
        let assembly_state = compilation
            .debug_info
            .assembly_lines
            .iter()
            .find(|state| state.assembly_line == call_line)
            .unwrap();
        let instruction = linked.instruction(call_address).unwrap();

        assert_eq!(instruction.function_id, assembly_state.function_id);
        assert_eq!(
            instruction.frame_base_delta,
            assembly_state.frame_base_delta
        );
        assert_eq!(
            instruction.dynamic_stack_slots,
            assembly_state.dynamic_stack_slots
        );
    }

    #[test]
    fn adjusts_debug_state_within_push_expansion() {
        let (compilation, assembled, linked) = compile_assemble_link(
            r#"
                int identity(int value) { return value; }
                int main(void) { return identity(42); }
            "#,
        );
        let push_line = assembly_line(&compilation.assembly, "PUSH");
        let entries = assembled
            .source_map
            .iter()
            .filter(|entry| entry.span.line == push_line as usize)
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].expansion_index, 0);
        assert_eq!(entries[1].expansion_index, 1);
        let before_adjust = linked.instruction(entries[0].address).unwrap();
        let before_store = linked.instruction(entries[1].address).unwrap();
        assert_eq!(
            before_store.frame_base_delta,
            before_adjust.frame_base_delta + 1
        );
        assert_eq!(
            before_store.dynamic_stack_slots.len(),
            before_adjust.dynamic_stack_slots.len() + 1
        );
    }

    #[test]
    fn links_function_ranges_and_leaves_non_c_addresses_unmapped() {
        let source = r#"
                int data = 9;
                int double_value(int value) { return value + value; }
                int main(void) { return double_value(2 * 3); }
            "#;
        let plain_assembly = cc::compile(source).unwrap();
        let plain_image = Assembler::new().assemble(&plain_assembly).unwrap().image;
        let (compilation, assembled, linked) = compile_assemble_link(source);
        assert_eq!(plain_assembly, compilation.assembly);
        assert_eq!(plain_image, assembled.image);
        assert_eq!(linked.functions.len(), 2);
        for function in &linked.functions {
            assert_eq!(function.address_start, assembled.symbols[&function.name]);
            assert_eq!(function.address_ranges.len(), 1);
            assert!(function.address_ranges[0].contains(function.address_start));
            let function_addresses = linked
                .instructions
                .iter()
                .filter_map(|(address, instruction)| {
                    (instruction.function_id == function.id).then_some(*address)
                })
                .collect::<Vec<_>>();
            assert!(!function_addresses.is_empty());
            assert!(function_addresses.iter().all(|address| function
                .address_ranges
                .iter()
                .any(|range| range.contains(*address))));
            assert_eq!(
                function.address_end_exclusive,
                function_addresses.last().unwrap().wrapping_add(1)
            );
        }

        assert!(linked.instruction(Address::RESET).is_none());
        assert!(linked.instruction(Address::RESET.wrapping_add(1)).is_none());
        assert!(linked.instruction(assembled.symbols["data"]).is_none());
        assert!(linked
            .instruction(assembled.symbols["__ex3_mul_i32"])
            .is_none());

        let image_before_link = assembled.image.clone();
        let relinked = link_program_debug_info(&compilation.debug_info, &assembled).unwrap();
        assert_eq!(assembled.image, image_before_link);
        assert_eq!(relinked, linked);
    }

    #[test]
    fn accepts_programs_without_compiler_metadata() {
        let assembled = Assembler::new()
            .assemble("ORG 0x0010\nmain:\nHLT\nEND\n")
            .unwrap();
        let linked = link_program_debug_info(&CompilerDebugInfo::default(), &assembled).unwrap();

        assert!(linked.functions.is_empty());
        assert!(linked.instructions.is_empty());
    }

    #[test]
    fn reports_unlinkable_metadata_without_panicking() {
        let compilation = cc::compile_with_debug_info("int main(void) { return 0; }").unwrap();
        let mut assembled = Assembler::new().assemble(&compilation.assembly).unwrap();
        assembled.symbols.remove("main");
        let errors = link_program_debug_info(&compilation.debug_info, &assembled).unwrap_err();

        assert!(errors
            .0
            .iter()
            .any(|error| matches!(error, DebugInfoLinkError::MissingFunctionSymbol(name) if name == "main")));

        let mut compiler = compilation.debug_info;
        compiler.assembly_lines[0].assembly_line = u32::MAX;
        let assembled = Assembler::new().assemble(&compilation.assembly).unwrap();
        let errors = link_program_debug_info(&compiler, &assembled).unwrap_err();
        assert!(errors.0.iter().any(|error| matches!(
            error,
            DebugInfoLinkError::MissingExecutableAssemblyLine(u32::MAX)
        )));
    }

    #[test]
    fn wrapped_function_ranges_use_ex3_address_arithmetic() {
        let range = FunctionAddressRange {
            start: Address::new(0xfffe).unwrap(),
            end_exclusive: Address::new(0x0001).unwrap(),
        };

        assert!(range.contains(Address::new(0xffff).unwrap()));
        assert!(range.contains(Address::new(0x0000).unwrap()));
        assert!(!range.contains(Address::new(0x0001).unwrap()));
    }
}

//! Semantic reconstruction of EX3 C stack frames from sidecar debug metadata.

use crate::{
    cc::{DynamicStackSlotKind, FixedFrameState, FunctionDebugId, ScalarType, TemporaryRole},
    debug_info::{
        InstructionDebugInfo, LinkedFunctionDebugInfo, LinkedSymbolKind, ProgramDebugInfo,
    },
    emulator::Memory,
    isa::{Address, Word},
};
use std::{collections::HashSet, error::Error, fmt};

pub const DEFAULT_MAX_UNWIND_DEPTH: usize = 256;
pub const DEFAULT_RAW_STACK_WORDS: usize = 16;
pub const MAX_RAW_STACK_WORDS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonCContextHint {
    Interrupt,
    Assembly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackViewOptions {
    pub max_depth: usize,
    pub raw_stack_words: usize,
    pub non_c_context_hint: Option<NonCContextHint>,
}

impl Default for StackViewOptions {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_UNWIND_DEPTH,
            raw_stack_words: DEFAULT_RAW_STACK_WORDS,
            non_c_context_hint: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackViewContext {
    C {
        function_id: FunctionDebugId,
        function: String,
    },
    Runtime {
        symbol: String,
    },
    Startup,
    Assembly {
        symbol: Option<String>,
    },
    Interrupt,
    Unmapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackView {
    pub context: StackViewContext,
    pub pc: Address,
    pub sp: Address,
    pub frames: Vec<StackFrame>,
    pub raw_stack: Vec<RawStackWord>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawStackWord {
    pub address: Address,
    pub raw: Word,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameProgramCounterKind {
    Current,
    SuspendedReturn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    pub function_id: FunctionDebugId,
    pub function: String,
    pub current_sp: Address,
    pub frame_sp: Address,
    pub program_counter: SymbolicAddress,
    pub program_counter_kind: FrameProgramCounterKind,
    pub slots: Vec<StackSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicAddress {
    pub address: Address,
    pub symbol: Option<String>,
    pub offset: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackSlot {
    pub address: Address,
    pub frame_offset: i32,
    pub kind: StackSlotKind,
    pub display_name: String,
    pub ty: Option<ScalarType>,
    pub raw: Word,
    pub typed_value: Option<TypedStackValue>,
    pub value_status: StackValueStatus,
    pub return_target: Option<SymbolicAddress>,
}

impl StackSlot {
    pub fn raw_hex(&self) -> String {
        format!("0x{:08x}", self.raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackSlotKind {
    Parameter {
        index: u16,
    },
    ReturnAddress,
    Local {
        slot: u16,
    },
    Temporary {
        slot: u16,
        active: bool,
        role: Option<TemporaryRole>,
    },
    OutgoingArgument {
        callee: String,
        argument_index: u16,
        parameter_name: Option<String>,
    },
    RuntimeArgument {
        helper: String,
        argument_index: u16,
    },
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedStackValue {
    Signed(i32),
    Unsigned(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackValueStatus {
    Value,
    CurrentStorage,
    StaleScratch,
    NotAllocated,
    Released,
    Control,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackViewError {
    MissingFunction(FunctionDebugId),
    FrameOffsetOutOfRange(i32),
    SlotCountOutOfRange(usize),
}

impl fmt::Display for StackViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFunction(id) => {
                write!(
                    formatter,
                    "instruction references missing linked function {id:?}"
                )
            }
            Self::FrameOffsetOutOfRange(offset) => {
                write!(
                    formatter,
                    "frame offset {offset} exceeds signed 16-bit range"
                )
            }
            Self::SlotCountOutOfRange(count) => {
                write!(formatter, "stack slot count {count} exceeds i32")
            }
        }
    }
}

impl Error for StackViewError {}

pub fn build_stack_view(
    program: Option<&ProgramDebugInfo>,
    memory: &impl Memory,
    pc: Address,
    sp: Address,
    options: StackViewOptions,
) -> Result<StackView, StackViewError> {
    let Some(program) = program else {
        let context = match options.non_c_context_hint {
            Some(NonCContextHint::Interrupt) => StackViewContext::Interrupt,
            Some(NonCContextHint::Assembly) => StackViewContext::Assembly { symbol: None },
            None => StackViewContext::Unmapped,
        };
        return Ok(fallback_view(None, memory, pc, sp, options, context));
    };
    let Some(instruction) = program.instruction(pc) else {
        let context = classify_non_c_context(program, pc, options.non_c_context_hint);
        return Ok(fallback_view(
            Some(program),
            memory,
            pc,
            sp,
            options,
            context,
        ));
    };
    let current_function = program
        .function(instruction.function_id)
        .ok_or(StackViewError::MissingFunction(instruction.function_id))?;
    let context = StackViewContext::C {
        function_id: current_function.id,
        function: current_function.name.clone(),
    };
    let mut view = StackView {
        context,
        pc,
        sp,
        frames: Vec::new(),
        raw_stack: Vec::new(),
        warnings: Vec::new(),
    };
    let max_depth = options.max_depth.clamp(1, DEFAULT_MAX_UNWIND_DEPTH);
    if options.max_depth > DEFAULT_MAX_UNWIND_DEPTH {
        view.warnings.push(format!(
            "Requested unwind depth {} was limited to {DEFAULT_MAX_UNWIND_DEPTH}.",
            options.max_depth
        ));
    } else if options.max_depth == 0 {
        view.warnings
            .push("Requested unwind depth 0 was raised to 1.".into());
    }

    let mut frame_pc = pc;
    let mut frame_current_sp = sp;
    let mut frame_instruction = instruction;
    let mut frame_sp = add_signed(sp, instruction.frame_base_delta)?;
    let mut pc_kind = FrameProgramCounterKind::Current;
    let mut visited = HashSet::new();
    loop {
        let key = (frame_instruction.function_id, frame_sp);
        if !visited.insert(key) {
            view.warnings.push(format!(
                "Stopped stack unwind after revisiting frame {:?} at {}.",
                frame_instruction.function_id, frame_sp
            ));
            break;
        }
        let function = program.function(frame_instruction.function_id).ok_or(
            StackViewError::MissingFunction(frame_instruction.function_id),
        )?;
        append_metadata_warnings(&mut view.warnings, function, frame_instruction);
        let (frame, return_address) = build_frame(
            program,
            memory,
            function,
            FrameCursor {
                instruction: frame_instruction,
                pc: frame_pc,
                current_sp: frame_current_sp,
                frame_sp,
                pc_kind,
            },
        )?;
        let return_slot = add_signed(frame_sp, i32::from(function.frame_size))?;
        view.frames.push(frame);

        let Some(caller_instruction) = program.instruction(return_address) else {
            if is_normal_unwind_boundary(program, return_address) {
                break;
            }
            view.warnings.push(format!(
                "Unable to unwind beyond frame `{}`: return address {} has no C debug metadata.",
                function.name, return_address
            ));
            break;
        };
        if view.frames.len() >= max_depth {
            view.warnings.push(format!(
                "Stack unwind stopped at the maximum depth of {max_depth}."
            ));
            break;
        }
        let caller_resume_sp = return_slot.wrapping_add(1);
        let caller_frame_sp = add_signed(caller_resume_sp, caller_instruction.frame_base_delta)?;
        if visited.contains(&(caller_instruction.function_id, caller_frame_sp)) {
            view.warnings.push(format!(
                "Stopped stack unwind after detecting a cyclic frame at {caller_frame_sp}."
            ));
            break;
        }
        frame_pc = return_address;
        frame_current_sp = caller_resume_sp;
        frame_instruction = caller_instruction;
        frame_sp = caller_frame_sp;
        pc_kind = FrameProgramCounterKind::SuspendedReturn;
    }
    Ok(view)
}

fn is_normal_unwind_boundary(program: &ProgramDebugInfo, return_address: Address) -> bool {
    matches!(
        classify_non_c_context(program, return_address, None),
        StackViewContext::Startup
    )
}

#[derive(Clone, Copy)]
struct FrameCursor<'a> {
    instruction: &'a InstructionDebugInfo,
    pc: Address,
    current_sp: Address,
    frame_sp: Address,
    pc_kind: FrameProgramCounterKind,
}

fn build_frame(
    program: &ProgramDebugInfo,
    memory: &impl Memory,
    function: &LinkedFunctionDebugInfo,
    cursor: FrameCursor<'_>,
) -> Result<(StackFrame, Address), StackViewError> {
    let FrameCursor {
        instruction,
        pc,
        current_sp,
        frame_sp,
        pc_kind,
    } = cursor;
    let mut slots = Vec::with_capacity(
        function.parameters.len()
            + 1
            + function.locals.len()
            + usize::from(function.temporary_count)
            + instruction.dynamic_stack_slots.len(),
    );
    for parameter in &function.parameters {
        slots.push(read_slot(
            memory,
            frame_sp,
            parameter.frame_offset,
            StackSlotKind::Parameter {
                index: parameter.index,
            },
            parameter.name.clone(),
            Some(parameter.ty),
            StackValueStatus::Value,
            None,
        )?);
    }
    let return_offset = function.return_address.frame_offset;
    let return_address_slot = add_signed(frame_sp, return_offset)?;
    let return_raw = memory.read(return_address_slot);
    let return_address = Address::from_low16(return_raw);
    slots.push(StackSlot {
        address: return_address_slot,
        frame_offset: return_offset,
        kind: StackSlotKind::ReturnAddress,
        display_name: "return address".into(),
        ty: None,
        raw: return_raw,
        typed_value: None,
        value_status: StackValueStatus::Control,
        return_target: Some(symbolicate_nearest(program, return_address)),
    });
    for local in &function.locals {
        let value_status = fixed_storage_status(cursor.instruction.fixed_frame_state);
        slots.push(read_slot(
            memory,
            frame_sp,
            local.frame_offset,
            StackSlotKind::Local { slot: local.slot },
            local.name.clone(),
            Some(local.ty),
            value_status,
            None,
        )?);
    }
    for slot in 0..function.temporary_count {
        let active = instruction
            .active_temporaries
            .iter()
            .find(|temporary| temporary.slot == slot);
        let local_count = i32::try_from(function.locals.len())
            .map_err(|_| StackViewError::SlotCountOutOfRange(function.locals.len()))?;
        let offset = local_count + i32::from(slot);
        let value_status = match cursor.instruction.fixed_frame_state {
            FixedFrameState::NotAllocated => StackValueStatus::NotAllocated,
            FixedFrameState::Released => StackValueStatus::Released,
            FixedFrameState::Allocated if active.is_some() => StackValueStatus::Value,
            FixedFrameState::Allocated => StackValueStatus::StaleScratch,
        };
        slots.push(read_slot(
            memory,
            frame_sp,
            offset,
            StackSlotKind::Temporary {
                slot,
                active: active.is_some(),
                role: active.map(|temporary| temporary.role),
            },
            active.map_or_else(
                || format!("temporary #{slot} (inactive / scratch)"),
                |temporary| temporary.display_name.clone(),
            ),
            None,
            value_status,
            None,
        )?);
    }
    for dynamic in &instruction.dynamic_stack_slots {
        let (kind, value_status) = match &dynamic.kind {
            DynamicStackSlotKind::OutgoingArgument {
                callee,
                argument_index,
                parameter_name,
            } => (
                StackSlotKind::OutgoingArgument {
                    callee: callee.clone(),
                    argument_index: *argument_index,
                    parameter_name: parameter_name.clone(),
                },
                StackValueStatus::Value,
            ),
            DynamicStackSlotKind::RuntimeArgument {
                helper,
                argument_index,
            } => (
                StackSlotKind::RuntimeArgument {
                    helper: helper.clone(),
                    argument_index: *argument_index,
                },
                StackValueStatus::Value,
            ),
            DynamicStackSlotKind::Other => (StackSlotKind::Unknown, StackValueStatus::Unknown),
        };
        slots.push(read_slot(
            memory,
            frame_sp,
            dynamic.frame_offset,
            kind,
            dynamic.display_name.clone(),
            dynamic.ty,
            value_status,
            None,
        )?);
    }
    Ok((
        StackFrame {
            function_id: function.id,
            function: function.name.clone(),
            current_sp,
            frame_sp,
            program_counter: symbolicate_nearest(program, pc),
            program_counter_kind: pc_kind,
            slots,
        },
        return_address,
    ))
}

fn fixed_storage_status(frame_state: FixedFrameState) -> StackValueStatus {
    match frame_state {
        FixedFrameState::NotAllocated => StackValueStatus::NotAllocated,
        FixedFrameState::Allocated => StackValueStatus::CurrentStorage,
        FixedFrameState::Released => StackValueStatus::Released,
    }
}

fn append_metadata_warnings(
    warnings: &mut Vec<String>,
    function: &LinkedFunctionDebugInfo,
    instruction: &InstructionDebugInfo,
) {
    if function.return_address.frame_offset != i32::from(function.frame_size) {
        warnings.push(format!(
            "Frame `{}` has inconsistent return-address metadata.",
            function.name
        ));
    }
    for temporary in &instruction.active_temporaries {
        if temporary.slot >= function.temporary_count {
            warnings.push(format!(
                "Frame `{}` marks out-of-range temporary #{} active.",
                function.name, temporary.slot
            ));
        }
    }
    for dynamic in &instruction.dynamic_stack_slots {
        if dynamic.frame_offset >= 0 {
            warnings.push(format!(
                "Frame `{}` has dynamic slot `{}` at non-negative offset {}.",
                function.name, dynamic.display_name, dynamic.frame_offset
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn read_slot(
    memory: &impl Memory,
    frame_sp: Address,
    frame_offset: i32,
    kind: StackSlotKind,
    display_name: String,
    ty: Option<ScalarType>,
    value_status: StackValueStatus,
    return_target: Option<SymbolicAddress>,
) -> Result<StackSlot, StackViewError> {
    let address = add_signed(frame_sp, frame_offset)?;
    let raw = memory.read(address);
    let typed_value = if matches!(
        value_status,
        StackValueStatus::StaleScratch
            | StackValueStatus::NotAllocated
            | StackValueStatus::Released
            | StackValueStatus::Control
            | StackValueStatus::Unknown
    ) {
        None
    } else {
        ty.map(|ty| typed_value(ty, raw))
    };
    Ok(StackSlot {
        address,
        frame_offset,
        kind,
        display_name,
        ty,
        raw,
        typed_value,
        value_status,
        return_target,
    })
}

fn typed_value(ty: ScalarType, raw: Word) -> TypedStackValue {
    match ty {
        ScalarType::Int32 => TypedStackValue::Signed(raw as i32),
        ScalarType::UInt32 => TypedStackValue::Unsigned(raw),
    }
}

fn add_signed(address: Address, offset: i32) -> Result<Address, StackViewError> {
    let offset =
        i16::try_from(offset).map_err(|_| StackViewError::FrameOffsetOutOfRange(offset))?;
    Ok(address.wrapping_add_signed(offset))
}

fn symbolicate_nearest(program: &ProgramDebugInfo, address: Address) -> SymbolicAddress {
    if let Some(instruction) = program.instruction(address) {
        if let Some(function) = program.function(instruction.function_id) {
            return SymbolicAddress {
                address,
                symbol: Some(function.name.clone()),
                offset: Some(address.get().wrapping_sub(function.address_start.get())),
            };
        }
    }
    let symbol = program.nearest_symbol(address);
    SymbolicAddress {
        address,
        symbol: symbol.as_ref().map(|symbol| symbol.name.clone()),
        offset: symbol.map(|symbol| symbol.offset),
    }
}

fn classify_non_c_context(
    program: &ProgramDebugInfo,
    pc: Address,
    hint: Option<NonCContextHint>,
) -> StackViewContext {
    if hint == Some(NonCContextHint::Interrupt) {
        return StackViewContext::Interrupt;
    }
    if let Some(symbol) = program.symbol_at(pc) {
        match symbol.kind {
            LinkedSymbolKind::Runtime => {
                return StackViewContext::Runtime {
                    symbol: symbol.name.clone(),
                };
            }
            LinkedSymbolKind::Assembly => {
                return StackViewContext::Assembly {
                    symbol: Some(symbol.name.clone()),
                };
            }
            LinkedSymbolKind::CFunction | LinkedSymbolKind::Data | LinkedSymbolKind::Other => {}
        }
    }
    let first_function = program
        .functions
        .iter()
        .map(|function| function.address_start)
        .min();
    if pc >= Address::RESET
        && first_function.is_some_and(|start| pc < start)
        && program.is_executable(pc)
    {
        return StackViewContext::Startup;
    }
    if hint == Some(NonCContextHint::Assembly) {
        StackViewContext::Assembly {
            symbol: program.nearest_symbol(pc).map(|symbol| symbol.name),
        }
    } else {
        StackViewContext::Unmapped
    }
}

fn fallback_view(
    program: Option<&ProgramDebugInfo>,
    memory: &impl Memory,
    pc: Address,
    sp: Address,
    options: StackViewOptions,
    context: StackViewContext,
) -> StackView {
    let raw_count = options.raw_stack_words.min(MAX_RAW_STACK_WORDS);
    let raw_stack = (0..raw_count)
        .map(|offset| {
            let address =
                sp.wrapping_add(u16::try_from(offset).expect("raw stack limit exceeds u16"));
            RawStackWord {
                address,
                raw: memory.read(address),
            }
        })
        .collect();
    let known = program.is_some();
    StackView {
        context,
        pc,
        sp,
        frames: Vec::new(),
        raw_stack,
        warnings: vec![format!(
            "Semantic C frame unavailable at PC {pc}{}.",
            if known { "" } else { " (no debug metadata)" }
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assembler::Assembler,
        cc::{self, CompilerDebugInfo},
        debug_info::link_program_debug_info,
        emulator::{ArrayMemory, Cpu, NullIoBus, StepOutcome},
        isa::{decode, ImmediateOp, Instruction},
    };
    use std::cell::Cell;

    fn compile_program(source: &str) -> (ProgramDebugInfo, ArrayMemory) {
        let compilation = cc::compile_with_debug_info(source).unwrap();
        let assembled = Assembler::new().assemble(&compilation.assembly).unwrap();
        let debug = link_program_debug_info(&compilation.debug_info, &assembled).unwrap();
        (debug, ArrayMemory::from_image(&assembled.image))
    }

    fn mapped_address(
        program: &ProgramDebugInfo,
        function: &str,
        predicate: impl Fn(&InstructionDebugInfo) -> bool,
    ) -> Address {
        let id = program
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap()
            .id;
        program
            .instructions
            .iter()
            .find(|(_, instruction)| instruction.function_id == id && predicate(instruction))
            .map(|(address, _)| *address)
            .unwrap()
    }

    fn slot(frame: &StackFrame, predicate: impl Fn(&StackSlotKind) -> bool) -> &StackSlot {
        frame
            .slots
            .iter()
            .find(|slot| predicate(&slot.kind))
            .unwrap()
    }

    fn fixed_slot_addresses(frame: &StackFrame) -> Vec<(StackSlotKind, Address)> {
        frame
            .slots
            .iter()
            .filter(|slot| {
                !matches!(
                    slot.kind,
                    StackSlotKind::OutgoingArgument { .. }
                        | StackSlotKind::RuntimeArgument { .. }
                        | StackSlotKind::Unknown
                )
            })
            .map(|slot| (slot.kind.clone(), slot.address))
            .collect()
    }

    fn assert_push_boundaries(
        source: &str,
        function_name: &str,
        call_target: &str,
        minimum_frame_depth: usize,
    ) {
        let compilation = cc::compile_with_debug_info(source).unwrap();
        let assembled = Assembler::new().assemble(&compilation.assembly).unwrap();
        let program = link_program_debug_info(&compilation.debug_info, &assembled).unwrap();
        let function_id = program
            .functions
            .iter()
            .find(|function| function.name == function_name)
            .unwrap()
            .id;
        let call_line = compilation
            .debug_info
            .assembly_lines
            .iter()
            .find(|state| {
                state.function_id == function_id
                    && compilation
                        .assembly
                        .lines()
                        .nth(state.assembly_line as usize - 1)
                        .is_some_and(|line| line.trim() == format!("CALL {call_target}"))
            })
            .unwrap()
            .assembly_line;
        let push_line = call_line - 1;
        assert_eq!(
            compilation
                .assembly
                .lines()
                .nth(push_line as usize - 1)
                .unwrap()
                .trim(),
            "PUSH"
        );
        let push_addresses = assembled
            .source_map
            .iter()
            .filter(|entry| entry.span.line == push_line as usize)
            .map(|entry| entry.address)
            .collect::<Vec<_>>();
        assert_eq!(push_addresses.len(), 2);

        let mut memory = ArrayMemory::from_image(&assembled.image);
        let mut cpu = Cpu::new();
        let mut io = NullIoBus;
        let before = (0..10_000)
            .find_map(|_| {
                if cpu.state().pc == push_addresses[0] {
                    let view = build_stack_view(
                        Some(&program),
                        &memory,
                        cpu.state().pc,
                        cpu.state().sp,
                        StackViewOptions::default(),
                    )
                    .unwrap();
                    if view.frames.len() >= minimum_frame_depth {
                        return Some(view);
                    }
                }
                assert!(!matches!(
                    cpu.step(&mut memory, &mut io).unwrap(),
                    StepOutcome::Halted
                ));
                None
            })
            .expect("selected PUSH was not reached");
        let frame_sp = before.frames[0].frame_sp;
        let fixed_addresses = fixed_slot_addresses(&before.frames[0]);
        let dynamic_count = before.frames[0]
            .slots
            .iter()
            .filter(|slot| {
                matches!(
                    slot.kind,
                    StackSlotKind::OutgoingArgument { .. } | StackSlotKind::RuntimeArgument { .. }
                )
            })
            .count();

        cpu.step(&mut memory, &mut io).unwrap();
        assert_eq!(cpu.state().pc, push_addresses[1]);
        let before_store = build_stack_view(
            Some(&program),
            &memory,
            cpu.state().pc,
            cpu.state().sp,
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(before_store.frames[0].frame_sp, frame_sp);
        assert_eq!(
            fixed_slot_addresses(&before_store.frames[0]),
            fixed_addresses
        );
        assert_eq!(before_store.frames.len(), before.frames.len());
        assert!(before.warnings.is_empty());
        assert!(before_store.warnings.is_empty());
        assert_eq!(
            before_store.frames[0]
                .slots
                .iter()
                .filter(|slot| {
                    matches!(
                        slot.kind,
                        StackSlotKind::OutgoingArgument { .. }
                            | StackSlotKind::RuntimeArgument { .. }
                    )
                })
                .count(),
            dynamic_count + 1
        );

        cpu.step(&mut memory, &mut io).unwrap();
        let after = build_stack_view(
            Some(&program),
            &memory,
            cpu.state().pc,
            cpu.state().sp,
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(after.frames[0].frame_sp, frame_sp);
        assert_eq!(fixed_slot_addresses(&after.frames[0]), fixed_addresses);
        assert_eq!(after.frames.len(), before.frames.len());
        assert!(after.warnings.is_empty());
        assert_eq!(
            after.frames[0]
                .slots
                .iter()
                .filter(|slot| {
                    matches!(
                        slot.kind,
                        StackSlotKind::OutgoingArgument { .. }
                            | StackSlotKind::RuntimeArgument { .. }
                    )
                })
                .count(),
            dynamic_count + 1
        );
    }

    #[test]
    fn reconstructs_fixed_slots_addresses_and_typed_values() {
        let (program, mut memory) = compile_program(
            r#"
                int inspect(int signed_value, unsigned int unsigned_value) {
                    int local;
                    local = signed_value;
                    return local + unsigned_value;
                }
                int main(void) { return inspect(-1, 0xffffffffu); }
            "#,
        );
        let inspect = program
            .functions
            .iter()
            .find(|function| function.name == "inspect")
            .unwrap();
        let pc = mapped_address(&program, "inspect", |state| {
            state.frame_base_delta == 0 && !state.active_temporaries.is_empty()
        });
        let frame_sp = Address::new(0xff00).unwrap();
        let main_pc = mapped_address(&program, "main", |_| true);
        memory.write(frame_sp.wrapping_add(0), u32::MAX);
        memory.write(frame_sp.wrapping_add(1), 0x1234_5678);
        memory.write(frame_sp.wrapping_add(2), 0x8765_4321);
        memory.write(
            frame_sp.wrapping_add(inspect.frame_size),
            u32::from(main_pc.get()),
        );
        memory.write(frame_sp.wrapping_add(inspect.frame_size + 1), u32::MAX);
        memory.write(frame_sp.wrapping_add(inspect.frame_size + 2), u32::MAX);

        let view = build_stack_view(
            Some(&program),
            &memory,
            pc,
            frame_sp,
            StackViewOptions::default(),
        )
        .unwrap();
        let frame = &view.frames[0];
        assert_eq!(frame.function, "inspect");
        assert_eq!(frame.current_sp, frame_sp);
        assert_eq!(frame.frame_sp, frame_sp);
        assert_eq!(frame.program_counter_kind, FrameProgramCounterKind::Current);

        let signed = slot(frame, |kind| {
            matches!(kind, StackSlotKind::Parameter { index: 0 })
        });
        assert_eq!(
            signed.address,
            frame_sp.wrapping_add(inspect.frame_size + 1)
        );
        assert_eq!(signed.typed_value, Some(TypedStackValue::Signed(-1)));
        assert_eq!(signed.raw_hex(), "0xffffffff");
        let unsigned = slot(frame, |kind| {
            matches!(kind, StackSlotKind::Parameter { index: 1 })
        });
        assert_eq!(
            unsigned.typed_value,
            Some(TypedStackValue::Unsigned(u32::MAX))
        );
        let local = slot(frame, |kind| {
            matches!(kind, StackSlotKind::Local { slot: 0 })
        });
        assert_eq!(local.address, frame_sp);
        assert_eq!(local.value_status, StackValueStatus::CurrentStorage);
        let return_slot = slot(frame, |kind| matches!(kind, StackSlotKind::ReturnAddress));
        assert_eq!(
            return_slot.address,
            frame_sp.wrapping_add(inspect.frame_size)
        );
        assert_eq!(return_slot.raw, u32::from(main_pc.get()));
        assert_eq!(
            return_slot
                .return_target
                .as_ref()
                .unwrap()
                .symbol
                .as_deref(),
            Some("main")
        );
        assert!(return_slot.return_target.as_ref().unwrap().offset.is_some());

        let temporaries = frame
            .slots
            .iter()
            .filter(|slot| matches!(slot.kind, StackSlotKind::Temporary { .. }))
            .collect::<Vec<_>>();
        assert_eq!(temporaries.len(), usize::from(inspect.temporary_count));
        assert!(temporaries
            .iter()
            .any(|slot| slot.value_status == StackValueStatus::Value));
        assert!(temporaries.iter().any(|slot| {
            slot.value_status == StackValueStatus::StaleScratch && slot.typed_value.is_none()
        }));
    }

    #[test]
    fn reconstructs_outgoing_arguments_in_logical_and_physical_order() {
        let (program, mut memory) = compile_program(
            r#"
                int target(int first, int second, int third) { return first + second + third; }
                int main(void) { return target(10, 20, 30); }
            "#,
        );
        let call_pc = mapped_address(&program, "main", |state| {
            state.dynamic_stack_slots.len() == 3
        });
        let state = program.instruction(call_pc).unwrap();
        assert_eq!(state.frame_base_delta, 3);
        let frame_sp = Address::new(0xf000).unwrap();
        let current_sp = frame_sp.wrapping_add_signed(-3);
        memory.write(frame_sp.wrapping_add_signed(-1), 30);
        memory.write(frame_sp.wrapping_add_signed(-2), 20);
        memory.write(frame_sp.wrapping_add_signed(-3), 10);
        memory.write(frame_sp, 0x0011);

        let view = build_stack_view(
            Some(&program),
            &memory,
            call_pc,
            current_sp,
            StackViewOptions::default(),
        )
        .unwrap();
        let dynamic = view.frames[0]
            .slots
            .iter()
            .filter_map(|slot| match &slot.kind {
                StackSlotKind::OutgoingArgument {
                    argument_index,
                    parameter_name,
                    ..
                } => Some((
                    slot.frame_offset,
                    *argument_index,
                    parameter_name.as_deref(),
                    slot.raw,
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            dynamic,
            [
                (-1, 2, Some("third"), 30),
                (-2, 1, Some("second"), 20),
                (-3, 0, Some("first"), 10),
            ]
        );

        let partial_pc = mapped_address(&program, "main", |state| {
            state.dynamic_stack_slots.len() == 1
        });
        let partial = build_stack_view(
            Some(&program),
            &memory,
            partial_pc,
            frame_sp.wrapping_add_signed(-1),
            StackViewOptions::default(),
        )
        .unwrap();
        let partial_arguments = partial.frames[0]
            .slots
            .iter()
            .filter_map(|slot| match &slot.kind {
                StackSlotKind::OutgoingArgument { argument_index, .. } => {
                    Some((*argument_index, slot.address, slot.raw))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            partial_arguments,
            [(2, frame_sp.wrapping_add_signed(-1), 30)]
        );
    }

    #[test]
    fn preserves_frame_across_single_argument_push_in_a_nested_call() {
        assert_push_boundaries(
            r#"
                int id(int value) { return value; }
                int caller(int seed) { int local; local = seed; return id(local); }
                int main(void) { return caller(7); }
            "#,
            "caller",
            "id",
            2,
        );
    }

    #[test]
    fn preserves_frame_across_multiple_argument_pushes() {
        assert_push_boundaries(
            r#"
                int sum3(int a, int b, int c) { return a + b + c; }
                int main(void) { return sum3(1, 2, 3); }
            "#,
            "main",
            "sum3",
            1,
        );
    }

    #[test]
    fn preserves_recursive_call_stack_across_argument_push() {
        assert_push_boundaries(
            r#"
                int fact(int n) {
                    if (n <= 1) return 1;
                    return n * fact(n - 1);
                }
                int main(void) { return fact(4); }
            "#,
            "fact",
            "fact",
            3,
        );
    }

    #[test]
    fn preserves_frame_across_runtime_helper_argument_push() {
        assert_push_boundaries(
            "int main(void) { return 6 * 7; }",
            "main",
            "__ex3_mul_i32",
            1,
        );
    }

    #[test]
    fn fixed_slots_follow_prologue_and_epilogue_lifetime() {
        let compilation = cc::compile_with_debug_info(
            r#"
                int target(int parameter) {
                    int local;
                    local = parameter;
                    return local + 2;
                }
                int main(void) { return target(1); }
            "#,
        )
        .unwrap();
        let assembled = Assembler::new().assemble(&compilation.assembly).unwrap();
        let program = link_program_debug_info(&compilation.debug_info, &assembled).unwrap();
        let function = program
            .functions
            .iter()
            .find(|function| function.name == "target")
            .unwrap();
        assert!(function.frame_size > 0);
        let epilogue = program
            .instructions
            .keys()
            .copied()
            .find(|address| {
                matches!(
                    decode(assembled.image.cells.iter().find(|cell| cell.address == *address).unwrap().word),
                    Ok(Instruction::Immediate { op: ImmediateOp::Adjsp, value })
                        if value.as_i16() == function.frame_size as i16
                ) && matches!(
                    assembled
                        .image
                        .cells
                        .iter()
                        .find(|cell| cell.address == address.wrapping_add(1))
                        .map(|cell| decode(cell.word)),
                    Some(Ok(Instruction::System(crate::isa::SystemOp::Ret)))
                )
            })
            .expect("epilogue ADJSP was not found");
        let mut memory = ArrayMemory::from_image(&assembled.image);
        let mut cpu = Cpu::new();
        let mut io = NullIoBus;
        while cpu.state().pc != function.address_start {
            cpu.step(&mut memory, &mut io).unwrap();
        }

        let entry = build_stack_view(
            Some(&program),
            &memory,
            cpu.state().pc,
            cpu.state().sp,
            StackViewOptions::default(),
        )
        .unwrap();
        let entry_fixed = entry.frames[0]
            .slots
            .iter()
            .filter(|slot| {
                matches!(
                    slot.kind,
                    StackSlotKind::Local { .. } | StackSlotKind::Temporary { .. }
                )
            })
            .collect::<Vec<_>>();
        assert!(!entry_fixed.is_empty());
        assert!(entry_fixed
            .iter()
            .all(|slot| slot.value_status == StackValueStatus::NotAllocated
                && slot.typed_value.is_none()));

        cpu.step(&mut memory, &mut io).unwrap();
        let body = build_stack_view(
            Some(&program),
            &memory,
            cpu.state().pc,
            cpu.state().sp,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(body.frames[0].slots.iter().any(|slot| {
            matches!(slot.kind, StackSlotKind::Local { .. })
                && slot.value_status == StackValueStatus::CurrentStorage
        }));

        while cpu.state().pc != epilogue {
            cpu.step(&mut memory, &mut io).unwrap();
        }
        let before_release = build_stack_view(
            Some(&program),
            &memory,
            cpu.state().pc,
            cpu.state().sp,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(before_release.frames[0].slots.iter().any(|slot| {
            matches!(slot.kind, StackSlotKind::Local { .. })
                && slot.value_status == StackValueStatus::CurrentStorage
        }));

        cpu.step(&mut memory, &mut io).unwrap();
        let released = build_stack_view(
            Some(&program),
            &memory,
            cpu.state().pc,
            cpu.state().sp,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(released.frames[0]
            .slots
            .iter()
            .filter(|slot| matches!(
                slot.kind,
                StackSlotKind::Local { .. } | StackSlotKind::Temporary { .. }
            ))
            .all(|slot| slot.value_status == StackValueStatus::Released
                && slot.typed_value.is_none()));

        let entry_control = entry.frames[0]
            .slots
            .iter()
            .filter(|slot| {
                matches!(
                    slot.kind,
                    StackSlotKind::Parameter { .. } | StackSlotKind::ReturnAddress
                )
            })
            .map(|slot| (slot.kind.clone(), slot.address))
            .collect::<Vec<_>>();
        assert_eq!(
            entry.frames[0]
                .slots
                .iter()
                .find(|slot| matches!(slot.kind, StackSlotKind::Parameter { index: 0 }))
                .unwrap()
                .typed_value,
            Some(TypedStackValue::Signed(1))
        );
        let released_control = released.frames[0]
            .slots
            .iter()
            .filter(|slot| {
                matches!(
                    slot.kind,
                    StackSlotKind::Parameter { .. } | StackSlotKind::ReturnAddress
                )
            })
            .map(|slot| (slot.kind.clone(), slot.address))
            .collect::<Vec<_>>();
        assert_eq!(entry_control, released_control);
    }

    #[test]
    fn zero_sized_frames_are_always_allocated() {
        let (program, _) = compile_program("int main(void) { return 0; }");
        let main = program
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap();
        assert_eq!(main.frame_size, 0);
        assert!(program
            .instructions
            .values()
            .filter(|instruction| instruction.function_id == main.id)
            .all(|instruction| instruction.fixed_frame_state == FixedFrameState::Allocated));
    }

    #[test]
    fn main_to_startup_is_a_normal_unwind_boundary() {
        let (program, mut memory) = compile_program("int main(void) { return 0; }");
        let mut cpu = Cpu::new();
        let mut io = NullIoBus;
        let view = (0..100)
            .find_map(|_| {
                let view = build_stack_view(
                    Some(&program),
                    &memory,
                    cpu.state().pc,
                    cpu.state().sp,
                    StackViewOptions::default(),
                )
                .unwrap();
                if view
                    .frames
                    .first()
                    .is_some_and(|frame| frame.function == "main")
                {
                    Some(view)
                } else {
                    assert!(!matches!(
                        cpu.step(&mut memory, &mut io).unwrap(),
                        StepOutcome::Halted
                    ));
                    None
                }
            })
            .expect("main was not entered");

        assert_eq!(view.frames.len(), 1);
        assert_eq!(view.frames[0].function, "main");
        assert!(
            view.warnings.is_empty(),
            "unexpected warnings: {:?}",
            view.warnings
        );
    }

    #[test]
    fn nested_c_frames_unwind_to_startup_without_a_warning() {
        let (program, mut memory) = compile_program(
            r#"
                int foo(void) { return 42; }
                int main(void) { return foo(); }
            "#,
        );
        let mut cpu = Cpu::new();
        let mut io = NullIoBus;
        let view = (0..100)
            .find_map(|_| {
                let view = build_stack_view(
                    Some(&program),
                    &memory,
                    cpu.state().pc,
                    cpu.state().sp,
                    StackViewOptions::default(),
                )
                .unwrap();
                let functions = view
                    .frames
                    .iter()
                    .map(|frame| frame.function.as_str())
                    .collect::<Vec<_>>();
                if functions == ["foo", "main"] {
                    Some(view)
                } else {
                    assert!(!matches!(
                        cpu.step(&mut memory, &mut io).unwrap(),
                        StepOutcome::Halted
                    ));
                    None
                }
            })
            .expect("foo and main frames were not reconstructed");

        assert!(
            view.warnings.is_empty(),
            "unexpected warnings: {:?}",
            view.warnings
        );
    }

    #[test]
    fn unknown_return_address_remains_an_unwind_warning() {
        let (program, mut memory) = compile_program(
            r#"
                int foo(void) { return 42; }
                int main(void) { return foo(); }
            "#,
        );
        let pc = mapped_address(&program, "foo", |state| state.frame_base_delta == 0);
        let frame_size = program
            .functions
            .iter()
            .find(|function| function.name == "foo")
            .unwrap()
            .frame_size;
        let unknown_return = (0..=u16::MAX)
            .map(|raw| Address::new(raw).unwrap())
            .find(|address| {
                program.instruction(*address).is_none()
                    && matches!(
                        classify_non_c_context(&program, *address, None),
                        StackViewContext::Unmapped
                    )
            })
            .expect("program has no unmapped address");
        let frame_sp = Address::new(0x4000).unwrap();
        memory.write(
            frame_sp.wrapping_add(frame_size),
            u32::from(unknown_return.get()),
        );

        let view = build_stack_view(
            Some(&program),
            &memory,
            pc,
            frame_sp,
            StackViewOptions::default(),
        )
        .unwrap();

        assert!(view
            .warnings
            .iter()
            .any(|warning| warning.contains("no C debug metadata")));
    }

    #[test]
    fn distinguishes_runtime_arguments_from_user_arguments() {
        let (program, mut memory) = compile_program("int main(void) { return 2 * 3; }");
        let call_pc = mapped_address(&program, "main", |state| {
            state.dynamic_stack_slots.len() == 2
        });
        let frame_sp = Address::new(0xe000).unwrap();
        memory.write(frame_sp.wrapping_add_signed(-1), 3);
        memory.write(frame_sp.wrapping_add_signed(-2), 2);
        memory.write(frame_sp, 0x0011);
        let view = build_stack_view(
            Some(&program),
            &memory,
            call_pc,
            frame_sp.wrapping_add_signed(-2),
            StackViewOptions::default(),
        )
        .unwrap();
        let runtime = view.frames[0]
            .slots
            .iter()
            .filter_map(|slot| match &slot.kind {
                StackSlotKind::RuntimeArgument {
                    helper,
                    argument_index,
                } => Some((helper.as_str(), *argument_index, slot.raw)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(runtime, [("__ex3_mul_i32", 1, 3), ("__ex3_mul_i32", 0, 2)]);
        assert!(!view.frames[0]
            .slots
            .iter()
            .any(|slot| matches!(slot.kind, StackSlotKind::OutgoingArgument { .. })));
    }

    #[test]
    fn unwinds_factorial_recursion_and_preserves_each_parameter() {
        let (program, mut memory) = compile_program(
            r#"
                int fact(int n) {
                    if (n <= 1) return 1;
                    return n * fact(n - 1);
                }
                int main(void) { return fact(5); }
            "#,
        );
        let mut cpu = Cpu::new();
        let mut io = NullIoBus;
        let view = (0..10_000)
            .find_map(|_| {
                let view = build_stack_view(
                    Some(&program),
                    &memory,
                    cpu.state().pc,
                    cpu.state().sp,
                    StackViewOptions::default(),
                )
                .unwrap();
                if view.frames.len() == 6 {
                    Some(view)
                } else {
                    assert!(!matches!(
                        cpu.step(&mut memory, &mut io).unwrap(),
                        StepOutcome::Halted
                    ));
                    None
                }
            })
            .expect("factorial did not reach six nested C frames");

        assert_eq!(
            view.frames
                .iter()
                .map(|frame| frame.function.as_str())
                .collect::<Vec<_>>(),
            ["fact", "fact", "fact", "fact", "fact", "main"]
        );
        let values = view.frames[..5]
            .iter()
            .map(|frame| {
                slot(frame, |kind| {
                    matches!(kind, StackSlotKind::Parameter { index: 0 })
                })
                .typed_value
            })
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            [1, 2, 3, 4, 5].map(|value| Some(TypedStackValue::Signed(value)))
        );
        assert_eq!(
            view.frames[0].program_counter_kind,
            FrameProgramCounterKind::Current
        );
        assert!(view.frames[1..].iter().all(|frame| {
            frame.program_counter_kind == FrameProgramCounterKind::SuspendedReturn
                && frame.program_counter.symbol.is_some()
        }));
        assert!(
            view.warnings.is_empty(),
            "unexpected warnings: {:?}",
            view.warnings
        );
    }

    #[test]
    fn distinguishes_degraded_contexts_and_returns_bounded_raw_stack() {
        let (program, memory) = compile_program("int main(void) { return 2 * 3; }");
        let options = StackViewOptions {
            raw_stack_words: usize::MAX,
            ..StackViewOptions::default()
        };
        let startup = build_stack_view(
            Some(&program),
            &memory,
            Address::RESET,
            Address::ZERO,
            options,
        )
        .unwrap();
        assert_eq!(startup.context, StackViewContext::Startup);
        assert_eq!(startup.raw_stack.len(), MAX_RAW_STACK_WORDS);
        assert!(!startup.warnings.is_empty());
        let runtime_pc = program.symbol_address("__ex3_mul_i32").unwrap();
        let runtime = build_stack_view(
            Some(&program),
            &memory,
            runtime_pc,
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(matches!(runtime.context, StackViewContext::Runtime { .. }));
        let runtime_inside = build_stack_view(
            Some(&program),
            &memory,
            runtime_pc.wrapping_add(1),
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(matches!(
            runtime_inside.context,
            StackViewContext::Runtime { .. }
        ));

        let assembled = Assembler::new()
            .assemble("ORG 0x0200\nhandler:\nHLT\nEND\n")
            .unwrap();
        let assembly_program =
            link_program_debug_info(&CompilerDebugInfo::default(), &assembled).unwrap();
        let assembly_memory = ArrayMemory::from_image(&assembled.image);
        let assembly = build_stack_view(
            Some(&assembly_program),
            &assembly_memory,
            Address::new(0x0200).unwrap(),
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(matches!(
            assembly.context,
            StackViewContext::Assembly { .. }
        ));
        let outside_assembly = build_stack_view(
            Some(&assembly_program),
            &assembly_memory,
            Address::new(0x0201).unwrap(),
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(outside_assembly.context, StackViewContext::Unmapped);
        let interrupt = build_stack_view(
            Some(&assembly_program),
            &assembly_memory,
            Address::new(0x0200).unwrap(),
            Address::ZERO,
            StackViewOptions {
                non_c_context_hint: Some(NonCContextHint::Interrupt),
                ..StackViewOptions::default()
            },
        )
        .unwrap();
        assert_eq!(interrupt.context, StackViewContext::Interrupt);
        let unmapped = build_stack_view(
            None,
            &assembly_memory,
            Address::ZERO,
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(unmapped.context, StackViewContext::Unmapped);
    }

    #[test]
    fn nearest_runtime_symbol_does_not_classify_an_out_of_range_pc() {
        let assembled = Assembler::new()
            .assemble("ORG 0x0100\n__ex3_fake:\nHLT\nORG 0x0180\ndata: HEX 00000000\nEND\n")
            .unwrap();
        let program = link_program_debug_info(&CompilerDebugInfo::default(), &assembled).unwrap();
        let memory = ArrayMemory::from_image(&assembled.image);
        let outside = Address::new(0x0101).unwrap();

        assert_eq!(program.nearest_symbol(outside).unwrap().name, "__ex3_fake");
        assert!(program.symbol_at(outside).is_none());
        let view = build_stack_view(
            Some(&program),
            &memory,
            outside,
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(view.context, StackViewContext::Unmapped);

        let data = Address::new(0x0180).unwrap();
        assert_eq!(
            program.symbol_at(data).unwrap().kind,
            LinkedSymbolKind::Data
        );
        let view = build_stack_view(
            Some(&program),
            &memory,
            data,
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(view.context, StackViewContext::Unmapped);
    }

    #[test]
    fn reports_cycles_and_enforces_the_unwind_depth_limit() {
        let (mut program, mut memory) = compile_program("int main(void) { return 0; }");
        let pc = mapped_address(&program, "main", |_| true);
        program.instructions.get_mut(&pc).unwrap().frame_base_delta = -1;
        let sp = Address::new(0x1001).unwrap();
        memory.write(Address::new(0x1000).unwrap(), u32::from(pc.get()));
        let cyclic =
            build_stack_view(Some(&program), &memory, pc, sp, StackViewOptions::default()).unwrap();
        assert_eq!(cyclic.frames.len(), 1);
        assert!(cyclic
            .warnings
            .iter()
            .any(|warning| warning.contains("cyclic")));

        program.instructions.get_mut(&pc).unwrap().frame_base_delta = 0;
        let sp = Address::new(0x2000).unwrap();
        for offset in 0..4 {
            memory.write(sp.wrapping_add(offset), u32::from(pc.get()));
        }
        let limited = build_stack_view(
            Some(&program),
            &memory,
            pc,
            sp,
            StackViewOptions {
                max_depth: 3,
                ..StackViewOptions::default()
            },
        )
        .unwrap();
        assert_eq!(limited.frames.len(), 3);
        assert!(limited
            .warnings
            .iter()
            .any(|warning| warning.contains("maximum depth of 3")));
    }

    #[test]
    fn warns_for_metadata_mismatches_and_errors_for_broken_invariants() {
        let (mut program, mut memory) = compile_program("int main(void) { return 0; }");
        let pc = mapped_address(&program, "main", |state| state.frame_base_delta == 0);
        let sp = Address::new(0x2800).unwrap();
        program.functions[0].return_address.frame_offset = 1;
        memory.write(sp.wrapping_add(1), 0x0011);
        let view =
            build_stack_view(Some(&program), &memory, pc, sp, StackViewOptions::default()).unwrap();
        assert!(view
            .warnings
            .iter()
            .any(|warning| warning.contains("inconsistent return-address metadata")));

        program.functions.clear();
        assert!(matches!(
            build_stack_view(Some(&program), &memory, pc, sp, StackViewOptions::default()),
            Err(StackViewError::MissingFunction(_))
        ));
    }

    struct CountingMemory {
        inner: ArrayMemory,
        reads: Cell<usize>,
    }

    impl Memory for CountingMemory {
        fn read(&self, address: Address) -> Word {
            self.reads.set(self.reads.get() + 1);
            self.inner.read(address)
        }

        fn write(&mut self, address: Address, value: Word) {
            self.inner.write(address, value);
        }
    }

    #[test]
    fn reads_only_described_slots_or_the_bounded_fallback_window() {
        let (program, inner) = compile_program("int main(void) { return 0; }");
        let pc = mapped_address(&program, "main", |state| state.frame_base_delta == 0);
        let memory = CountingMemory {
            inner,
            reads: Cell::new(0),
        };
        let view = build_stack_view(
            Some(&program),
            &memory,
            pc,
            Address::new(0x3000).unwrap(),
            StackViewOptions::default(),
        )
        .unwrap();
        assert_eq!(view.frames.len(), 1);
        assert_eq!(memory.reads.get(), view.frames[0].slots.len());

        memory.reads.set(0);
        let fallback = build_stack_view(
            None,
            &memory,
            pc,
            Address::ZERO,
            StackViewOptions {
                raw_stack_words: usize::MAX,
                ..StackViewOptions::default()
            },
        )
        .unwrap();
        assert_eq!(fallback.raw_stack.len(), MAX_RAW_STACK_WORDS);
        assert_eq!(memory.reads.get(), MAX_RAW_STACK_WORDS);
        assert_eq!(
            add_signed(Address::ZERO, -1).unwrap(),
            Address::new(0xffff).unwrap()
        );
    }
}

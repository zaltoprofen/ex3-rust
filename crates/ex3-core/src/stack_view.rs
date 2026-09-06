//! Semantic reconstruction of EX3 C stack frames from sidecar debug metadata.

use crate::{
    cc::{DynamicStackSlotKind, FunctionDebugId, ScalarType, TemporaryRole},
    debug_info::{InstructionDebugInfo, LinkedFunctionDebugInfo, ProgramDebugInfo},
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
        return_target: Some(symbolicate(program, return_address)),
    });
    for local in &function.locals {
        slots.push(read_slot(
            memory,
            frame_sp,
            local.frame_offset,
            StackSlotKind::Local { slot: local.slot },
            local.name.clone(),
            Some(local.ty),
            StackValueStatus::CurrentStorage,
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
            if active.is_some() {
                StackValueStatus::Value
            } else {
                StackValueStatus::StaleScratch
            },
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
            program_counter: symbolicate(program, pc),
            program_counter_kind: pc_kind,
            slots,
        },
        return_address,
    ))
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
    let typed_value = if value_status == StackValueStatus::StaleScratch {
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

fn symbolicate(program: &ProgramDebugInfo, address: Address) -> SymbolicAddress {
    if let Some(instruction) = program.instruction(address) {
        if let Some(function) = program.function(instruction.function_id) {
            return SymbolicAddress {
                address,
                symbol: Some(function.name.clone()),
                offset: Some(address.get().wrapping_sub(function.address_start.get())),
            };
        }
    }
    let symbol = program
        .symbols
        .iter()
        .filter(|(_, symbol_address)| **symbol_address <= address)
        .max_by_key(|(_, symbol_address)| **symbol_address)
        .map(|(name, symbol_address)| {
            (
                name.clone(),
                address.get().wrapping_sub(symbol_address.get()),
            )
        });
    SymbolicAddress {
        address,
        symbol: symbol.as_ref().map(|(name, _)| name.clone()),
        offset: symbol.map(|(_, offset)| offset),
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
    let symbol = symbolicate(program, pc).symbol;
    if let Some(symbol) = symbol
        .as_ref()
        .filter(|symbol| symbol.starts_with("__ex3_"))
    {
        return StackViewContext::Runtime {
            symbol: symbol.clone(),
        };
    }
    let first_function = program
        .functions
        .iter()
        .map(|function| function.address_start)
        .min();
    if pc >= Address::RESET && first_function.is_some_and(|start| pc < start) {
        return StackViewContext::Startup;
    }
    if hint == Some(NonCContextHint::Assembly) || symbol.is_some() {
        StackViewContext::Assembly { symbol }
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
        let runtime_pc = program.symbols["__ex3_mul_i32"];
        let runtime = build_stack_view(
            Some(&program),
            &memory,
            runtime_pc,
            Address::ZERO,
            StackViewOptions::default(),
        )
        .unwrap();
        assert!(matches!(runtime.context, StackViewContext::Runtime { .. }));

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

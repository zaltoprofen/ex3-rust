use ex3_core::{
    cc::ScalarType,
    stack_view::{
        FrameProgramCounterKind, StackSlot, StackSlotKind, StackValueStatus, StackView,
        StackViewContext, TypedStackValue,
    },
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    pub assembly: String,
    pub symbols: Vec<SymbolEntry>,
    pub source_map: Vec<AssemblySourceMapRow>,
    pub loaded_words: u32,
    pub snapshot: CpuSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssemblySourceMapRow {
    pub address: u16,
    pub line: usize,
    pub executable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolEntry {
    pub name: String,
    pub address: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuSnapshot {
    pub pc: u16,
    pub sp: u16,
    pub ac: u32,
    pub ir: u32,
    pub psr: u32,
    pub ien: bool,
    pub negative: bool,
    pub zero: bool,
    pub carry: bool,
    pub overflow: bool,
    pub halted: bool,
    pub interrupt_pending: bool,
    pub executed_instructions: u64,
    pub serial_selected: bool,
    pub interrupt_mask: u8,
    pub input_register: u8,
    pub assembly_line: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StepOutcomeDto {
    Executed,
    Interrupted,
    Halted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub outcome: StepOutcomeDto,
    pub pc_before: Option<u16>,
    pub instruction: Option<String>,
    pub snapshot: CpuSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Halted,
    Breakpoint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunChunkResult {
    pub status: RunStatus,
    pub executed: u32,
    pub breakpoint_address: Option<u16>,
    pub snapshot: CpuSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRow {
    pub address: u16,
    pub word: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisassemblyRow {
    pub address: u16,
    pub word: u32,
    pub instruction: String,
    pub valid: bool,
    pub source_line: Option<usize>,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackViewSnapshot {
    pub available: bool,
    pub context: StackViewContextDto,
    pub context_symbol: Option<String>,
    pub pc: u16,
    pub sp: u16,
    pub frames: Vec<StackFrameDto>,
    pub raw_stack: Vec<MemoryRow>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StackViewContextDto {
    CFunction,
    Runtime,
    Startup,
    Assembly,
    Interrupt,
    Unmapped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFrameDto {
    pub function_name: String,
    pub current: bool,
    pub pc: Option<u16>,
    pub current_sp: u16,
    pub frame_sp: u16,
    pub return_address: Option<u16>,
    pub return_symbol: Option<String>,
    pub slots: Vec<StackSlotDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackSlotDto {
    pub address: u16,
    pub frame_offset: i32,
    pub kind: StackSlotKindDto,
    pub name: String,
    pub type_name: Option<&'static str>,
    pub raw_value: u32,
    pub signed_value: Option<i32>,
    pub unsigned_value: Option<u32>,
    pub active: Option<bool>,
    pub state: StackSlotStateDto,
    pub description: Option<String>,
    pub argument_index: Option<u16>,
    pub call_target: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StackSlotKindDto {
    Parameter,
    ReturnAddress,
    Local,
    Temporary,
    OutgoingArgument,
    RuntimeArgument,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StackSlotStateDto {
    Value,
    CurrentStorage,
    InactiveScratch,
    NotAllocated,
    Released,
    Control,
    Unknown,
}

impl From<StackView> for StackViewSnapshot {
    fn from(view: StackView) -> Self {
        let (context, context_symbol) = match view.context {
            StackViewContext::C { function, .. } => {
                (StackViewContextDto::CFunction, Some(function))
            }
            StackViewContext::Runtime { symbol } => (StackViewContextDto::Runtime, Some(symbol)),
            StackViewContext::Startup => (StackViewContextDto::Startup, None),
            StackViewContext::Assembly { symbol } => (StackViewContextDto::Assembly, symbol),
            StackViewContext::Interrupt => (StackViewContextDto::Interrupt, None),
            StackViewContext::Unmapped => (StackViewContextDto::Unmapped, None),
        };
        Self {
            available: !view.frames.is_empty(),
            context,
            context_symbol,
            pc: view.pc.get(),
            sp: view.sp.get(),
            frames: view.frames.into_iter().map(StackFrameDto::from).collect(),
            raw_stack: view
                .raw_stack
                .into_iter()
                .map(|word| MemoryRow {
                    address: word.address.get(),
                    word: word.raw,
                })
                .collect(),
            warnings: view.warnings,
        }
    }
}

impl From<ex3_core::stack_view::StackFrame> for StackFrameDto {
    fn from(frame: ex3_core::stack_view::StackFrame) -> Self {
        let return_slot = frame
            .slots
            .iter()
            .find(|slot| matches!(slot.kind, StackSlotKind::ReturnAddress));
        let return_address = return_slot.map(|slot| slot.raw as u16);
        let return_symbol = return_slot
            .and_then(|slot| slot.return_target.as_ref())
            .and_then(|target| {
                target.symbol.as_ref().map(|symbol| match target.offset {
                    Some(0) | None => symbol.clone(),
                    Some(offset) => format!("{symbol}+0x{offset:x}"),
                })
            });
        Self {
            function_name: frame.function,
            current: frame.program_counter_kind == FrameProgramCounterKind::Current,
            pc: Some(frame.program_counter.address.get()),
            current_sp: frame.current_sp.get(),
            frame_sp: frame.frame_sp.get(),
            return_address,
            return_symbol,
            slots: frame.slots.into_iter().map(StackSlotDto::from).collect(),
        }
    }
}

impl From<StackSlot> for StackSlotDto {
    fn from(slot: StackSlot) -> Self {
        let (kind, active, description, argument_index, call_target) = match &slot.kind {
            StackSlotKind::Parameter { .. } => {
                (StackSlotKindDto::Parameter, None, None, None, None)
            }
            StackSlotKind::ReturnAddress => {
                (StackSlotKindDto::ReturnAddress, None, None, None, None)
            }
            StackSlotKind::Local { .. } => (StackSlotKindDto::Local, None, None, None, None),
            StackSlotKind::Temporary { active, .. } => (
                StackSlotKindDto::Temporary,
                Some(*active),
                Some(slot.display_name.clone()),
                None,
                None,
            ),
            StackSlotKind::OutgoingArgument {
                callee,
                argument_index,
                ..
            } => (
                StackSlotKindDto::OutgoingArgument,
                None,
                Some(slot.display_name.clone()),
                Some(*argument_index),
                Some(callee.clone()),
            ),
            StackSlotKind::RuntimeArgument {
                helper,
                argument_index,
            } => (
                StackSlotKindDto::RuntimeArgument,
                None,
                Some(slot.display_name.clone()),
                Some(*argument_index),
                Some(helper.clone()),
            ),
            StackSlotKind::Unknown => (StackSlotKindDto::Unknown, None, None, None, None),
        };
        let (signed_value, unsigned_value) = match slot.typed_value {
            Some(TypedStackValue::Signed(value)) => (Some(value), None),
            Some(TypedStackValue::Unsigned(value)) => (None, Some(value)),
            None => (None, None),
        };
        let state = match slot.value_status {
            StackValueStatus::Value => StackSlotStateDto::Value,
            StackValueStatus::CurrentStorage => StackSlotStateDto::CurrentStorage,
            StackValueStatus::StaleScratch => StackSlotStateDto::InactiveScratch,
            StackValueStatus::NotAllocated => StackSlotStateDto::NotAllocated,
            StackValueStatus::Released => StackSlotStateDto::Released,
            StackValueStatus::Control => StackSlotStateDto::Control,
            StackValueStatus::Unknown => StackSlotStateDto::Unknown,
        };
        let typed_value_is_valid = matches!(
            slot.value_status,
            StackValueStatus::Value | StackValueStatus::CurrentStorage
        );
        Self {
            address: slot.address.get(),
            frame_offset: slot.frame_offset,
            kind,
            name: slot.display_name,
            type_name: slot.ty.map(type_name),
            raw_value: slot.raw,
            signed_value: typed_value_is_valid.then_some(signed_value).flatten(),
            unsigned_value: typed_value_is_valid.then_some(unsigned_value).flatten(),
            active,
            state,
            description,
            argument_index,
            call_target,
        }
    }
}

const fn type_name(ty: ScalarType) -> &'static str {
    match ty {
        ScalarType::Int32 => "int32_t",
        ScalarType::UInt32 => "uint32_t",
    }
}

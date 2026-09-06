use super::ScalarType;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionDebugId(usize);

impl FunctionDebugId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compilation {
    pub assembly: String,
    pub debug_info: CompilerDebugInfo,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompilerDebugInfo {
    pub functions: Vec<FunctionDebugSymbols>,
    pub frames: Vec<FunctionFrameDebugInfo>,
    pub assembly_lines: Vec<AssemblyLineDebugInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedAssembly {
    pub text: String,
    pub debug_lines: Vec<AssemblyLineDebugInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyLineDebugInfo {
    pub assembly_line: u32,
    pub function_id: FunctionDebugId,
    /// Signed word delta satisfying
    /// `canonical_frame_sp = current_sp + frame_base_delta` immediately before
    /// this instruction executes (with EX3's 16-bit address wrapping).
    pub frame_base_delta: i32,
    pub fixed_frame_state: FixedFrameState,
    pub active_temporaries: Vec<ActiveTemporaryDebugInfo>,
    pub dynamic_stack_slots: Vec<DynamicStackSlotDebugInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitDebugContext {
    pub function_id: FunctionDebugId,
    /// See [`AssemblyLineDebugInfo::frame_base_delta`].
    pub frame_base_delta: i32,
    pub fixed_frame_state: FixedFrameState,
    pub active_temporaries: Vec<ActiveTemporaryDebugInfo>,
    pub dynamic_stack_slots: Vec<DynamicStackSlotDebugInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedFrameState {
    NotAllocated,
    Allocated,
    Released,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveTemporaryDebugInfo {
    pub slot: u16,
    pub role: TemporaryRole,
    pub display_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporaryRole {
    UnaryOperand,
    BinaryLeft,
    BinaryRight,
    ComparisonLeft,
    ComparisonRight,
    SwitchValue,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicStackSlotDebugInfo {
    /// Signed word offset from the canonical frame SP. Dynamically pushed
    /// arguments occupy negative offsets.
    pub frame_offset: i32,
    pub kind: DynamicStackSlotKind,
    pub display_name: String,
    pub ty: Option<ScalarType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynamicStackSlotKind {
    OutgoingArgument {
        callee: String,
        argument_index: u16,
        parameter_name: Option<String>,
    },
    RuntimeArgument {
        helper: String,
        argument_index: u16,
    },
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDebugSymbols {
    pub id: FunctionDebugId,
    pub name: String,
    pub parameters: Vec<ParameterDebugSymbol>,
    pub locals: Vec<LocalDebugSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterDebugSymbol {
    pub index: usize,
    pub name: String,
    pub ty: ScalarType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDebugSymbol {
    pub slot: usize,
    pub name: String,
    pub ty: ScalarType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionFrameDebugInfo {
    pub function_id: FunctionDebugId,
    pub name: String,
    pub frame_size: u16,
    pub parameters: Vec<ParameterSlotDebugInfo>,
    pub return_address: ReturnAddressSlotDebugInfo,
    pub locals: Vec<LocalSlotDebugInfo>,
    pub temporary_count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterSlotDebugInfo {
    pub index: u16,
    pub name: String,
    pub ty: ScalarType,
    pub frame_offset: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReturnAddressSlotDebugInfo {
    pub frame_offset: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSlotDebugInfo {
    pub slot: u16,
    pub name: String,
    pub ty: ScalarType,
    pub frame_offset: i32,
}

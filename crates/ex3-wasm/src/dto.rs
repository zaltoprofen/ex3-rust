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

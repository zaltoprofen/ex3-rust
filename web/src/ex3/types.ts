export type MachinePhase =
  | "empty"
  | "ready"
  | "running"
  | "paused"
  | "halted"
  | "error";

export interface SymbolEntry {
  name: string;
  address: number;
}

export interface AssemblySourceMapRow {
  address: number;
  line: number;
  executable: boolean;
}

export interface CpuSnapshot {
  pc: number;
  sp: number;
  ac: number;
  ir: number;
  psr: number;
  ien: boolean;
  negative: boolean;
  zero: boolean;
  carry: boolean;
  overflow: boolean;
  halted: boolean;
  interruptPending: boolean;
  executedInstructions: number;
  serialSelected: boolean;
  interruptMask: number;
  inputRegister: number;
  assemblyLine: number | null;
}

export interface CompileResult {
  assembly: string;
  symbols: SymbolEntry[];
  sourceMap: AssemblySourceMapRow[];
  loadedWords: number;
  snapshot: CpuSnapshot;
}

export interface StepResult {
  outcome: "executed" | "interrupted" | "halted";
  pcBefore: number | null;
  instruction: string | null;
  snapshot: CpuSnapshot;
}

export interface RunChunkResult {
  status: "running" | "halted" | "breakpoint";
  executed: number;
  breakpointAddress: number | null;
  snapshot: CpuSnapshot;
}

export interface MemoryRow {
  address: number;
  word: number;
}

export interface DisassemblyRow {
  address: number;
  word: number;
  instruction: string;
  valid: boolean;
  sourceLine: number | null;
  labels: string[];
}

export interface Diagnostic {
  line: number | null;
  column: number | null;
  message: string;
}

export interface Ex3Error {
  stage: "compiler" | "assembler" | "emulator" | "session";
  message: string;
  diagnostics: Diagnostic[];
}

export interface Ex3SessionApi {
  compile_and_load(source: string): CompileResult;
  reset(): CpuSnapshot;
  step(): StepResult;
  run_chunk(maxInstructions: number): RunChunkResult;
  memory_range(start: number, count: number): MemoryRow[];
  disassembly_range(start: number, count: number): DisassemblyRow[];
  toggle_breakpoint(address: number): boolean;
  clear_breakpoints(): void;
  breakpoints(): number[];
  serial_output(): string;
}

export interface MachineUiState {
  source: string;
  assembly: string;
  sourceMap: AssemblySourceMapRow[];
  phase: MachinePhase;
  busy: boolean;
  snapshot: CpuSnapshot | null;
  disassembly: DisassemblyRow[];
  stackMemory: MemoryRow[];
  selectedMemory: MemoryRow[];
  selectedMemoryAddress: number;
  serialOutput: string;
  breakpoints: number[];
  diagnostics: Diagnostic[];
  errorMessage: string | null;
  stopMessage: string | null;
  runInstructionCount: number;
}

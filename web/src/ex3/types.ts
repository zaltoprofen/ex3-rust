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
  loadedWords: number;
  snapshot: CpuSnapshot;
}

export interface StepResult {
  outcome: "executed" | "interrupted" | "halted";
  pcBefore: number | null;
  instruction: string | null;
  snapshot: CpuSnapshot;
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
  step(): StepResult;
}

export interface MachineUiState {
  source: string;
  assembly: string;
  phase: MachinePhase;
  busy: boolean;
  snapshot: CpuSnapshot | null;
  diagnostics: Diagnostic[];
  errorMessage: string | null;
}

import type {
  CompileResult,
  CpuSnapshot,
  Ex3Error,
  Ex3SessionApi,
  MachinePhase,
  MachineUiState,
  RunChunkResult,
  StepResult,
} from "./types";

const DISASSEMBLY_WORDS = 16;
const STACK_WORDS = 24;
const SELECTED_MEMORY_WORDS = 32;

export function compileMachine(
  session: Ex3SessionApi,
  state: MachineUiState,
): MachineUiState {
  const result: CompileResult = session.compile_and_load(state.source);
  return refreshMachine(session, {
    ...state,
    assembly: result.assembly,
    sourceMap: result.sourceMap,
    phase: result.snapshot.halted ? "halted" : "ready",
    busy: false,
    snapshot: result.snapshot,
    diagnostics: [],
    errorStage: null,
    errorMessage: null,
    stopMessage: null,
    runInstructionCount: 0,
  });
}

export function resetMachine(
  session: Ex3SessionApi,
  state: MachineUiState,
): MachineUiState {
  const snapshot = session.reset();
  return refreshMachine(session, {
    ...state,
    phase: "ready",
    busy: false,
    snapshot,
    diagnostics: [],
    errorStage: null,
    errorMessage: null,
    stopMessage: null,
    runInstructionCount: 0,
  });
}

export function stepMachine(
  session: Ex3SessionApi,
  state: MachineUiState,
): MachineUiState {
  const result: StepResult = session.step();
  return refreshMachine(session, {
    ...state,
    phase: result.snapshot.halted ? "halted" : "ready",
    busy: false,
    snapshot: result.snapshot,
    diagnostics: [],
    errorStage: null,
    errorMessage: null,
    stopMessage: result.snapshot.halted ? "Program halted" : null,
  });
}

export interface RunChunkUpdate {
  state: MachineUiState;
  result: RunChunkResult;
}

export function runMachineChunk(
  session: Ex3SessionApi,
  state: MachineUiState,
  maxInstructions: number,
): RunChunkUpdate {
  const result = session.run_chunk(maxInstructions);
  const phase: MachinePhase =
    result.status === "running"
      ? "running"
      : result.status === "halted"
        ? "halted"
        : "paused";
  const stopMessage =
    result.status === "breakpoint"
      ? `Breakpoint reached at 0x${hex16(result.breakpointAddress ?? result.snapshot.pc)}`
      : result.status === "halted"
        ? "Program halted"
        : null;
  const next = refreshMachine(session, {
    ...state,
    phase,
    busy: false,
    snapshot: result.snapshot,
    diagnostics: [],
    errorStage: null,
    errorMessage: null,
    stopMessage,
    runInstructionCount: state.runInstructionCount + result.executed,
  });
  return { state: next, result };
}

export function selectMemory(
  session: Ex3SessionApi,
  state: MachineUiState,
  address: number,
): MachineUiState {
  const selectedMemoryAddress = address & 0xffff;
  return {
    ...state,
    selectedMemoryAddress,
    selectedMemory: session.memory_range(selectedMemoryAddress, SELECTED_MEMORY_WORDS),
    errorMessage: null,
  };
}

export function toggleBreakpoint(
  session: Ex3SessionApi,
  state: MachineUiState,
  address: number,
): MachineUiState {
  session.toggle_breakpoint(address);
  return {
    ...state,
    breakpoints: session.breakpoints(),
    stopMessage: null,
  };
}

export function errorState(state: MachineUiState, thrown: unknown): MachineUiState {
  const error = normalizeError(thrown);
  return {
    ...state,
    phase: "error",
    busy: false,
    diagnostics: error.diagnostics,
    errorStage: error.stage,
    errorMessage: error.message,
    stopMessage: null,
  };
}

export function refreshMachine(
  session: Ex3SessionApi,
  state: MachineUiState & { snapshot: CpuSnapshot },
): MachineUiState {
  const { snapshot } = state;
  return {
    ...state,
    disassembly: session.disassembly_range((snapshot.pc - 4) & 0xffff, DISASSEMBLY_WORDS),
    stackMemory: session.memory_range((snapshot.sp - 8) & 0xffff, STACK_WORDS),
    selectedMemory: session.memory_range(
      state.selectedMemoryAddress,
      SELECTED_MEMORY_WORDS,
    ),
    serialOutput: session.serial_output(),
    breakpoints: session.breakpoints(),
  };
}

function normalizeError(thrown: unknown): Ex3Error {
  if (isEx3Error(thrown)) {
    return thrown;
  }
  return {
    stage: "session",
    message: thrown instanceof Error ? thrown.message : String(thrown),
    diagnostics: [],
  };
}

function isEx3Error(value: unknown): value is Ex3Error {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<Ex3Error>;
  return (
    typeof candidate.stage === "string" &&
    typeof candidate.message === "string" &&
    Array.isArray(candidate.diagnostics)
  );
}

function hex16(value: number): string {
  return (value & 0xffff).toString(16).padStart(4, "0");
}

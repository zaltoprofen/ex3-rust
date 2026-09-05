import type {
  CompileResult,
  Ex3Error,
  Ex3SessionApi,
  MachineUiState,
  StepResult,
} from "./types";

export function compileMachine(
  session: Ex3SessionApi,
  state: MachineUiState,
): MachineUiState {
  const result: CompileResult = session.compile_and_load(state.source);
  return {
    ...state,
    assembly: result.assembly,
    phase: result.snapshot.halted ? "halted" : "ready",
    busy: false,
    snapshot: result.snapshot,
    diagnostics: [],
    errorMessage: null,
  };
}

export function stepMachine(
  session: Ex3SessionApi,
  state: MachineUiState,
): MachineUiState {
  const result: StepResult = session.step();
  return {
    ...state,
    phase: result.snapshot.halted ? "halted" : "ready",
    busy: false,
    snapshot: result.snapshot,
    diagnostics: [],
    errorMessage: null,
  };
}

export function errorState(state: MachineUiState, thrown: unknown): MachineUiState {
  const error = normalizeError(thrown);
  return {
    ...state,
    phase: "error",
    busy: false,
    diagnostics: error.diagnostics,
    errorMessage: error.message,
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

import type { MachineUiState } from "../ex3/types";
import { SAMPLE_PROGRAMS } from "../samples";

export const INITIAL_SOURCE = SAMPLE_PROGRAMS[0].source;

export function createInitialMachineState(): MachineUiState {
  return {
    source: INITIAL_SOURCE,
    assembly: "",
    sourceMap: [],
    phase: "empty",
    busy: false,
    snapshot: null,
    disassembly: [],
    stackMemory: [],
    selectedMemory: [],
    selectedMemoryAddress: 0,
    serialOutput: "",
    breakpoints: [],
    diagnostics: [],
    errorStage: null,
    errorMessage: null,
    stopMessage: null,
    runInstructionCount: 0,
  };
}

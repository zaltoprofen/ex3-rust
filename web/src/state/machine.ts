import type { MachineUiState } from "../ex3/types";

export const INITIAL_SOURCE = `int fact(int n) {
    if (n <= 1) return 1;
    return n * fact(n - 1);
}

int main(void) {
    return fact(5);
}
`;

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
    errorMessage: null,
    stopMessage: null,
    runInstructionCount: 0,
  };
}

import { useEffect, useRef, useState } from "react";
import { AssemblyView } from "./components/AssemblyView";
import { Diagnostics } from "./components/Diagnostics";
import { DisassemblyView } from "./components/DisassemblyView";
import { ExecutionControls } from "./components/ExecutionControls";
import { FlagsView } from "./components/FlagsView";
import { MemoryView } from "./components/MemoryView";
import { RegisterView } from "./components/RegisterView";
import { SerialConsole } from "./components/SerialConsole";
import { SourceEditor } from "./components/SourceEditor";
import {
  compileMachine,
  errorState,
  resetMachine,
  runMachineChunk,
  selectMemory,
  stepMachine,
  toggleBreakpoint,
} from "./ex3/runner";
import type { Ex3SessionApi, MachineUiState } from "./ex3/types";
import { getEx3Session } from "./ex3/wasm";
import { createInitialMachineState } from "./state/machine";

const RUN_CHUNK_SIZE = 2_000;
const RUN_INSTRUCTION_LIMIT = 1_000_000;

export default function App() {
  const [session, setSession] = useState<Ex3SessionApi | null>(null);
  const [machine, setMachine] = useState(createInitialMachineState);
  const machineRef = useRef(machine);
  const operationPending = useRef(false);
  const runGeneration = useRef(0);

  const commit = (next: MachineUiState) => {
    machineRef.current = next;
    setMachine(next);
  };

  useEffect(() => {
    let active = true;
    getEx3Session()
      .then((nextSession) => {
        if (active) setSession(nextSession);
      })
      .catch((error: unknown) => {
        if (active) commit(errorState(machineRef.current, error));
      });
    return () => {
      active = false;
      runGeneration.current += 1;
    };
  }, []);

  const compile = () =>
    executeSingleOperation(session, operationPending, machineRef.current, commit, (state) => {
      runGeneration.current += 1;
      return compileMachine(session!, state);
    });

  const reset = () =>
    executeSingleOperation(session, operationPending, machineRef.current, commit, (state) => {
      runGeneration.current += 1;
      return resetMachine(session!, state);
    });

  const step = () =>
    executeSingleOperation(session, operationPending, machineRef.current, commit, (state) =>
      stepMachine(session!, state),
    );

  const run = async () => {
    if (
      !session ||
      operationPending.current ||
      !["ready", "paused"].includes(machineRef.current.phase)
    ) {
      return;
    }
    const generation = runGeneration.current + 1;
    runGeneration.current = generation;
    let current: MachineUiState = {
      ...machineRef.current,
      phase: "running",
      runInstructionCount: 0,
      stopMessage: null,
      errorMessage: null,
    };
    commit(current);
    await nextFrame();

    try {
      if (
        current.snapshot &&
        current.breakpoints.includes(current.snapshot.pc) &&
        runGeneration.current === generation
      ) {
        const before = current.snapshot.executedInstructions;
        current = stepMachine(session, current);
        current = {
          ...current,
          phase: current.phase === "halted" ? "halted" : "running",
          runInstructionCount:
            current.snapshot!.executedInstructions - before,
        };
        commit(current);
        if (current.phase === "halted") return;
        await nextFrame();
      }

      while (runGeneration.current === generation) {
        const remaining = RUN_INSTRUCTION_LIMIT - current.runInstructionCount;
        if (remaining <= 0) {
          commit(executionLimitState(current));
          return;
        }
        const update = runMachineChunk(
          session,
          current,
          Math.min(RUN_CHUNK_SIZE, remaining),
        );
        current = update.state;
        commit(current);
        if (update.result.status !== "running") return;
        if (current.runInstructionCount >= RUN_INSTRUCTION_LIMIT) {
          commit(executionLimitState(current));
          return;
        }
        await nextFrame();
      }
    } catch (error) {
      if (runGeneration.current === generation) commit(errorState(current, error));
    }
  };

  const pause = () => {
    if (machineRef.current.phase !== "running") return;
    runGeneration.current += 1;
    commit({ ...machineRef.current, phase: "paused", stopMessage: "Paused" });
  };

  const setBreakpoint = (address: number) => {
    if (!session || machineRef.current.phase === "running") return;
    try {
      commit(toggleBreakpoint(session, machineRef.current, address));
    } catch (error) {
      commit(errorState(machineRef.current, error));
    }
  };

  const setMemoryAddress = (address: number) => {
    if (!session || machineRef.current.snapshot === null) return;
    try {
      commit(selectMemory(session, machineRef.current, address));
    } catch (error) {
      commit(errorState(machineRef.current, error));
    }
  };

  const controlsDisabled = machine.busy || machine.phase === "running";

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <p className="eyebrow">EX3 v3.0</p>
          <h1>Playground</h1>
        </div>
        <p className="status-copy">{session ? "WASM ready" : "Initializing WASM…"}</p>
      </header>

      <div className="editor-grid">
        <SourceEditor
          value={machine.source}
          disabled={controlsDisabled}
          onChange={(source) => commit({ ...machineRef.current, source })}
        />
        <AssemblyView
          assembly={machine.assembly}
          sourceMap={machine.sourceMap}
          currentLine={machine.snapshot?.assemblyLine ?? null}
          breakpoints={machine.breakpoints}
          disabled={controlsDisabled}
          onToggleBreakpoint={setBreakpoint}
        />
      </div>

      <ExecutionControls
        phase={machine.phase}
        busy={machine.busy}
        initialized={session !== null}
        loaded={machine.snapshot !== null}
        runInstructionCount={machine.runInstructionCount}
        onCompile={() => void compile()}
        onReset={() => void reset()}
        onStep={() => void step()}
        onRun={() => void run()}
        onPause={pause}
      />
      {machine.stopMessage && <p className="stop-message">{machine.stopMessage}</p>}
      <Diagnostics diagnostics={machine.diagnostics} message={machine.errorMessage} />

      <div className="machine-grid">
        <RegisterView snapshot={machine.snapshot} />
        <FlagsView snapshot={machine.snapshot} />
      </div>

      <div className="debug-grid">
        <DisassemblyView
          rows={machine.disassembly}
          pc={machine.snapshot?.pc ?? null}
          breakpoints={machine.breakpoints}
        />
        <MemoryView
          stackRows={machine.stackMemory}
          selectedRows={machine.selectedMemory}
          sp={machine.snapshot?.sp ?? null}
          selectedAddress={machine.selectedMemoryAddress}
          disabled={controlsDisabled || machine.snapshot === null}
          onSelectAddress={setMemoryAddress}
        />
      </div>

      <SerialConsole output={machine.serialOutput} />
    </main>
  );
}

async function executeSingleOperation(
  session: Ex3SessionApi | null,
  operationPending: { current: boolean },
  current: MachineUiState,
  commit: (state: MachineUiState) => void,
  operation: (state: MachineUiState) => MachineUiState,
): Promise<void> {
  if (!session || operationPending.current) return;
  operationPending.current = true;
  const pending = {
    ...current,
    busy: true,
    diagnostics: [],
    errorMessage: null,
    stopMessage: null,
  };
  commit(pending);
  await nextFrame();
  try {
    commit(operation(pending));
  } catch (error) {
    commit(errorState(pending, error));
  } finally {
    operationPending.current = false;
  }
}

function executionLimitState(state: MachineUiState): MachineUiState {
  return {
    ...state,
    phase: "paused",
    stopMessage: "Execution limit reached (1,000,000 instructions)",
  };
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

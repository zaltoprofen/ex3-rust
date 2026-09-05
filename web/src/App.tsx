import { useEffect, useRef, useState } from "react";
import { AssemblyView } from "./components/AssemblyView";
import { Diagnostics } from "./components/Diagnostics";
import { ExecutionControls } from "./components/ExecutionControls";
import { FlagsView } from "./components/FlagsView";
import { RegisterView } from "./components/RegisterView";
import { SourceEditor } from "./components/SourceEditor";
import { compileMachine, errorState, stepMachine } from "./ex3/runner";
import type { Ex3SessionApi } from "./ex3/types";
import { getEx3Session } from "./ex3/wasm";
import { createInitialMachineState } from "./state/machine";

export default function App() {
  const [session, setSession] = useState<Ex3SessionApi | null>(null);
  const [machine, setMachine] = useState(createInitialMachineState);
  const operationPending = useRef(false);

  useEffect(() => {
    let active = true;
    getEx3Session()
      .then((nextSession) => {
        if (active) setSession(nextSession);
      })
      .catch((error: unknown) => {
        if (active) setMachine((state) => errorState(state, error));
      });
    return () => {
      active = false;
    };
  }, []);

  const compile = async () => {
    if (!session || operationPending.current) return;
    operationPending.current = true;
    const pending = { ...machine, busy: true, diagnostics: [], errorMessage: null };
    setMachine(pending);
    await nextFrame();
    try {
      setMachine(compileMachine(session, pending));
    } catch (error) {
      setMachine(errorState(pending, error));
    } finally {
      operationPending.current = false;
    }
  };

  const step = async () => {
    if (!session || operationPending.current) return;
    operationPending.current = true;
    const pending = { ...machine, busy: true, diagnostics: [], errorMessage: null };
    setMachine(pending);
    await nextFrame();
    try {
      setMachine(stepMachine(session, pending));
    } catch (error) {
      setMachine(errorState(pending, error));
    } finally {
      operationPending.current = false;
    }
  };

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
          disabled={machine.busy}
          onChange={(source) => setMachine((state) => ({ ...state, source }))}
        />
        <AssemblyView assembly={machine.assembly} />
      </div>

      <div className="machine-grid">
        <RegisterView snapshot={machine.snapshot} />
        <FlagsView snapshot={machine.snapshot} />
      </div>

      <ExecutionControls
        phase={machine.phase}
        busy={machine.busy}
        initialized={session !== null}
        onCompile={compile}
        onStep={step}
      />
      <Diagnostics diagnostics={machine.diagnostics} message={machine.errorMessage} />
    </main>
  );
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

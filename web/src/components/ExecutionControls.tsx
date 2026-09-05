import type { MachinePhase } from "../ex3/types";

interface ExecutionControlsProps {
  phase: MachinePhase;
  busy: boolean;
  initialized: boolean;
  loaded: boolean;
  runInstructionCount: number;
  onCompile(): void;
  onReset(): void;
  onStep(): void;
  onRun(): void;
  onPause(): void;
}

export function ExecutionControls({
  phase,
  busy,
  initialized,
  loaded,
  runInstructionCount,
  onCompile,
  onReset,
  onStep,
  onRun,
  onPause,
}: ExecutionControlsProps) {
  const canOperate = initialized && !busy && phase !== "running";
  const canExecute = canOperate && (phase === "ready" || phase === "paused");
  return (
    <section className="controls" aria-label="Execution controls">
      <button type="button" onClick={onCompile} disabled={!initialized || busy || phase === "running"}>
        {busy ? "Working…" : "Compile"}
      </button>
      <button type="button" onClick={onReset} disabled={!canOperate || !loaded}>
        Reset
      </button>
      <button type="button" onClick={onStep} disabled={!canExecute}>
        Step
      </button>
      <button type="button" onClick={onRun} disabled={!canExecute}>
        Run
      </button>
      <button type="button" onClick={onPause} disabled={phase !== "running"}>
        Pause
      </button>
      <span className="run-count">Run: {runInstructionCount.toLocaleString()}</span>
      <span className={`phase phase-${phase}`}>{phase}</span>
    </section>
  );
}

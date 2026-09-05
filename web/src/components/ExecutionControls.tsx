import type { MachinePhase } from "../ex3/types";

interface ExecutionControlsProps {
  phase: MachinePhase;
  busy: boolean;
  initialized: boolean;
  onCompile(): void;
  onStep(): void;
}

export function ExecutionControls({
  phase,
  busy,
  initialized,
  onCompile,
  onStep,
}: ExecutionControlsProps) {
  const canStep = initialized && !busy && (phase === "ready" || phase === "paused");
  return (
    <section className="controls" aria-label="Execution controls">
      <button type="button" onClick={onCompile} disabled={!initialized || busy}>
        {busy ? "Working…" : "Compile"}
      </button>
      <button type="button" onClick={onStep} disabled={!canStep}>
        Step
      </button>
      <span className={`phase phase-${phase}`}>{phase}</span>
    </section>
  );
}

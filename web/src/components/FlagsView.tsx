import type { CpuSnapshot } from "../ex3/types";

interface FlagsViewProps {
  snapshot: CpuSnapshot | null;
}

export function FlagsView({ snapshot }: FlagsViewProps) {
  const flags = snapshot
    ? [
        ["IEN", snapshot.ien],
        ["N", snapshot.negative],
        ["Z", snapshot.zero],
        ["C", snapshot.carry],
        ["V", snapshot.overflow],
      ] as const
    : [];

  return (
    <section className="panel machine-panel">
      <h2>Flags</h2>
      {snapshot ? (
        <div className="flags">
          {flags.map(([name, enabled]) => (
            <div className={enabled ? "flag flag-on" : "flag"} key={name}>
              <span>{name}</span>
              <strong>{enabled ? "1" : "0"}</strong>
            </div>
          ))}
        </div>
      ) : (
        <p className="muted">No CPU state loaded.</p>
      )}
    </section>
  );
}

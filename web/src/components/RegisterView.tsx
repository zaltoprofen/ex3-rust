import type { CpuSnapshot } from "../ex3/types";

interface RegisterViewProps {
  snapshot: CpuSnapshot | null;
}

const hex = (value: number, width: number) =>
  `0x${(value >>> 0).toString(16).padStart(width, "0")}`;

export function RegisterView({ snapshot }: RegisterViewProps) {
  const rows = snapshot
    ? [
        ["PC", hex(snapshot.pc, 4)],
        ["SP", hex(snapshot.sp, 4)],
        ["AC", hex(snapshot.ac, 8)],
        ["IR", hex(snapshot.ir, 8)],
        ["PSR", hex(snapshot.psr, 8)],
        ["COUNT", snapshot.executedInstructions.toLocaleString()],
      ]
    : [];

  return (
    <section className="panel machine-panel">
      <h2>Registers</h2>
      {snapshot ? (
        <dl className="register-grid">
          {rows.map(([name, value]) => (
            <div key={name}>
              <dt>{name}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="muted">Compile a program to inspect the CPU.</p>
      )}
    </section>
  );
}

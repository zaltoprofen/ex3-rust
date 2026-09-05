import type { DisassemblyRow } from "../ex3/types";

interface DisassemblyViewProps {
  rows: DisassemblyRow[];
  pc: number | null;
  breakpoints: number[];
}

export function DisassemblyView({ rows, pc, breakpoints }: DisassemblyViewProps) {
  const breakpointSet = new Set(breakpoints);
  return (
    <section className="panel data-panel">
      <h2>Disassembly</h2>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              <th aria-label="markers" />
              <th>Address</th>
              <th>Word</th>
              <th>Instruction</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr className={row.address === pc ? "active-row" : undefined} key={row.address}>
                <td className="markers">
                  {row.address === pc ? "▶" : ""}
                  {breakpointSet.has(row.address) ? "●" : ""}
                </td>
                <td>{hex(row.address, 4)}</td>
                <td>{hex(row.word, 8)}</td>
                <td className={row.valid ? undefined : "invalid-instruction"}>
                  {row.labels.length > 0 && <span className="labels">{row.labels.join(", ")}: </span>}
                  {row.instruction}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function hex(value: number, width: number): string {
  return (value >>> 0).toString(16).padStart(width, "0");
}

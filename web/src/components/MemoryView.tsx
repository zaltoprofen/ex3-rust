import { type FormEvent, useEffect, useState } from "react";
import type { MemoryRow } from "../ex3/types";

interface MemoryViewProps {
  stackRows: MemoryRow[];
  selectedRows: MemoryRow[];
  sp: number | null;
  selectedAddress: number;
  disabled: boolean;
  onSelectAddress(address: number): void;
}

export function MemoryView({
  stackRows,
  selectedRows,
  sp,
  selectedAddress,
  disabled,
  onSelectAddress,
}: MemoryViewProps) {
  const [address, setAddress] = useState(hex(selectedAddress, 4));
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => setAddress(hex(selectedAddress, 4)), [selectedAddress]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const normalized = address.trim().replace(/^0x/i, "");
    if (!/^[0-9a-f]{1,4}$/i.test(normalized)) {
      setValidationError("Enter a 16-bit hexadecimal address (0000–ffff).");
      return;
    }
    setValidationError(null);
    onSelectAddress(Number.parseInt(normalized, 16));
  };

  return (
    <section className="memory-group">
      <MemoryTable title="Stack memory" rows={stackRows} marker={sp} markerLabel="SP" />
      <section className="panel data-panel">
        <div className="panel-heading-row">
          <h2>Memory</h2>
          <form onSubmit={submit}>
            <label>
              Address
              <input
                aria-label="Memory address"
                value={address}
                disabled={disabled}
                onChange={(event) => setAddress(event.target.value)}
              />
            </label>
            <button type="submit" disabled={disabled}>
              Go
            </button>
          </form>
        </div>
        {validationError && <p className="memory-error">{validationError}</p>}
        <MemoryRows rows={selectedRows} marker={selectedAddress} markerLabel="START" />
      </section>
    </section>
  );
}

function MemoryTable({
  title,
  rows,
  marker,
  markerLabel,
}: {
  title: string;
  rows: MemoryRow[];
  marker: number | null;
  markerLabel: string;
}) {
  return (
    <section className="panel data-panel">
      <h2>{title}</h2>
      <MemoryRows rows={rows} marker={marker} markerLabel={markerLabel} />
    </section>
  );
}

function MemoryRows({
  rows,
  marker,
  markerLabel,
}: {
  rows: MemoryRow[];
  marker: number | null;
  markerLabel: string;
}) {
  return (
    <div className="table-scroll memory-scroll">
      <table>
        <thead>
          <tr>
            <th aria-label="marker" />
            <th>Address</th>
            <th>Value</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const marked = row.address === marker;
            return (
              <tr className={marked ? "active-row" : undefined} key={row.address}>
                <td className="markers">{marked ? markerLabel : ""}</td>
                <td>{hex(row.address, 4)}</td>
                <td>{hex(row.word, 8)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function hex(value: number, width: number): string {
  return (value >>> 0).toString(16).padStart(width, "0");
}

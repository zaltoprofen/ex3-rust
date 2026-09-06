import { type ReactNode, useEffect, useState } from "react";
import type { MemoryRow, StackViewSnapshot } from "../ex3/types";
import { StackFrame } from "./StackFrame";

interface StackViewProps {
  snapshot: StackViewSnapshot | null;
  error: string | null;
  loading: boolean;
  memorySelectionDisabled?: boolean;
  onSelectAddress(address: number): void;
}

export function StackView({ snapshot, error, loading, memorySelectionDisabled = false, onSelectAddress }: StackViewProps) {
  const [selectedFrame, setSelectedFrame] = useState(0);
  const [expandedFrames, setExpandedFrames] = useState<ReadonlySet<number>>(
    () => new Set([0]),
  );

  useEffect(() => {
    setSelectedFrame((current) => clampFrameIndex(current, snapshot?.frames.length ?? 0));
  }, [snapshot?.frames.length]);

  if (loading && snapshot === null) {
    return <StackPanel><p className="stack-empty">Loading stack view…</p></StackPanel>;
  }
  if (error) {
    return (
      <StackPanel>
        <p className="stack-warning" role="status">Stack View unavailable: {error}</p>
      </StackPanel>
    );
  }
  if (snapshot === null) {
    return <StackPanel><p className="stack-empty">Compile a program to inspect its stack.</p></StackPanel>;
  }

  const frame = snapshot.frames[selectedFrame];
  return (
    <StackPanel>
      <div className="stack-context">
        <span>Context: {formatContext(snapshot)}</span>
        <span>PC: 0x{hex16(snapshot.pc)}</span>
        <span>SP: 0x{hex16(snapshot.sp)}</span>
      </div>
      {loading && <p className="stack-empty" role="status">Refreshing stack view…</p>}
      {snapshot.warnings.map((warning, index) => (
        <p className="stack-warning" role="status" key={`${index}-${warning}`}>{warning}</p>
      ))}

      {snapshot.available && snapshot.frames.length > 0 ? (
        <>
          <nav className="call-stack" aria-label="Call Stack">
            <h3>Call Stack</h3>
            <ol>
              {snapshot.frames.map((entry, index) => (
                <li key={`${index}-${entry.functionName}-${entry.frameSp}`}>
                  <button
                    type="button"
                    className={selectedFrame === index ? "selected-frame" : undefined}
                    aria-pressed={selectedFrame === index}
                    aria-label={`Select ${entry.current ? "current" : "suspended"} frame ${index + 1}: ${entry.functionName}`}
                    onClick={() => {
                      setSelectedFrame(index);
                      setExpandedFrames((current) => new Set(current).add(index));
                    }}
                  >
                    <span aria-hidden="true">{entry.current ? "▶" : "○"}</span>
                    <span>{entry.functionName}</span>
                    <small>{entry.current ? "current" : "suspended"}</small>
                  </button>
                </li>
              ))}
            </ol>
          </nav>
          {frame && (
            <StackFrame
              frame={frame}
              expanded={expandedFrames.has(selectedFrame)}
              memorySelectionDisabled={memorySelectionDisabled}
              onToggle={() => setExpandedFrames((current) => toggleFrame(current, selectedFrame))}
              onSelectAddress={onSelectAddress}
            />
          )}
        </>
      ) : (
        <FallbackStack rows={snapshot.rawStack} disabled={memorySelectionDisabled} onSelectAddress={onSelectAddress} />
      )}
    </StackPanel>
  );
}

function StackPanel({ children }: { children: ReactNode }) {
  return <section className="panel data-panel stack-view"><h2>Stack View</h2>{children}</section>;
}

function FallbackStack({ rows, disabled, onSelectAddress }: { rows: MemoryRow[]; disabled: boolean; onSelectAddress(address: number): void }) {
  return (
    <section className="raw-stack-fallback">
      <p className="stack-unavailable">Semantic C frame unavailable.</p>
      <h3>Raw stack fallback</h3>
      {rows.length === 0 ? <p className="stack-empty">No stack words available.</p> : (
        <div className="slot-table-scroll"><table><thead><tr><th>Address</th><th>Raw value</th></tr></thead>
          <tbody>{rows.map((row) => <tr key={row.address}><td><button type="button" className="address-link" disabled={disabled} aria-label={`Show raw memory at 0x${hex16(row.address)}`} onClick={() => onSelectAddress(row.address)}>{hex16(row.address)}</button></td><td>0x{hex32(row.word)}</td></tr>)}</tbody>
        </table></div>
      )}
    </section>
  );
}

export function clampFrameIndex(index: number, frameCount: number): number {
  return frameCount === 0 ? 0 : Math.min(Math.max(index, 0), frameCount - 1);
}

export function toggleFrame(current: ReadonlySet<number>, index: number): ReadonlySet<number> {
  const next = new Set(current);
  if (next.has(index)) next.delete(index);
  else next.add(index);
  return next;
}

function formatContext(snapshot: StackViewSnapshot): string {
  return snapshot.contextSymbol
    ? `${snapshot.context} (${snapshot.contextSymbol})`
    : snapshot.context;
}

function hex16(value: number): string {
  return (value & 0xffff).toString(16).padStart(4, "0");
}

function hex32(value: number): string {
  return (value >>> 0).toString(16).padStart(8, "0");
}

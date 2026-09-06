import type { StackFrameDto, StackSlotDto, StackSlotKind } from "../ex3/types";
import { StackSlot } from "./StackSlot";

interface StackFrameProps {
  frame: StackFrameDto;
  expanded: boolean;
  memorySelectionDisabled: boolean;
  onToggle(): void;
  onSelectAddress(address: number): void;
}

const sections: { title: string; kinds: StackSlotKind[] }[] = [
  { title: "Parameters", kinds: ["parameter"] },
  { title: "Control / return address", kinds: ["return-address"] },
  { title: "Locals", kinds: ["local"] },
  { title: "Temporaries", kinds: ["temporary"] },
  { title: "Outgoing Call", kinds: ["outgoing-argument"] },
  { title: "Runtime Arguments", kinds: ["runtime-argument"] },
  { title: "Other slots", kinds: ["unknown"] },
];

export function StackFrame({
  frame,
  expanded,
  memorySelectionDisabled,
  onToggle,
  onSelectAddress,
}: StackFrameProps) {
  return (
    <section className="stack-frame" aria-label={`${frame.functionName} frame`}>
      <div className="frame-heading">
        <div>
          <h3>{frame.functionName}</h3>
          <span className={frame.current ? "current-badge" : "suspended-badge"}>
            {frame.current ? "current frame" : "suspended frame"}
          </span>
        </div>
        <button
          type="button"
          className="secondary-button"
          aria-expanded={expanded}
          aria-label={`${expanded ? "Collapse" : "Expand"} ${frame.functionName} frame details`}
          onClick={onToggle}
        >
          {expanded ? "Collapse" : "Expand"}
        </button>
      </div>

      <dl className="frame-registers">
        <div><dt>PC</dt><dd>{frame.pc === null ? "—" : `0x${hex16(frame.pc)}`}</dd></div>
        <div><dt>Current SP</dt><dd>0x{hex16(frame.currentSp)}</dd></div>
        <div><dt>Frame SP</dt><dd>0x{hex16(frame.frameSp)}</dd></div>
        <div>
          <dt>Return to</dt>
          <dd>
            {frame.returnAddress === null ? "—" : `0x${hex16(frame.returnAddress)}`}
            {frame.returnSymbol ? ` → ${frame.returnSymbol}` : ""}
          </dd>
        </div>
      </dl>

      {expanded && (
        <div className="frame-details">
          {sections.map(({ title, kinds }) => {
            const slots = sortSlots(
              frame.slots.filter((slot) => kinds.includes(slot.kind)),
            );
            return slots.length > 0 ? (
              <SlotSection
                key={title}
                title={title}
                slots={slots}
                memorySelectionDisabled={memorySelectionDisabled}
                onSelectAddress={onSelectAddress}
              />
            ) : null;
          })}
          {frame.slots.length === 0 && <p className="stack-empty">No frame slots.</p>}
        </div>
      )}
    </section>
  );
}

function SlotSection({
  title,
  slots,
  memorySelectionDisabled,
  onSelectAddress,
}: {
  title: string;
  slots: StackSlotDto[];
  memorySelectionDisabled: boolean;
  onSelectAddress(address: number): void;
}) {
  const target = slots.find((slot) => slot.callTarget)?.callTarget;
  return (
    <section className="slot-section">
      <h4>
        {title}
        {target ? <span className="call-target">Target: {target}</span> : null}
      </h4>
      <div className="slot-table-scroll">
        <table>
          <thead>
            <tr>
              <th>Addr</th><th>Off</th><th>Name</th><th>Type</th>
              <th>Value</th><th>Raw</th><th>State</th>
            </tr>
          </thead>
          <tbody>
            {slots.map((slot) => (
              <StackSlot
                key={`${slot.kind}-${slot.address}-${slot.name}`}
                slot={slot}
                disabled={memorySelectionDisabled}
                onSelectAddress={onSelectAddress}
              />
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

export function sortSlots(slots: StackSlotDto[]): StackSlotDto[] {
  if (!slots.every((slot) => slot.kind.endsWith("argument"))) return slots;
  return [...slots].sort(
    (left, right) => (left.argumentIndex ?? Number.MAX_SAFE_INTEGER) - (right.argumentIndex ?? Number.MAX_SAFE_INTEGER),
  );
}

function hex16(value: number): string {
  return (value & 0xffff).toString(16).padStart(4, "0");
}

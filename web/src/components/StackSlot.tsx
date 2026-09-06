import type { StackSlotDto } from "../ex3/types";

interface StackSlotProps {
  slot: StackSlotDto;
  disabled: boolean;
  onSelectAddress(address: number): void;
}

export function StackSlot({ slot, disabled, onSelectAddress }: StackSlotProps) {
  const state = formatState(slot);
  const rowClass = slot.active
    ? "active-slot"
    : ["not-allocated", "released"].includes(slot.state)
      ? "invalid-storage-slot"
      : undefined;
  return (
    <tr className={rowClass}>
      <td>
        <button
          type="button"
          className="address-link"
          disabled={disabled}
          aria-label={`Show raw memory at 0x${hex(slot.address, 4)}`}
          onClick={() => onSelectAddress(slot.address)}
        >
          {hex(slot.address, 4)}
        </button>
      </td>
      <td>{formatOffset(slot.frameOffset)}</td>
      <td>
        <span>{slot.name}</span>
        {slot.description && slot.description !== slot.name && (
          <small className="slot-description">{slot.description}</small>
        )}
      </td>
      <td>{slot.typeName ?? "—"}</td>
      <td>{primaryValue(slot)}</td>
      <td>0x{hex(slot.rawValue, 8)}</td>
      <td className={slot.active === false ? "inactive-slot" : undefined}>{state}</td>
    </tr>
  );
}

export function primaryValue(slot: StackSlotDto): string {
  if (slot.kind === "return-address") return `0x${hex(slot.rawValue & 0xffff, 4)}`;
  if (!["value", "current-storage"].includes(slot.state)) return "—";
  if (slot.signedValue !== null) return String(slot.signedValue);
  if (slot.unsignedValue !== null) return String(slot.unsignedValue);
  return "—";
}

export function formatState(slot: StackSlotDto): string {
  switch (slot.state) {
    case "value":
      return slot.kind === "temporary" ? "active" : "";
    case "current-storage":
      return "";
    case "inactive-scratch":
      return "inactive / scratch";
    case "not-allocated":
      return "not allocated";
    case "released":
      return "released";
    case "control":
      return "control";
    case "unknown":
      return "unknown";
  }
}

export function formatOffset(offset: number): string {
  return offset >= 0 ? `+${offset}` : String(offset);
}

function hex(value: number, width: number): string {
  return (value >>> 0).toString(16).padStart(width, "0");
}

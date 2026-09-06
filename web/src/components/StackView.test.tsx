import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { StackFrameDto, StackSlotDto, StackViewSnapshot } from "../ex3/types";
import { sortSlots } from "./StackFrame";
import { formatOffset, primaryValue } from "./StackSlot";
import { clampFrameIndex, StackView, toggleFrame } from "./StackView";

const slot = (overrides: Partial<StackSlotDto>): StackSlotDto => ({
  address: 0xfff8,
  frameOffset: 0,
  kind: "local",
  name: "value",
  typeName: "int32_t",
  rawValue: 1,
  signedValue: 1,
  unsignedValue: null,
  active: null,
  description: null,
  argumentIndex: null,
  callTarget: null,
  ...overrides,
});

const frame = (overrides: Partial<StackFrameDto> = {}): StackFrameDto => ({
  functionName: "fact",
  current: true,
  pc: 0x120,
  currentSp: 0xffe8,
  frameSp: 0xffe8,
  returnAddress: 0x13c,
  returnSymbol: "fact+0x18",
  slots: [],
  ...overrides,
});

const snapshot = (overrides: Partial<StackViewSnapshot> = {}): StackViewSnapshot => ({
  available: true,
  context: "c-function",
  contextSymbol: "fact",
  pc: 0x120,
  sp: 0xffe8,
  frames: [frame()],
  rawStack: [],
  warnings: [],
  ...overrides,
});

describe("Stack View", () => {
  it("renders recursive current and suspended frames as separate call-stack entries", () => {
    const html = renderToStaticMarkup(
      <StackView
        snapshot={snapshot({
          frames: [frame(), frame({ current: false, pc: 0x13c, frameSp: 0xffed })],
        })}
        error={null}
        loading={false}
        onSelectAddress={vi.fn()}
      />,
    );

    expect(html).toContain("Select current frame 1: fact");
    expect(html).toContain("Select suspended frame 2: fact");
    expect(html).toContain("current frame");
    expect(html).toContain("Current SP");
    expect(html).toContain("Frame SP");
    expect(html).toContain("fact+0x18");
  });

  it("renders typed slots, activity, outgoing calls, and address links", () => {
    const slots = [
      slot({ kind: "parameter", name: "signed", signedValue: -1, rawValue: 0xffffffff, frameOffset: 5 }),
      slot({ kind: "local", name: "unsigned", typeName: "uint32_t", signedValue: null, unsignedValue: 4294967295, rawValue: 0xffffffff }),
      slot({ kind: "temporary", name: "temporary #0", active: true, description: "value of n - 1" }),
      slot({ kind: "temporary", name: "temporary #1", active: false, signedValue: null, rawValue: 2 }),
      slot({ kind: "outgoing-argument", name: "second", frameOffset: -1, argumentIndex: 1, callTarget: "sum", address: 0xffe7 }),
      slot({ kind: "outgoing-argument", name: "first", frameOffset: -2, argumentIndex: 0, callTarget: "sum", address: 0xffe6 }),
      slot({ kind: "runtime-argument", name: "lhs", argumentIndex: 0, callTarget: "__ex3_mul_i32" }),
    ];
    const html = renderToStaticMarkup(
      <StackView snapshot={snapshot({ frames: [frame({ slots })] })} error={null} loading={false} onSelectAddress={vi.fn()} />,
    );

    expect(html).toContain("-1");
    expect(html).toContain("4294967295");
    expect(html).toContain("0xffffffff");
    expect(html).toContain("inactive / scratch");
    expect(html).toContain("Outgoing Call");
    expect(html).toContain("Target: sum");
    expect(html).toContain("Runtime Arguments");
    expect(html).toContain("Show raw memory at 0xffe7");
    expect(html.indexOf(">first<")).toBeLessThan(html.indexOf(">second<"));
  });

  it("shows degraded context, warnings, and raw stack fallback", () => {
    const html = renderToStaticMarkup(
      <StackView
        snapshot={snapshot({
          available: false,
          context: "runtime",
          contextSymbol: "__ex3_mul_i32",
          frames: [],
          rawStack: [{ address: 0xfff0, word: 0x12345678 }],
          warnings: ["C metadata is unavailable"],
        })}
        error={null}
        loading={false}
        onSelectAddress={vi.fn()}
      />,
    );

    expect(html).toContain("runtime (__ex3_mul_i32)");
    expect(html).toContain("C metadata is unavailable");
    expect(html).toContain("Semantic C frame unavailable");
    expect(html).toContain("0x12345678");
  });

  it("supports frame selection bounds and independent expand/collapse state", () => {
    expect(clampFrameIndex(4, 2)).toBe(1);
    expect(clampFrameIndex(-1, 2)).toBe(0);
    expect(clampFrameIndex(1, 0)).toBe(0);
    expect(toggleFrame(new Set([0]), 0).has(0)).toBe(false);
    expect(toggleFrame(new Set<number>(), 1).has(1)).toBe(true);
  });

  it("formats values and logical outgoing argument order", () => {
    expect(primaryValue(slot({ signedValue: -1 }))).toBe("-1");
    expect(primaryValue(slot({ signedValue: null, unsignedValue: 0xffffffff }))).toBe("4294967295");
    expect(primaryValue(slot({ kind: "temporary", active: false }))).toBe("—");
    expect(formatOffset(3)).toBe("+3");
    expect(formatOffset(-2)).toBe("-2");
    const outgoing = sortSlots([
      slot({ kind: "outgoing-argument", argumentIndex: 2 }),
      slot({ kind: "outgoing-argument", argumentIndex: 0 }),
    ]);
    expect(outgoing.map((entry) => entry.argumentIndex)).toEqual([0, 2]);
  });
});

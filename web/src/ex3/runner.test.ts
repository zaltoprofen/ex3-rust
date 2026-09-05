import { describe, expect, it, vi } from "vitest";
import { createInitialMachineState } from "../state/machine";
import { compileMachine, errorState, stepMachine } from "./runner";
import type { CpuSnapshot, Ex3SessionApi } from "./types";

const snapshot = (overrides: Partial<CpuSnapshot> = {}): CpuSnapshot => ({
  pc: 0x10,
  sp: 0,
  ac: 0,
  ir: 0,
  psr: 0,
  ien: false,
  negative: false,
  zero: false,
  carry: false,
  overflow: false,
  halted: false,
  interruptPending: false,
  executedInstructions: 0,
  serialSelected: false,
  interruptMask: 0,
  inputRegister: 0,
  assemblyLine: 1,
  ...overrides,
});

describe("machine runner", () => {
  it("stores generated assembly and snapshot after compile", () => {
    const session: Ex3SessionApi = {
      compile_and_load: vi.fn(() => ({
        assembly: "ORG 0x0010\nHLT\nEND\n",
        symbols: [],
        loadedWords: 1,
        snapshot: snapshot(),
      })),
      step: vi.fn(),
    };
    const initial = createInitialMachineState();

    const next = compileMachine(session, initial);

    expect(session.compile_and_load).toHaveBeenCalledWith(initial.source);
    expect(next.assembly).toContain("HLT");
    expect(next.snapshot?.pc).toBe(0x10);
    expect(next.phase).toBe("ready");
  });

  it("uses the WASM step snapshot as the next machine state", () => {
    const session: Ex3SessionApi = {
      compile_and_load: vi.fn(),
      step: vi.fn(() => ({
        outcome: "executed" as const,
        pcBefore: 0x10,
        instruction: "CALL 0012",
        snapshot: snapshot({ pc: 0x12, executedInstructions: 1 }),
      })),
    };
    const initial = { ...createInitialMachineState(), phase: "ready" as const };

    const next = stepMachine(session, initial);

    expect(next.snapshot?.pc).toBe(0x12);
    expect(next.snapshot?.executedInstructions).toBe(1);
    expect(next.phase).toBe("ready");
  });

  it("keeps structured compiler diagnostics", () => {
    const initial = createInitialMachineState();
    const next = errorState(initial, {
      stage: "compiler",
      message: "compile failed",
      diagnostics: [{ line: 2, column: 5, message: "expected expression" }],
    });

    expect(next.phase).toBe("error");
    expect(next.errorMessage).toBe("compile failed");
    expect(next.diagnostics).toEqual([
      { line: 2, column: 5, message: "expected expression" },
    ]);
  });
});

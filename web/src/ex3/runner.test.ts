import { describe, expect, it, vi } from "vitest";
import { createInitialMachineState } from "../state/machine";
import {
  compileMachine,
  errorState,
  resetMachine,
  runMachineChunk,
  selectMemory,
  stepMachine,
  toggleBreakpoint,
} from "./runner";
import type {
  CpuSnapshot,
  DisassemblyRow,
  Ex3SessionApi,
  MemoryRow,
} from "./types";

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

const memoryRows = (start: number, count: number): MemoryRow[] =>
  Array.from({ length: count }, (_, offset) => ({
    address: (start + offset) & 0xffff,
    word: offset,
  }));

const disassemblyRows = (start: number, count: number): DisassemblyRow[] =>
  Array.from({ length: count }, (_, offset) => ({
    address: (start + offset) & 0xffff,
    word: 0x84000000,
    instruction: "HLT",
    valid: true,
    sourceLine: offset + 1,
    labels: [],
  }));

function fakeSession(overrides: Partial<Ex3SessionApi> = {}): Ex3SessionApi {
  return {
    compile_and_load: vi.fn(() => ({
      assembly: "ORG 0x0010\nHLT\nEND\n",
      symbols: [],
      sourceMap: [{ address: 0x10, line: 2, executable: true }],
      loadedWords: 1,
      snapshot: snapshot(),
    })),
    reset: vi.fn(() => snapshot()),
    step: vi.fn(() => ({
      outcome: "executed" as const,
      pcBefore: 0x10,
      instruction: "HLT",
      snapshot: snapshot({ pc: 0x11, executedInstructions: 1 }),
    })),
    run_chunk: vi.fn(() => ({
      status: "running" as const,
      executed: 2_000,
      breakpointAddress: null,
      snapshot: snapshot({ pc: 0x20, executedInstructions: 2_000 }),
    })),
    memory_range: vi.fn(memoryRows),
    disassembly_range: vi.fn(disassemblyRows),
    toggle_breakpoint: vi.fn(() => true),
    clear_breakpoints: vi.fn(),
    breakpoints: vi.fn(() => []),
    serial_output: vi.fn(() => ""),
    ...overrides,
  };
}

describe("machine runner", () => {
  it("stores generated assembly, source map, and machine views after compile", () => {
    const session = fakeSession();
    const initial = createInitialMachineState();

    const next = compileMachine(session, initial);

    expect(session.compile_and_load).toHaveBeenCalledWith(initial.source);
    expect(next.assembly).toContain("HLT");
    expect(next.sourceMap).toEqual([{ address: 0x10, line: 2, executable: true }]);
    expect(next.snapshot?.pc).toBe(0x10);
    expect(next.disassembly).toHaveLength(16);
    expect(next.stackMemory).toHaveLength(24);
    expect(next.phase).toBe("ready");
  });

  it("uses the WASM step snapshot as the next machine state", () => {
    const session = fakeSession();
    const initial = { ...createInitialMachineState(), phase: "ready" as const };

    const next = stepMachine(session, initial);

    expect(next.snapshot?.pc).toBe(0x11);
    expect(next.snapshot?.executedInstructions).toBe(1);
    expect(session.disassembly_range).toHaveBeenCalledWith(0x0d, 16);
    expect(next.phase).toBe("ready");
  });

  it("maps chunk status and accumulates the run safety count", () => {
    const session = fakeSession({
      run_chunk: vi.fn(() => ({
        status: "breakpoint" as const,
        executed: 12,
        breakpointAddress: 0x44,
        snapshot: snapshot({ pc: 0x44, executedInstructions: 12 }),
      })),
    });
    const initial = {
      ...createInitialMachineState(),
      phase: "running" as const,
      runInstructionCount: 100,
    };

    const update = runMachineChunk(session, initial, 2_000);

    expect(update.state.phase).toBe("paused");
    expect(update.state.runInstructionCount).toBe(112);
    expect(update.state.stopMessage).toContain("0x0044");
  });

  it("reset refreshes memory and serial state", () => {
    const session = fakeSession({
      reset: vi.fn(() => snapshot()),
      serial_output: vi.fn(() => ""),
    });
    const initial = {
      ...createInitialMachineState(),
      phase: "halted" as const,
      serialOutput: "old output",
    };

    const next = resetMachine(session, initial);

    expect(next.phase).toBe("ready");
    expect(next.serialOutput).toBe("");
    expect(next.runInstructionCount).toBe(0);
  });

  it("selects wrapped memory and synchronizes breakpoints", () => {
    const session = fakeSession({ breakpoints: vi.fn(() => [0x10]) });
    let state = compileMachine(session, createInitialMachineState());
    state = selectMemory(session, state, 0x1_0010);
    state = toggleBreakpoint(session, state, 0x10);

    expect(state.selectedMemoryAddress).toBe(0x10);
    expect(session.memory_range).toHaveBeenCalledWith(0x10, 32);
    expect(state.breakpoints).toEqual([0x10]);
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

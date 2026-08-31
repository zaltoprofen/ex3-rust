# EX3 v3.0 ABI Specification

**Status:** Frozen draft for implementation  
**ABI version:** EX3 v3.0  
**Target ISA:** EX3 v3.0

---

## 1. Scope

This document defines the Application Binary Interface for EX3 v3.0 software.

It specifies:

- stack conventions,
- function argument passing,
- function return values,
- register/flag preservation,
- stack-frame layout,
- subroutine prologue/epilogue rules,
- pointer representation,
- interrupt-handler preservation rules,
- vector-area software conventions.

The machine-level instruction semantics are defined in the separate **EX3 v3.0 ISA Specification**.

---

## 2. Fundamental ABI properties

| Property | EX3 v3.0 ABI |
|---|---|
| Stack direction | Downward |
| Stack slot size | 1 word = 32 bits |
| Stack alignment | 1 word |
| Argument transport | Stack |
| Argument order | Right-to-left |
| Argument cleanup | Caller |
| Return address | Pushed by `CALL` |
| Scalar return value | `AC` |
| `AC` preservation | Caller-saved / volatile |
| `NZCV` preservation | Caller-saved / volatile |
| `SP` preservation | Callee must restore |
| `IEN/IMSK/I/O control` | Preserved by ordinary functions |
| Frame pointer | None |
| Index register | None |
| Local storage | Allocated with `ADJSP` |
| Interrupt PC/PSR save | Hardware |
| Interrupt AC preservation | Handler responsibility |

---

## 3. Stack convention

The stack grows toward lower addresses.

Conceptual push:

```text
SP <- SP - 1
M[SP] <- value
```

Conceptual pop:

```text
value <- M[SP]
SP <- SP + 1
```

SP arithmetic is modulo `2^16`.

The ABI assumes correct software stack management. No architectural stack-limit register or stack overflow trap exists.

---

## 4. Stack slot representation

One ABI stack slot is one 32-bit word.

### 4.1 32-bit scalar values

A 32-bit scalar occupies one slot.

### 4.2 16-bit addresses/pointers

Canonical pointer representation:

```text
31                  16 15                 0
+---------------------+--------------------+
|       0x0000        | address16          |
+---------------------+--------------------+
```

The upper 16 bits must be zero in a canonical pointer value.

### 4.3 Smaller scalar types

For an initial C-like ABI, scalar values smaller than 32 bits should still occupy one full 32-bit stack slot.

Exact language-level signed/unsigned extension rules are a compiler ABI concern, but each argument slot remains one word.

### 4.4 Wider values

A value wider than 32 bits occupies multiple consecutive stack words. Detailed layout for such types is not standardized by ABI v3.0 and must be defined by a language-specific ABI extension.

---

## 5. Function arguments

Arguments are pushed **right-to-left**.

For:

```c
foo(a, b, c);
```

the conceptual caller sequence is:

```asm
LDA c
PUSH

LDA b
PUSH

LDA a
PUSH

CALL FOO
```

`PUSH` is a pseudo instruction:

```asm
ADJSP -1
STSP 0
```

Immediately on entry to `FOO`:

```text
SP+0 = return address
SP+1 = argument 1 (a)
SP+2 = argument 2 (b)
SP+3 = argument 3 (c)
```

This entry layout is an ABI invariant.

---

## 6. CALL

`CALL` itself pushes the return address.

Architecturally:

```text
SP <- SP - 1
M[SP] <- zero_extend(PC)
PC <- target
```

The return-address slot belongs to the call mechanism and must not be removed by the callee before `RET`.

---

## 7. Caller cleanup

Function arguments are removed by the caller.

Example with three arguments:

```asm
CALL FOO
ADJSP 3
```

After argument cleanup:

```text
SP == SP value before arguments were pushed
```

The ABI intentionally uses caller cleanup so that:

- `RET` remains simple,
- variadic functions can be supported by language ABIs,
- callees do not need encoded argument counts.

---

## 8. Return values

A scalar return value of up to 32 bits is returned in `AC`.

Example:

```asm
FOO:
    ...
    LDA result
    RET
```

The caller observes the return value in `AC` after `CALL` returns.

A 16-bit pointer return value must use the canonical pointer representation with upper 16 bits zero.

Aggregate and multiword return conventions are not standardized in ABI v3.0 and must be defined by a language-specific ABI extension.

---

## 9. Volatile architectural state

### 9.1 AC

`AC` is **caller-saved / volatile**.

A caller must assume:

```text
AC after CALL = unspecified except for the defined return value
```

If a live AC value must survive a call, the caller must save it in memory or a stack slot before the call.

### 9.2 NZCV

`N`, `Z`, `C`, and `V` are caller-saved / volatile.

A caller must never rely on a comparison performed before a function call remaining valid afterward.

Invalid pattern:

```asm
CMP VALUE
CALL FOO
BEQ TARGET
```

Valid pattern:

```asm
CALL FOO
CMP VALUE
BEQ TARGET
```

### 9.3 SP

`SP` is callee-preserved.

If the callee-entry SP is `S`, then immediately before executing `RET`:

```text
SP == S
```

must hold.

This is mandatory because `M[S]` contains the return address.

---

## 10. Processor control state

Ordinary ABI-conforming functions must preserve:

- `IEN`,
- `IMSK`,
- serial/parallel I/O selection,
- other implementation-defined persistent I/O control state.

Functions whose documented purpose is to modify processor control state are exempt, but that side effect must be part of their interface contract.

`NZCV` is not included in this preservation rule because it is explicitly volatile.

---

## 11. Local stack frame

A callee allocates all fixed local storage in one operation whenever possible.

For `L` local words:

```asm
ADJSP -L
```

After allocation:

```text
SP+0     = local 0
SP+1     = local 1
...
SP+L-1   = local L-1
SP+L     = return address
SP+L+1   = argument 1
SP+L+2   = argument 2
...
```

Canonical layout:

```text
                     higher addresses

        +--------------------------+
SP+L+N  | argument N               |
        +--------------------------+
        | ...                      |
        +--------------------------+
SP+L+2  | argument 2               |
        +--------------------------+
SP+L+1  | argument 1               |
        +--------------------------+
SP+L    | return address           |
        +--------------------------+
SP+L-1  | local L-1                |
        +--------------------------+
        | ...                      |
        +--------------------------+
SP+1    | local 1                  |
        +--------------------------+
SP+0    | local 0                  | <- SP
        +--------------------------+

                     lower addresses
```

---

## 12. Function prologue and epilogue

### 12.1 Canonical prologue

For three local words:

```asm
FUNC:
    ADJSP -3
```

### 12.2 Canonical epilogue

```asm
    ADJSP 3
    RET
```

The callee must release all local and temporary stack allocation before `RET`.

---

## 13. SP-relative access

Local variables and arguments are accessed using SP-relative operations.

Examples:

```asm
LDSP 0
STSP 1
ADDSP 2
SUBSP 3
ANDSP 4
ORSP 5
XORSP 6
CMPSP 7
```

No dedicated frame pointer exists.

A compiler must therefore track any temporary changes to `SP`, including temporary argument pushes for nested calls.

For this reason, compilers are encouraged to allocate fixed local space once in the prologue and avoid unnecessary transient SP movement inside a function.

---

## 14. Nested calls

Nested and recursive calls are supported because every `CALL` pushes an independent return address.

When a function makes another call, any outgoing arguments are pushed below its current frame.

A compiler must account for the temporary SP displacement when referencing the current function's own locals and arguments.

After the nested callee returns and caller cleanup completes, the original frame offsets become valid again.

---

## 15. PUSH and POP pseudo instructions

### 15.1 PUSH

```asm
PUSH
```

expands exactly to:

```asm
ADJSP -1
STSP 0
```

It pushes `AC`.

### 15.2 POP

```asm
POP
```

expands exactly to:

```asm
LDSP 0
ADJSP 1
```

It pops into `AC`.

Because `LDSP` updates `N/Z`, `POP` also updates `N/Z`; `C/V` remain unchanged.

`PUSH` does not modify `NZCV`.

---

## 16. Recommended ordinary function pattern

Example conceptual function:

```c
int add(int a, int b) {
    int result;
    result = a + b;
    return result;
}
```

Possible EX3 v3 sequence:

```asm
ADD_FUNC:
    ADJSP -1

    ; SP+0 = result
    ; SP+1 = return address
    ; SP+2 = a
    ; SP+3 = b

    LDSP 2
    ADDSP 3
    STSP 0

    LDSP 0

    ADJSP 1
    RET
```

Caller:

```asm
LDA b
PUSH

LDA a
PUSH

CALL ADD_FUNC
ADJSP 2

; AC contains return value
```

---

## 17. Tail calls

ABI v3.0 does not define a special tail-call instruction.

A compiler may perform a tail-call optimization only when it can establish the correct target stack layout and preserve all ABI invariants.

A normal `JMP` may be used after appropriately dismantling/reusing the current frame.

---

## 18. Interrupt transparency

Interrupt entry uses the same stack but is architecturally separate from ordinary function calls.

Hardware automatically saves:

1. `PSR`
2. `PC`

using:

```text
SP <- SP - 1
M[SP] <- PSR

SP <- SP - 1
M[SP] <- zero_extend(PC)
```

After entry:

```text
SP+0 = saved PC
SP+1 = saved PSR
```

`IRET` removes exactly these two slots.

Therefore an interrupt may occur while any ordinary function frame is active without changing that function's ABI-visible stack layout after `IRET`.

---

## 19. Interrupt-handler ABI

### 19.1 Hardware-preserved state

The hardware automatically preserves:

- `PC`,
- full architectural `PSR` (`IEN,N,Z,C,V`).

### 19.2 Handler-preserved state

An interrupt handler must preserve any other architectural state that it modifies if interrupted software is expected to resume transparently.

In particular:

- `AC` must be preserved by a normal interrupt handler if the handler modifies it.
- `IMSK` must be restored if modified.
- I/O selection/control state must be restored if modified.

Canonical AC-preserving handler pattern:

```asm
IRQ_HANDLER:
    PUSH

    ; handler body

    POP
    IRET
```

Because `POP` updates `N/Z`, this does **not** harm interrupted code: the subsequent `IRET` restores the original PSR, including original `NZCV`.

### 19.3 IRET

An interrupt handler must terminate with `IRET`, not `RET`.

`IRET` restores the interrupted PC and PSR, including the previous value of `IEN`.

---

## 20. System Vector Area ABI

The ABI reserves:

```text
0x0000 .. 0x000F
```

for interrupt vectors.

v3.0 defines:

```text
vector 0 = 0x0000 = general/legacy maskable interrupt entry
```

`0x0001..0x000F` are reserved.

Recommended v3.0 layout:

```asm
ORG 0x0000
JMP IRQ_HANDLER

ORG 0x0010
; reset/program entry
```

Handler bodies must not be placed inside the vector area.

This convention preserves forward compatibility with a future CPU that dispatches different interrupt sources directly to different vector slots.

---

## 21. Future vectored-interrupt compatibility

Future EX3 revisions may assign:

```text
0x0000 vector 0
0x0001 vector 1
...
0x000F vector 15
```

Each slot remains an executable instruction location rather than a raw pointer entry.

A v3.0 binary that uses only:

```asm
ORG 0x0000
JMP COMMON_IRQ
```

remains compatible on a future CPU when operating through vector 0.

The interrupt stack frame and `IRET` semantics must remain unchanged for such compatibility.

---

## 22. Interrupt handler and ordinary function calls

An interrupt handler may call an ordinary ABI function if it first preserves any volatile interrupted state that must remain transparent.

Since ordinary functions may destroy `AC` and `NZCV`:

- interrupted `NZCV` is already safe in the hardware-saved PSR,
- interrupted `AC` must be saved by the handler before calling ordinary functions.

Example:

```asm
IRQ_HANDLER:
    PUSH
    CALL SERVICE_IRQ
    POP
    IRET
```

Any arguments to `SERVICE_IRQ` follow the normal ABI and require caller cleanup.

---

## 23. Variadic functions

Caller cleanup and right-to-left argument placement permit language-specific variadic calling conventions.

ABI v3.0 does not define metadata for determining the number or type of variadic arguments; that must be supplied by the language/library convention.

The fixed rule remains:

```text
SP+1 = first argument at callee entry
SP+2 = second argument
...
```

---

## 24. Stack overflow and underflow

No architectural stack exception exists.

Software must avoid stack collision with:

- program text,
- global/static data,
- vector area,
- other allocated memory.

Emulators and debuggers are encouraged to provide optional stack-range diagnostics.

---

## 25. Code and data placement

The ABI reserves only:

```text
0x0000..0x000F = System Vector Area
```

and defines reset entry:

```text
0x0010
```

No mandatory split between code, static data, heap, and stack is defined by ABI v3.0.

Toolchains/linkers may define platform-specific memory maps provided they preserve the vector area and reset entry semantics.

---

## 26. ABI conformance summary

An ordinary function conforms to EX3 v3.0 ABI when it:

1. accepts stack arguments in the defined order,
2. treats `AC` and `NZCV` as volatile,
3. restores `SP` to its entry value before `RET`,
4. returns scalar values in `AC`,
5. leaves argument cleanup to the caller,
6. preserves processor control state such as `IEN`, `IMSK`, and I/O selection unless explicitly documented otherwise.

An interrupt handler conforms when it:

1. enters through the vector convention,
2. preserves `AC` and any other non-hardware-saved state it modifies,
3. leaves the hardware interrupt frame intact,
4. terminates with `IRET`.

---

## 27. Quick reference

### Callee entry

```text
SP+0 = return address
SP+1 = argument 1
SP+2 = argument 2
...
```

### With L locals

```text
SP+0     = local 0
...
SP+L-1   = local L-1
SP+L     = return address
SP+L+1   = argument 1
...
```

### Register/state classes

```text
AC        caller-saved
NZCV      caller-saved
SP        callee-restored
IEN       preserved by ordinary functions
IMSK      preserved by ordinary functions
I/O mode  preserved by ordinary functions
```

### Return

```text
result -> AC
RET
```

### Interrupt hardware frame

```text
SP+0 = saved PC
SP+1 = saved PSR
```

### Pseudo stack operations

```text
PUSH = ADJSP -1 ; STSP 0
POP  = LDSP 0   ; ADJSP 1
```

# EX3 v3.0 ISA Specification

**Status:** Frozen draft for implementation  
**ISA version:** EX3 v3.0  
**Instruction width:** 32 bits  
**Data word width:** 32 bits  
**Address width:** 16 bits  
**Addressing unit:** word

---

## 1. Scope

This document defines the EX3 v3.0 Instruction Set Architecture (ISA).

EX3 v3 is a binary-incompatible redesign of the previous EX3 ISA. It returns to a strict accumulator-oriented execution model while adding a 16-bit address space, stack support, modern condition flags, signed and unsigned conditional branches, stack-based subroutine calls, and stack-based interrupt context preservation.

The separate **EX3 v3.0 ABI Specification** defines software calling conventions, stack-frame layout, argument passing, return values, and interrupt-handler obligations.

---

## 2. Design principles

EX3 v3.0 follows these principles:

1. 32-bit fixed-length instructions.
2. 32-bit accumulator (`AC`) as the principal arithmetic register.
3. 16-bit word-addressed unified memory space.
4. 16-bit `PC` and `SP`.
5. No memory-memory arithmetic instruction family.
6. The former N2/N2I and 11/11I formats are removed.
7. Operations are expressed primarily as `AC op operand`.
8. Operand source is separated from operation through instruction formats.
9. Direct, indirect, immediate, and SP-relative operand sources are supported.
10. `NZCV` condition flags and `IEN` form the architectural `PSR`.
11. The legacy `E` flag does not exist.
12. Deprecated/legacy v1/v2 instructions are not part of v3.
13. EX3 v3.0 is binary-incompatible with earlier EX3 versions.

---

## 3. Architectural state

| State | Width | Reset value | Description |
|---|---:|---:|---|
| `AC` | 32 | `0x00000000` | Accumulator |
| `PC` | 16 | `0x0010` | Program Counter |
| `SP` | 16 | `0x0000` | Stack Pointer |
| `PSR` | 32 | `0x00000000` | Program Status Register |
| `IMSK` | 4 | `0x0` | Interrupt mask |
| I/O selection | implementation state | Parallel | Serial/parallel I/O selection |
| CPU run state | - | Running | Running/Halted |

No `FP` or general index register is defined in v3.0.

---

## 4. Memory model

EX3 v3.0 has one unified instruction/data address space:

```text
0x0000 .. 0xFFFF
```

Each address identifies one 32-bit word.

Total architecturally addressable storage:

```text
65536 words × 32 bits = 256 KiB
```

Instruction fetches, normal loads/stores, stack accesses, vector entries, and self-modifying writes all use the same architectural memory.

A store to an address later fetched as an instruction must be observable by subsequent instruction fetches. Cache coherency is outside the scope of v3.0 because no architectural cache is defined.

All `PC`, `SP`, and effective-address arithmetic is modulo `2^16`.

---

## 5. Reserved system vector area

```text
0x0000 .. 0x000F  System Vector Area
0x0010             Reset entry
0x0010 .. 0xFFFF  Program/data/stack address space
```

Software must not place an interrupt-handler body directly in `0x0000..0x000F`.

In v3.0 all maskable interrupts enter through vector 0:

```text
PC <- 0x0000
```

The normal content of `0x0000` is therefore a `JMP` trampoline to the common interrupt handler.

Addresses `0x0001..0x000F` are reserved for future vectored-interrupt extensions.

---

## 6. Program Status Register

### 6.1 Layout

```text
31                              5 4   3   2   1   0
+--------------------------------+---+---+---+---+---+
|            Reserved            |IEN| N | Z | C | V |
+--------------------------------+---+---+---+---+---+
```

| Bit | Name | Meaning |
|---:|---|---|
| 4 | `IEN` | Interrupt Enable |
| 3 | `N` | Negative |
| 2 | `Z` | Zero |
| 1 | `C` | Carry / no-borrow |
| 0 | `V` | Signed overflow |
| 31:5 | Reserved | Architecturally zero |

Reserved PSR bits read as zero. When PSR is restored, reserved bits are ignored and remain zero.

### 6.2 C flag

For addition:

```text
C = carry-out from bit 31
```

For subtraction and comparison:

```text
C = 1 iff unsigned lhs >= rhs
C = 0 iff a borrow is required
```

Thus `C` is a **carry / no-borrow flag**.

---

## 7. Instruction word format

All instructions are one 32-bit word:

```text
31      29 28      24 23              16 15               0
+----------+----------+------------------+------------------+
| format   | opcode   | modifier         | operand16        |
+----------+----------+------------------+------------------+
   3 bits     5 bits        8 bits             16 bits
```

### 7.1 Format assignments

| Value | Binary | Format | Meaning |
|---:|:---:|---|---|
| `0` | `000` | `MEM` | Memory operand |
| `1` | `001` | `IMM` | 16-bit immediate |
| `2` | `010` | `SPREL` | SP-relative memory operand |
| `3` | `011` | `BRANCH` | Direct control transfer |
| `4` | `100` | `SYS` | No-operand/system/I/O |
| `5` | `101` | `EXT` | Reserved extension format |
| `6` | `110` | Reserved | Illegal in v3.0 |
| `7` | `111` | Reserved | Illegal in v3.0 |

`EXT` is reserved for future ISA extensions. Every `EXT` encoding is illegal in v3.0.

---

## 8. Modifier field

### 8.1 MEM format

Only bit 0 is defined:

```text
modifier[0] = I
modifier[7:1] = 0
```

| `I` | Addressing |
|---:|---|
| 0 | Direct |
| 1 | Indirect |

Direct:

```text
EA = operand16
```

Indirect:

```text
EA = M[operand16][15:0]
```

Only the low 16 bits of the referenced word form the effective address.

### 8.2 Other formats

For `IMM`, `SPREL`, `BRANCH`, and `SYS`:

```text
modifier = 0x00
```

Any non-zero reserved modifier bit makes the instruction illegal.

---

## 9. Common ALU/data opcode namespace

`MEM`, `IMM`, and `SPREL` share the same operation numbers.

| Opcode | Operation | MEM | IMM | SPREL mnemonic |
|---:|---|:---:|:---:|---|
| `0x00` | ADD | `ADD` | `ADD` | `ADDSP` |
| `0x01` | SUB | `SUB` | `SUB` | `SUBSP` |
| `0x02` | AND | `AND` | `AND` | `ANDSP` |
| `0x03` | OR | `OR` | `OR` | `ORSP` |
| `0x04` | XOR | `XOR` | `XOR` | `XORSP` |
| `0x05` | LDA | `LDA` | `LDA` | `LDSP` |
| `0x06` | STA | `STA` | - | `STSP` |
| `0x07` | CMP | `CMP` | `CMP` | `CMPSP` |
| `0x08` | ISZ | `ISZ` | - | - |
| `0x09` | LDHI | - | `LDHI` | - |
| `0x0A` | LDLO | - | `LDLO` | - |
| `0x0B` | ADJSP | - | `ADJSP` | - |
| `0x0C..0x1F` | Reserved | - | - | - |

A format/opcode combination not listed above is illegal.

---

## 10. MEM format

```text
31      29 28      24 23              16 15               0
+----------+----------+------------------+------------------+
| 000 MEM  | opcode   | modifier         | address16        |
+----------+----------+------------------+------------------+
```

### 10.1 Arithmetic and logical operations

For `ADD`, `SUB`, `AND`, `OR`, `XOR`, and `CMP`:

```text
operand = M[EA]
```

`LDA`:

```text
AC <- M[EA]
```

`STA`:

```text
M[EA] <- AC
```

### 10.2 ISZ

```text
M[EA] <- M[EA] + 1 (mod 2^32)

if M[EA] == 0:
    PC <- PC + 1 (mod 2^16)
```

`ISZ` does not modify `NZCV`.

---

## 11. IMM format

```text
31      29 28      24 23              16 15               0
+----------+----------+------------------+------------------+
| 001 IMM  | opcode   | 0x00             | immediate16      |
+----------+----------+------------------+------------------+
```

### 11.1 Arithmetic immediate extension

For:

- `ADD`
- `SUB`
- `CMP`
- `LDA`

the 16-bit immediate is sign-extended to 32 bits:

```text
operand = sign_extend_16_to_32(immediate16)
```

### 11.2 Logical immediate extension

For:

- `AND`
- `OR`
- `XOR`

the 16-bit immediate is zero-extended:

```text
operand = zero_extend_16_to_32(immediate16)
```

---

## 12. LDHI and LDLO

### 12.1 LDHI

```text
AC[31:16] <- immediate16
AC[15:0]  <- unchanged
```

Flags:

```text
N <- AC[31] after update
Z <- 1 iff updated AC == 0
C <- unchanged
V <- unchanged
```

### 12.2 LDLO

```text
AC[15:0]  <- immediate16
AC[31:16] <- unchanged
```

Flags are updated identically to `LDHI`.

Example:

```asm
LDHI 0x1234
LDLO 0x5678
```

produces:

```text
AC = 0x12345678
```

---

## 13. SPREL format

```text
31      29 28      24 23              16 15               0
+----------+----------+------------------+------------------+
|010 SPREL | opcode   | 0x00             | signed offset16  |
+----------+----------+------------------+------------------+
```

Effective address:

```text
EA = (SP + sign_extend(offset16)) mod 2^16
```

Mnemonics:

```text
LDSP
STSP
ADDSP
SUBSP
ANDSP
ORSP
XORSP
CMPSP
```

Semantics correspond to the common operation namespace:

```text
LDSP d   == LDA M[SP+d]
STSP d   == STA M[SP+d]
ADDSP d  == ADD M[SP+d]
...
```

---

## 14. ADJSP

`ADJSP` uses `IMM` format and opcode `0x0B`.

```text
SP <- (SP + sign_extend(immediate16)) mod 2^16
```

`ADJSP` does not modify `NZCV`.

---

## 15. Arithmetic and flag semantics

Let `A` be the old `AC`, `B` the operand, and `R` the 32-bit result.

### 15.1 ADD

```text
R = A + B
AC <- R

N <- R[31]
Z <- (R == 0)
C <- unsigned carry-out
V <- signed addition overflow
```

### 15.2 SUB

```text
R = A - B
AC <- R

N <- R[31]
Z <- (R == 0)
C <- 1 iff unsigned A >= B
V <- signed subtraction overflow
```

### 15.3 CMP

```text
R = A - B
AC <- unchanged

N <- R[31]
Z <- (R == 0)
C <- 1 iff unsigned A >= B
V <- signed subtraction overflow
```

### 15.4 AND / OR / XOR

```text
AC <- result

N <- AC[31]
Z <- (AC == 0)
C <- unchanged
V <- unchanged
```

### 15.5 LDA / LDSP

```text
AC <- operand

N <- AC[31]
Z <- (AC == 0)
C <- unchanged
V <- unchanged
```

### 15.6 STA / STSP

All flags unchanged.

---

## 16. BRANCH format

```text
31      29 28      24 23              16 15               0
+----------+----------+------------------+------------------+
|011 BRANCH| opcode   | 0x00             | target16         |
+----------+----------+------------------+------------------+
```

All v3.0 branch and call targets are **direct absolute 16-bit addresses**.

Indirect branch and indirect call are not defined in v3.0.

### 16.1 Branch opcode assignments

| Opcode | Mnemonic | Condition |
|---:|---|---|
| `0x00` | `JMP` | Always |
| `0x01` | `CALL` | Always; push return PC |
| `0x02` | `BEQ` | `Z == 1` |
| `0x03` | `BNE` | `Z == 0` |
| `0x04` | `BLT` | `N != V` |
| `0x05` | `BGE` | `N == V` |
| `0x06` | `BGT` | `Z == 0 && N == V` |
| `0x07` | `BLE` | `Z == 1 || N != V` |
| `0x08` | `BULT` | `C == 0` |
| `0x09` | `BUGE` | `C == 1` |
| `0x0A` | `BUGT` | `C == 1 && Z == 0` |
| `0x0B` | `BULE` | `C == 0 || Z == 1` |
| `0x0C..0x1F` | Reserved | Illegal |

Branches do not modify `NZCV`.

---

## 17. Stack architecture

The stack grows toward lower addresses.

Push primitive:

```text
SP <- SP - 1
M[SP] <- value
```

Pop primitive:

```text
value <- M[SP]
SP <- SP + 1
```

All SP arithmetic wraps modulo `2^16`.

No architectural stack overflow/underflow exception exists in v3.0. Emulators and debuggers may provide diagnostic warnings.

---

## 18. CALL and RET

### 18.1 CALL

Before execution, normal instruction fetch has already advanced `PC` to the following instruction.

```text
SP <- SP - 1
M[SP] <- zero_extend_16_to_32(PC)
PC <- target16
```

`CALL` does not modify `NZCV`.

### 18.2 RET

`RET` is a `SYS` instruction:

```text
PC <- M[SP][15:0]
SP <- SP + 1
```

The upper 16 bits of the stored return-address word are ignored.

`RET` does not modify `NZCV`.

---

## 19. SYS format

```text
31      29 28      24 23                                   0
+----------+----------+--------------------------------------+
| 100 SYS  | opcode   |             all zero                |
+----------+----------+--------------------------------------+
```

For all SYS instructions, `modifier` and `operand16` must be zero.

### 19.1 SYS opcode assignments

| Opcode | Mnemonic | Operation |
|---:|---|---|
| `0x00` | `CLA` | Clear accumulator |
| `0x01` | `CMA` | Complement accumulator |
| `0x02` | `RET` | Return from subroutine |
| `0x03` | `IRET` | Return from interrupt |
| `0x04` | `HLT` | Halt CPU |
| `0x05` | `INP` | Input byte |
| `0x06` | `OUT` | Output byte |
| `0x07` | `SKI` | Skip if selected input ready |
| `0x08` | `SKO` | Skip if selected output ready |
| `0x09` | `ION` | Enable interrupts |
| `0x0A` | `IOF` | Disable interrupts |
| `0x0B` | `SIO` | Select serial I/O |
| `0x0C` | `PIO` | Select parallel I/O |
| `0x0D` | `IMK` | Load interrupt mask |
| `0x0E..0x1F` | Reserved | Illegal |

No `INC`, `CLE`, `CME`, rotate, or legacy sign/zero skip instruction exists in v3.0.

---

## 20. CLA and CMA

### CLA

```text
AC <- 0

N <- 0
Z <- 1
C <- unchanged
V <- unchanged
```

### CMA

```text
AC <- bitwise NOT AC

N <- AC[31]
Z <- (AC == 0)
C <- unchanged
V <- unchanged
```

---

## 21. I/O model

EX3 v3.0 retains the existing EX3 serial/parallel I/O model.

Logical ports:

| IMSK bit | Port | Meaning |
|---:|---|---|
| 3 | `SIN` | Serial Input |
| 2 | `SOU` | Serial Output |
| 1 | `PIN` | Parallel Input |
| 0 | `POU` | Parallel Output |

`PIO` selects parallel I/O.  
`SIO` selects serial I/O.

### 21.1 INP

Reads one byte from the selected input port into the low 8 bits of `AC`.

The existing EX3 behavior for the remaining AC bits is retained by implementations.

**INP does not modify N, Z, C, or V.**

### 21.2 OUT

Writes `AC[7:0]` to the selected output port.

Flags unchanged.

### 21.3 SKI / SKO

If the selected input/output port is ready:

```text
PC <- PC + 1 (mod 2^16)
```

Otherwise execution continues normally.

Flags unchanged.

### 21.4 ION / IOF

```text
ION: IEN <- 1
IOF: IEN <- 0
```

Other PSR bits unchanged.

### 21.5 IMK

```text
IMSK <- AC[3:0]
```

PSR unchanged.

---

## 22. Interrupt model

### 22.1 Interrupt acceptance

A maskable interrupt is accepted only when:

1. `IEN == 1`,
2. at least one enabled interrupt source is pending, and
3. the current instruction has completed.

Interrupts are therefore architecturally taken at **instruction boundaries**.

If multiple enabled sources are simultaneously pending, v3.0 uses this priority order:

```text
SIN > SOU > PIN > POU
```

v3.0 still transfers all accepted maskable interrupts to vector 0. The priority rule determines which current source is considered selected/pending first; software may inspect device state as needed.

### 22.2 Interrupt entry

The saved PSR is the pre-interrupt PSR, including its original `IEN` value.

```text
SP <- SP - 1
M[SP] <- PSR

SP <- SP - 1
M[SP] <- zero_extend_16_to_32(PC)

IEN <- 0
PC <- 0x0000
```

Stack after entry:

```text
higher address

+------------------+
| saved PSR        |
+------------------+
| saved PC         | <- SP
+------------------+

lower address
```

### 22.3 IRET

```text
PC <- M[SP][15:0]
SP <- SP + 1

PSR <- sanitize(M[SP])
SP <- SP + 1
```

`sanitize()` restores bits `IEN,N,Z,C,V` and forces all reserved PSR bits to zero.

PSR is restored after PC so that `IEN` is not re-enabled before the architectural interrupt-return sequence has completed.

---

## 23. HLT

`HLT` transitions the CPU into the halted state.

While halted:

- no further instruction is fetched,
- maskable interrupts do not resume execution,
- execution resumes only after reset.

A future wait-for-interrupt instruction may be added by a later ISA revision; `HLT` is not such an instruction.

---

## 24. Illegal instructions

The following are illegal in v3.0:

- formats `110` or `111`,
- every `EXT` encoding,
- reserved opcodes,
- unsupported format/opcode combinations,
- non-zero reserved modifier bits,
- non-zero reserved SYS payload bits.

Architectural behavior:

> An illegal instruction causes the CPU to enter the halted state.

Development tools that can report diagnostics, including assemblers, emulators, debuggers, and RTL simulation environments, should report an **illegal instruction error** in addition to modeling the architectural stop.

---

## 25. Removed instructions and formats

EX3 v3.0 does not contain the following legacy features:

- N2/N2I memory-memory instruction forms,
- 11/11I memory+immediate instruction forms,
- `MOVE`,
- `BSA`,
- `BUN` (renamed/replaced by `JMP`),
- `JPA`,
- `JZA`,
- `JNA`,
- `JZE`,
- `SPA`,
- `SZA`,
- `SNA`,
- `SZE`,
- `INC`,
- `CLE`,
- `CME`,
- `CIR`,
- `CIL`,
- any architectural `E` flag.

Legacy source using these operations must be rewritten for v3.

---

## 26. Pseudo instructions

The following are assembler pseudo instructions, not machine instructions.

### PUSH

```asm
PUSH
```

expands to:

```asm
ADJSP -1
STSP 0
```

### POP

```asm
POP
```

expands to:

```asm
LDSP 0
ADJSP 1
```

Because `POP` expands through `LDSP`, it updates `N/Z` according to the normal `LDA`/`LDSP` rule while preserving `C/V`.

---

## 27. Reset

On reset:

```text
AC   = 0x00000000
PC   = 0x0010
SP   = 0x0000
PSR  = 0x00000000
IMSK = 0x0

I/O selection = Parallel
CPU state     = Running
```

Peripheral ready/data state may be implementation-defined unless otherwise specified by a concrete platform.

---

## 28. Compatibility

EX3 v3.0 has **no binary compatibility** with EX3 v1/v2.

Toolchains must identify v3 explicitly and must not silently interpret earlier machine code as v3 machine code.

Source compatibility is not guaranteed for removed formats or removed legacy instructions.

---

## 29. Summary

EX3 v3.0 is a 32-bit fixed-width, 32-bit accumulator architecture with:

- 16-bit word addressing,
- 16-bit PC and SP,
- direct and indirect memory operands,
- 16-bit immediates,
- SP-relative operands,
- NZCV condition flags,
- stack-based CALL/RET,
- stack-based interrupt state preservation,
- signed and unsigned conditional branching,
- a unified 64K-word address space.

The core execution model is intentionally accumulator-centric:

```text
AC <- AC op operand
```

with operand source selected by the instruction format.

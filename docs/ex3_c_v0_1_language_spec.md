# EX3 C v0.1 Language Specification

**Status:** Frozen draft for initial compiler implementation  
**Language version:** EX3 C v0.1  
**Target ISA:** EX3 v3.0  
**Execution environment:** Freestanding

---

## 1. Overview

EX3 C v0.1 is a pointerless, 32-bit-integer-only freestanding C subset targeting EX3 v3.0.

It supports:

- `int` / `int32_t`
- `unsigned int` / `uint32_t`
- `void` for function returns and `(void)` parameter lists
- global scalar variables
- stack-local scalar variables
- fixed-arity direct functions
- recursion
- arithmetic, bitwise, logical, and comparison expressions
- `if` / `else`
- `switch`
- `while`
- `break`
- `continue`
- `goto`
- `return`
- built-in serial `putchar(int)` / `getchar(void)`

It does not support:

- pointers
- address-of and dereference
- arrays
- structures/unions
- function pointers
- `for`
- `do-while`
- variadic functions
- floating-point types

---

## 2. Required entry point

Every complete program must define:

```c
int main(void)
```

Startup code is equivalent to:

```asm
ORG 0x0010
CALL main
HLT
```

The value returned from `main` remains in `AC`.

---

## 3. Types

| Source spelling | Canonical type | Width | Semantics |
|---|---|---:|---|
| `int` | `int32_t` | 32 | signed two's-complement |
| `int32_t` | `int32_t` | 32 | signed two's-complement |
| `unsigned int` | `uint32_t` | 32 | unsigned |
| `uint32_t` | `uint32_t` | 32 | unsigned |

Aliases:

```text
int          == int32_t
unsigned int == uint32_t
```

`void` is supported only as:

- function return type
- `(void)` parameter list

Unsupported types include:

```text
char
short
long
long long
_Bool
float
double
enum
struct
union
pointer types
array types
function pointer types
```

Unsigned arithmetic wraps modulo `2^32`.

Signed overflow is undefined behavior.

---

## 4. Integer literals

Required forms:

- decimal
- hexadecimal
- optional `u` / `U` unsigned suffix

Examples:

```c
10
0xff
123u
0xffffffffU
```

An unsuffixed literal fitting in `int32_t` has type `int32_t`. Otherwise, if representable, it has type `uint32_t`.

---

## 5. Variables

Global scalar variables are supported:

```c
int counter;
uint32_t flags;
int limit = 100;
```

Uninitialized globals are initialized to zero.

Initializers must be compile-time integer constant expressions.

Local scalar variables are stored in the function stack frame.

---

## 6. Assignment

Simple assignment is supported:

```c
x = expression;
```

Not supported:

```text
+= -= *= /= %= &= |= ^=
++ --
```

Equivalent explicit forms must be used:

```c
x = x + 1;
```

---

## 7. Expression evaluation model

The canonical backend evaluates scalar expression results into EX3 register `AC`.

Intermediate results may be spilled into compiler-generated SP-relative stack slots.

---

## 8. Unary operators

Supported:

```text
~ ! + -
```

### `~x`

32-bit bitwise complement.

Typical implementation:

```text
evaluate x -> AC
CMA
```

### `!x`

Logical negation:

```text
1 if x == 0
0 otherwise
```

Result type is `int32_t`.

### `+x`

Returns `x` unchanged.

### `-x`

Arithmetic negation.

For unsigned values:

```text
0 - x mod 2^32
```

Negating signed `INT32_MIN` is undefined behavior.

---

## 9. Binary arithmetic operators

Supported:

```text
+ - * / %
```

| Operator | Signed | Unsigned | Implementation |
|---|:---:|:---:|---|
| `+` | Yes | Yes | EX3 `ADD` |
| `-` | Yes | Yes | EX3 `SUB` |
| `*` | Yes | Yes | Software runtime |
| `/` | Yes | Yes | Software runtime |
| `%` | Yes | Yes | Software runtime |

Required compiler runtime routines:

```text
__ex3_mul_i32
__ex3_mul_u32
__ex3_div_i32
__ex3_div_u32
__ex3_mod_i32
__ex3_mod_u32
```

Signed division truncates toward zero.

```text
 7 / 3 ==  2
-7 / 3 == -2
-7 % 3 == -1
```

Division by zero is undefined behavior.

---

## 10. Bitwise operators

Supported:

```text
& | ^
```

They operate on all 32 bits and lower to:

```text
& -> AND
| -> OR
^ -> XOR
```

---

## 11. Comparison operators

Supported:

```text
< <= > >= == !=
```

Comparison result type is always `int32_t`:

```text
false = 0
true  = 1
```

### Signed

| C | EX3 |
|---|---|
| `<` | `BLT` |
| `<=` | `BLE` |
| `>` | `BGT` |
| `>=` | `BGE` |
| `==` | `BEQ` |
| `!=` | `BNE` |

### Unsigned

| C | EX3 |
|---|---|
| `<` | `BULT` |
| `<=` | `BULE` |
| `>` | `BUGT` |
| `>=` | `BUGE` |
| `==` | `BEQ` |
| `!=` | `BNE` |

---

## 12. Logical operators

Supported:

```text
! && ||
```

Truth conversion:

```text
0     = false
non-0 = true
```

Logical results are normalized to `0` or `1`.

`&&` and `||` use standard C short-circuit evaluation.

---

## 13. Signed/unsigned conversion

When `int32_t` and `uint32_t` participate in the same binary arithmetic, bitwise, or comparison expression, the `int32_t` operand is converted to `uint32_t`.

Therefore:

```c
int a;
unsigned int b;

a < b
```

uses unsigned comparison semantics.

---

## 14. Operator precedence

Standard C precedence is used for the supported operators:

```text
~ ! unary+ unary-
* / %
+ -
< <= > >=
== !=
&
^
|
&&
||
=
```

Parenthesized expressions are supported.

---

## 15. `if` / `else`

Supported:

```c
if (condition) {
    ...
}

if (condition) {
    ...
} else {
    ...
}
```

Zero is false and non-zero is true.

---

## 16. `while`

Supported:

```c
while (condition) {
    ...
}
```

The condition is tested before each iteration.

Not supported:

```text
for
do-while
```

---

## 17. `break`

Supported inside:

- `while`
- `switch`

It exits the innermost applicable construct.

---

## 18. `continue`

Supported inside `while`.

It transfers control to the condition evaluation of the innermost enclosing `while`.

A `continue` inside a `switch` nested in a `while` applies to that surrounding `while`.

---

## 19. `switch`

Supported for `int32_t` and `uint32_t`.

```c
switch (x) {
case 0:
    ...
    break;

case 1:
    ...
    break;

default:
    ...
}
```

Rules:

- `case` values must be compile-time integer constants
- duplicate cases are errors
- at most one `default`
- fall-through is supported

Lowering uses comparison chains or comparison trees.

Jump tables are not required.

---

## 20. `goto`

Direct intra-function `goto` is supported:

```c
goto retry;

retry:
    ...
```

Not supported:

- cross-function goto
- computed goto
- label addresses

Lowering uses direct `JMP`.

---

## 21. Functions

Allowed return types:

```text
void
int / int32_t
unsigned int / uint32_t
```

Functions support fixed-length parameter lists, including zero arguments.

Examples:

```c
int foo(void);
int add(int a, int b);
uint32_t mask(uint32_t a, uint32_t b);
void reset(int value);
```

Every argument has type `int32_t` or `uint32_t`.

Direct calls and recursion are supported.

Not supported:

- variadic functions
- function pointers
- indirect calls

---

## 22. Function ABI

EX3 C v0.1 uses the EX3 v3 ABI.

Arguments are pushed right-to-left.

For:

```c
foo(a, b, c)
```

the stack at callee entry is:

```text
SP+0 = return address
SP+1 = a
SP+2 = b
SP+3 = c
```

Caller sequence is conceptually:

```text
push c
push b
push a
CALL foo
ADJSP 3
```

Return values are placed in `AC`.

Argument cleanup is performed by the caller.

---

## 23. Local variables and temporaries

Local variables and compiler-generated temporaries are placed in the function stack frame.

Canonical prologue:

```asm
ADJSP -N
```

Canonical epilogue:

```asm
ADJSP N
RET
```

Available SP-relative operations include:

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

---

## 24. `return`

For integer functions:

```c
return expression;
```

The expression is evaluated into `AC`.

For `void` functions:

```c
return;
```

Falling off the end of a `void` function is equivalent to `return;`.

Reaching the end of a non-void function without returning a value is undefined behavior and should produce a diagnostic.

---

## 25. Built-in serial I/O

The runtime provides:

```c
void putchar(int c);
int getchar(void);
```

These identifiers are reserved.

### `putchar`

Sends the low 8 bits of `c` through serial output.

Upper 24 bits are ignored.

Conceptual implementation:

```asm
__ex3_putchar:
    SIO

.wait:
    SKO
    JMP .wait

    LDSP 1
    OUT
    RET
```

### `getchar`

Waits for serial input and returns a zero-extended value in:

```text
0 .. 255
```

Conceptual implementation:

```asm
__ex3_getchar:
    SIO

.wait:
    SKI
    JMP .wait

    INP
    ; normalize upper bits to zero
    RET
```

EOF semantics are not defined in v0.1.

---

## 26. Calls inside expressions

Function calls may appear inside expressions:

```c
x = foo(a) + bar(b);
```

Intermediate expression results are preserved using stack-temporary slots as required.

Canonical backend model:

```text
evaluate subexpression -> AC
spill if necessary
evaluate next subexpression -> AC
combine using EX3 ALU/SP-relative operation
```

---

## 27. Evaluation order

Except where required by `&&`, `||`, or control-flow semantics, EX3 C v0.1 adds no evaluation-order guarantees beyond standard C.

The compiler may choose an order that minimizes accumulator spills.

---

## 28. Explicitly unsupported features

```text
pointers
address-of &
dereference *
arrays
struct
union
function pointers
indirect calls
pointer/address arithmetic
for
do-while
variadic functions
sizeof
typedef
enum
floating point
atomic operations
dynamic allocation
string literals
character arrays
compound literals
designated initializers
runtime global initializers
```

`*` remains valid as multiplication.

---

## 29. Example program

```c
int counter;

int factorial(int n)
{
    int result;

    result = 1;

    while (n > 1) {
        result = result * n;
        n = n - 1;
    }

    return result;
}

int main(void)
{
    int value;

    value = factorial(5);

    if (value != 120) {
        putchar(69);  /* E */
        putchar(82);  /* R */
    } else {
        putchar(79);  /* O */
        putchar(75);  /* K */
    }

    return value;
}
```

---

## 30. Suggested compiler pipeline

```text
source
  -> lexer
  -> parser
  -> AST
  -> name resolution
  -> type checking
  -> typed IR
  -> control-flow lowering
  -> EX3 accumulator-oriented IR
  -> stack-frame / temporary allocation
  -> EX3 v3 assembly
  -> assembler
```

---

## 31. Conformance requirements

A conforming EX3 C v0.1 compiler must:

1. accept every construct defined as supported
2. diagnose constructs explicitly excluded from v0.1
3. generate code conforming to the EX3 v3 ISA and ABI
4. preserve `&&` / `||` short-circuit semantics
5. distinguish signed and unsigned comparisons
6. use software runtime routines for unavailable hardware arithmetic
7. require `int main(void)` for complete programs
8. provide functioning `putchar(int)` and `getchar(void)` built-ins

---

## 32. Quick reference

### Types

```text
int / int32_t
unsigned int / uint32_t
void
```

### Unary operators

```text
~ ! + -
```

### Binary operators

```text
+ - * / %
& | ^
&& ||
< <= > >= == !=
=
```

### Control flow

```text
if / else
switch
while
break
continue
goto
return
```

### Entry point

```c
int main(void)
```

### Built-ins

```c
void putchar(int);
int getchar(void);
```

---

EX3 C v0.1 is a **pointerless, 32-bit-integer-only, freestanding C subset** designed specifically for the EX3 v3 accumulator architecture.
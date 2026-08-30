# EX3 v3 assembler / emulator (Rust)

EX3 v3.0 ISA のアセンブラ、エミュレータ、デバッガです。v1/v2とのバイナリ・ソース互換性はありません。

実装範囲:

- 32-bit固定長命令（`format[31:29] + opcode[28:24] + modifier[23:16] + operand[15:0]`）
- 64Kワードの統合メモリ、16-bit `PC` / `SP`
- MEM（直接・間接）、IMM、SPREL、BRANCH、SYSの全v3形式
- `NZCV` / `IEN`を含む`PSR`
- スタック式`CALL` / `RET`、`PUSH` / `POP`擬似命令
- ハードウェア割り込みフレームと`IRET`
- シリアル・パラレルI/O

```sh
cargo build --release
cargo test

# Assemble (`program.mem` and `program.prb` are produced)
cargo run -- assemble program.asm

# Validate, execute, debug, or disassemble
cargo run -- check program.asm
cargo run -- run program.asm --max-steps 100000 --trace
cargo run -- debug program.mem
cargo run -- disasm --word 0x2500ffff
```

## Assembly syntax

ラベルは`LABEL:`または`LABEL,`、コメントは`;`または`/`を使用できます。`END`は任意です。

```asm
ORG 0x0000
JMP IRQ_HANDLER

ORG 0x0010
START:
    LDA -1          ; immediate (sign-extended)
    LDA VALUE       ; direct memory
    LDA POINTER I   ; indirect memory
    LDA @0x2000     ; numeric direct memory address
    LDSP 2          ; M[SP+2]
    PUSH            ; ADJSP -1 / STSP 0
    POP             ; LDSP 0 / ADJSP 1
    HLT

VALUE:   HEX 12345678
POINTER: SYM VALUE
END
```

`ADD`、`SUB`、`AND`、`OR`、`XOR`、`LDA`、`CMP`では、数値オペランドは即値、シンボルはメモリ参照です。数値のメモリアドレスを明示する場合は`@`を付けます。間接参照は末尾の大文字`I`で指定します。

`.mem`は16-bitアドレスを使う`@aaaa wwwwwwww`形式です。CPUはリセット時に`PC=0x0010`、`SP=0x0000`から開始します。

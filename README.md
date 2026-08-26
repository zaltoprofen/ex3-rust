# EX3 assembler / emulator (Rust)

EX3 32-bit ISA のリファレンス・アセンブラ、エミュレータ、デバッガです。仕様書に基づき、命令の encode/decode、2パスシンボル解決、`.mem` / `.prb`、I/O、割り込みを実装しています。

```sh
cargo build --release
cargo test

# Assemble (`program.mem` and `program.prb` are produced)
cargo run -- assemble program.asm

# Validate, execute, or debug
cargo run -- check program.asm
cargo run -- run program.asm --max-steps 100000 --trace
cargo run -- debug program.mem
cargo run -- disasm --word 0xc8000001
```

`--compat strict`（既定）は仕様上明白な旧実装の問題を修正します。`--compat legacy` は JZA/JNA の入れ替わり、CIR の算術シフト、INP の符号拡張、旧 E フラグ判定を再現します。

アセンブリは `END` 必須です。PC は `0x010` から開始し、既定の実行上限は 10,000,000 命令です。


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
cargo run -- run program.asm --compat legacy --io legacy --seed 1234
cargo run -- debug program.mem
cargo run -- disasm --word 0xc8000001
```

## 互換モード

`--compat strict`（既定）はRust版のEX3リファレンス動作です。旧Scalaエミュレータで確認されている以下の問題を修正しています。

- JZA/JNAのdispatch
- CIRの論理右シフト
- 入力byteのゼロ拡張
- 32 bit加減算のcarry/borrow
- 間接アドレスの12 bit化

`--compat legacy` は、旧Scala実装のCPUおよび周辺機器について、主要な観測可能挙動を回帰比較用に再現します。旧パーサの不正入力処理やすべての境界的なソース構文について、完全互換を保証するものではありません。

## I/O backend

- `--io null`（既定）: 入力なし、出力破棄
- `--io legacy`: 旧Scala版のready、mask、IEN停止タイミングを再現

`--seed N` はlegacy I/Oの入力ready間隔を決定論的にします。同じプログラム、入力、seedでは同じタイミングになります。

アセンブリは `END` 必須です。PC は `0x010` から開始し、既定の実行上限は 10,000,000 命令です。

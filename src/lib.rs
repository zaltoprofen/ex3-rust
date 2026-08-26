//! EX3 32-bit assembler and emulator.

pub mod assembler;
pub mod debugger;
pub mod emulator;
pub mod isa;
pub mod output;

/// アセンブラとCPUが使用する互換性ポリシー。
///
/// [`Strict`](Self::Strict) はEX3の意図された仕様を実装する。
/// [`Legacy`](Self::Legacy) は旧Scala実装との比較に必要な既知の挙動を再現する。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompatibilityMode {
    /// 範囲検証と正しいcarry、rotate、分岐条件を使用する標準モード。
    #[default]
    Strict,
    /// JZA/JNAの交換など、旧Scala実装固有の挙動を再現するモード。
    Legacy,
}

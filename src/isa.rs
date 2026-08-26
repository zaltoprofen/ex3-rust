//! EX3 の型付き命令モデルと、32 bit 機械語のエンコード／デコード。
//!
//! アセンブラとエミュレータはこのモジュールの [`Instruction`] を共有する。
//! opcode を文字列で持たないことで、両者の命令表が食い違うのを防いでいる。

use std::{error::Error, fmt};

/// EX3 の1ワード。命令とデータはいずれも32 bitである。
pub type Word = u32;
/// 12 bitアドレス空間に含まれるワード数。
pub const MEMORY_SIZE: usize = 4096;

/// 範囲が保証された12 bitメモリアドレス。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Address(u16);

impl Address {
    /// 割り込み復帰先の保存に使われるアドレス。
    pub const ZERO: Self = Self(0);
    /// リセット後に最初の命令をフェッチするアドレス。
    pub const RESET: Self = Self(0x10);

    pub fn new(value: u16) -> Result<Self, ValueError> {
        (value <= 0x0fff)
            .then_some(Self(value))
            .ok_or(ValueError::AddressOutOfRange(value as u32))
    }

    /// ワードの下位12 bitをアドレスとして解釈する。
    ///
    /// strictモードの間接アドレッシングはこの規則を使用する。
    pub const fn from_low12(value: u32) -> Self {
        Self((value & 0x0fff) as u16)
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    /// 12 bit幅で加算する。`0xfff + 1` は `0x000` に戻る。
    pub const fn wrapping_add(self, rhs: u16) -> Self {
        Self(self.0.wrapping_add(rhs) & 0x0fff)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03x}", self.0)
    }
}

/// 命令中の12 bit即値を、生のビットパターンとして保持する型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Immediate12(u16);

impl Immediate12 {
    pub fn from_raw(raw: u16) -> Result<Self, ValueError> {
        (raw <= 0x0fff)
            .then_some(Self(raw))
            .ok_or(ValueError::ImmediateOutOfRange(raw as i64))
    }

    pub fn from_signed(value: i32) -> Result<Self, ValueError> {
        if (-2048..=2047).contains(&value) {
            Ok(Self((value as u32 & 0x0fff) as u16))
        } else {
            Err(ValueError::ImmediateOutOfRange(value as i64))
        }
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    /// 12 bitの2の補数を32 bit符号付き整数へ符号拡張する。
    pub const fn as_i32(self) -> i32 {
        if self.0 & 0x0800 != 0 {
            (self.0 as i32) | !0x0fff
        } else {
            self.0 as i32
        }
    }

    pub const fn as_word(self) -> Word {
        self.as_i32() as Word
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueError {
    AddressOutOfRange(u32),
    ImmediateOutOfRange(i64),
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressOutOfRange(v) => write!(f, "address out of 12-bit range: {v}"),
            Self::ImmediateOutOfRange(v) => write!(f, "immediate out of 12-bit range: {v}"),
        }
    }
}
impl Error for ValueError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum N1Op {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Lda,
    Sta,
    Bun,
    Bsa,
    Jpa,
    Jza,
    Jna,
    Jze,
    Isz,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum N2Op {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Move,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImmediateOp {
    Add,
    And,
    Or,
    Lda,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryImmediateOp {
    Add,
    And,
    Or,
    Sta,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoOperandOp {
    Cla,
    Cle,
    Cma,
    Cme,
    Cir,
    Cil,
    Inc,
    Spa,
    Sza,
    Sna,
    Sze,
    Inp,
    Out,
    Ski,
    Sko,
    Ion,
    Iof,
    Sio,
    Pio,
    Imk,
    Hlt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// EX3の5種類の命令形式を表す型付き命令。
pub enum Instruction {
    N1 {
        op: N1Op,
        operand: Address,
        indirect: bool,
    },
    N2 {
        op: N2Op,
        operand1: Address,
        operand2: Address,
        indirect: bool,
    },
    Immediate {
        op: ImmediateOp,
        value: Immediate12,
    },
    MemoryImmediate {
        op: MemoryImmediateOp,
        operand: Address,
        value: Immediate12,
        indirect: bool,
    },
    NoOperand(NoOperandOp),
}

impl Instruction {
    /// 命令を仕様書互換の32 bitワードへ変換する。
    pub fn encode(self) -> Word {
        match self {
            Self::N1 {
                op,
                operand,
                indirect,
            } => n1_base(op) | if indirect { 0x4000_0000 } else { 0 } | operand.get() as u32,
            Self::N2 {
                op,
                operand1,
                operand2,
                indirect,
            } => {
                n2_base(op)
                    | if indirect { 0x4000_0000 } else { 0 }
                    | ((operand1.get() as u32) << 12)
                    | operand2.get() as u32
            }
            Self::Immediate { op, value } => immediate_base(op) | value.raw() as u32,
            Self::MemoryImmediate {
                op,
                operand,
                value,
                indirect,
            } => {
                memory_immediate_base(op)
                    | if indirect { 0x4000_0000 } else { 0 }
                    | ((operand.get() as u32) << 12)
                    | value.raw() as u32
            }
            Self::NoOperand(op) => no_operand_word(op),
        }
    }
}

const fn n1_base(op: N1Op) -> u32 {
    match op {
        N1Op::Add => 0x1000,
        N1Op::Sub => 0x2000,
        N1Op::And => 0x4000,
        N1Op::Or => 0x8000,
        N1Op::Xor => 0x10000,
        N1Op::Lda => 0x20000,
        N1Op::Sta => 0x40000,
        N1Op::Bun => 0x80000,
        N1Op::Bsa => 0x100000,
        N1Op::Jpa => 0x200000,
        N1Op::Jza => 0x400000,
        N1Op::Jna => 0x800000,
        N1Op::Jze => 0x1000000,
        N1Op::Isz => 0x2000000,
    }
}
const fn n2_base(op: N2Op) -> u32 {
    match op {
        N2Op::Add => 0x21000000,
        N2Op::Sub => 0x22000000,
        N2Op::And => 0x23000000,
        N2Op::Or => 0x24000000,
        N2Op::Xor => 0x25000000,
        N2Op::Move => 0x26000000,
    }
}
const fn immediate_base(op: ImmediateOp) -> u32 {
    match op {
        ImmediateOp::Add => 0xc1000000,
        ImmediateOp::And => 0xc2000000,
        ImmediateOp::Or => 0xc4000000,
        ImmediateOp::Lda => 0xc8000000,
    }
}
const fn memory_immediate_base(op: MemoryImmediateOp) -> u32 {
    match op {
        MemoryImmediateOp::Add => 0xa1000000,
        MemoryImmediateOp::And => 0xa2000000,
        MemoryImmediateOp::Or => 0xa4000000,
        MemoryImmediateOp::Sta => 0xa8000000,
    }
}
const fn no_operand_word(op: NoOperandOp) -> u32 {
    match op {
        NoOperandOp::Cla => 0x80000001,
        NoOperandOp::Cle => 0x80000002,
        NoOperandOp::Cma => 0x80000004,
        NoOperandOp::Cme => 0x80000008,
        NoOperandOp::Cir => 0x80000010,
        NoOperandOp::Cil => 0x80000020,
        NoOperandOp::Inc => 0x80000040,
        NoOperandOp::Spa => 0x80000080,
        NoOperandOp::Sza => 0x80000100,
        NoOperandOp::Sna => 0x80000200,
        NoOperandOp::Sze => 0x80000400,
        NoOperandOp::Inp => 0x80000800,
        NoOperandOp::Out => 0x80001000,
        NoOperandOp::Ski => 0x80002000,
        NoOperandOp::Sko => 0x80004000,
        NoOperandOp::Ion => 0x80008000,
        NoOperandOp::Iof => 0x80010000,
        NoOperandOp::Sio => 0x80020000,
        NoOperandOp::Pio => 0x80040000,
        NoOperandOp::Imk => 0x80080000,
        NoOperandOp::Hlt => 0x80100000,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub word: Word,
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown EX3 opcode: 0x{:08x}", self.word)
    }
}
impl Error for DecodeError {}

pub fn decode(word: Word) -> Result<Instruction, DecodeError> {
    let unknown = || DecodeError { word };

    // 上位3 bitで命令形式を選ぶ。間接指定bitを含むため、directと
    // indirectは同じmatch armで処理する。
    match word >> 29 {
        0 | 2 => {
            // N1のbits 28..26は予約領域。opcodeだけをマスクすると
            // 不正なワードを合法命令として受理してしまうため先に検査する。
            if word & 0x1c00_0000 != 0 {
                return Err(unknown());
            }
            let indirect = word & 0x4000_0000 != 0;
            let key = word & 0x03ff_f000;
            let op = [
                N1Op::Add,
                N1Op::Sub,
                N1Op::And,
                N1Op::Or,
                N1Op::Xor,
                N1Op::Lda,
                N1Op::Sta,
                N1Op::Bun,
                N1Op::Bsa,
                N1Op::Jpa,
                N1Op::Jza,
                N1Op::Jna,
                N1Op::Jze,
                N1Op::Isz,
            ]
            .into_iter()
            .find(|op| n1_base(*op) == key)
            .ok_or_else(unknown)?;
            Ok(Instruction::N1 {
                op,
                operand: Address::from_low12(word),
                indirect,
            })
        }
        1 | 3 => {
            let indirect = word & 0x4000_0000 != 0;
            let key = word & 0x3f00_0000;
            let op = [
                N2Op::Add,
                N2Op::Sub,
                N2Op::And,
                N2Op::Or,
                N2Op::Xor,
                N2Op::Move,
            ]
            .into_iter()
            .find(|op| n2_base(*op) & 0x3f00_0000 == key)
            .ok_or_else(unknown)?;
            Ok(Instruction::N2 {
                op,
                operand1: Address::from_low12(word >> 12),
                operand2: Address::from_low12(word),
                indirect,
            })
        }
        4 => {
            let op = [
                NoOperandOp::Cla,
                NoOperandOp::Cle,
                NoOperandOp::Cma,
                NoOperandOp::Cme,
                NoOperandOp::Cir,
                NoOperandOp::Cil,
                NoOperandOp::Inc,
                NoOperandOp::Spa,
                NoOperandOp::Sza,
                NoOperandOp::Sna,
                NoOperandOp::Sze,
                NoOperandOp::Inp,
                NoOperandOp::Out,
                NoOperandOp::Ski,
                NoOperandOp::Sko,
                NoOperandOp::Ion,
                NoOperandOp::Iof,
                NoOperandOp::Sio,
                NoOperandOp::Pio,
                NoOperandOp::Imk,
                NoOperandOp::Hlt,
            ]
            .into_iter()
            .find(|op| no_operand_word(*op) == word)
            .ok_or_else(unknown)?;
            Ok(Instruction::NoOperand(op))
        }
        5 | 7 => {
            let indirect = word & 0x4000_0000 != 0;
            let key = word & 0x3f00_0000;
            let op = [
                MemoryImmediateOp::Add,
                MemoryImmediateOp::And,
                MemoryImmediateOp::Or,
                MemoryImmediateOp::Sta,
            ]
            .into_iter()
            .find(|op| memory_immediate_base(*op) & 0x3f00_0000 == key)
            .ok_or_else(unknown)?;
            Ok(Instruction::MemoryImmediate {
                op,
                operand: Address::from_low12(word >> 12),
                value: Immediate12::from_raw((word & 0xfff) as u16).expect("masked"),
                indirect,
            })
        }
        6 => {
            if word & 0x00ff_f000 != 0 {
                return Err(unknown());
            }
            let key = word & 0xff00_0000;
            let op = [
                ImmediateOp::Add,
                ImmediateOp::And,
                ImmediateOp::Or,
                ImmediateOp::Lda,
            ]
            .into_iter()
            .find(|op| immediate_base(*op) == key)
            .ok_or_else(unknown)?;
            Ok(Instruction::Immediate {
                op,
                value: Immediate12::from_raw((word & 0xfff) as u16).expect("masked"),
            })
        }
        _ => Err(unknown()),
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::N1 {
                op,
                operand,
                indirect,
            } => write!(
                f,
                "{} {:03x}{}",
                n1_name(op),
                operand.get(),
                if indirect { " I" } else { "" }
            ),
            Self::N2 {
                op,
                operand1,
                operand2,
                indirect,
            } => write!(
                f,
                "{} {:03x} {:03x}{}",
                n2_name(op),
                operand1.get(),
                operand2.get(),
                if indirect { " I" } else { "" }
            ),
            Self::Immediate { op, value } => write!(f, "{} {}", immediate_name(op), value.as_i32()),
            Self::MemoryImmediate {
                op,
                operand,
                value,
                indirect,
            } => write!(
                f,
                "{} {:03x} {}{}",
                memory_immediate_name(op),
                operand.get(),
                value.as_i32(),
                if indirect { " I" } else { "" }
            ),
            Self::NoOperand(op) => f.write_str(no_operand_name(op)),
        }
    }
}

pub const fn n1_name(op: N1Op) -> &'static str {
    match op {
        N1Op::Add => "ADD",
        N1Op::Sub => "SUB",
        N1Op::And => "AND",
        N1Op::Or => "OR",
        N1Op::Xor => "XOR",
        N1Op::Lda => "LDA",
        N1Op::Sta => "STA",
        N1Op::Bun => "BUN",
        N1Op::Bsa => "BSA",
        N1Op::Jpa => "JPA",
        N1Op::Jza => "JZA",
        N1Op::Jna => "JNA",
        N1Op::Jze => "JZE",
        N1Op::Isz => "ISZ",
    }
}
pub const fn n2_name(op: N2Op) -> &'static str {
    match op {
        N2Op::Add => "ADD",
        N2Op::Sub => "SUB",
        N2Op::And => "AND",
        N2Op::Or => "OR",
        N2Op::Xor => "XOR",
        N2Op::Move => "MOVE",
    }
}
pub const fn immediate_name(op: ImmediateOp) -> &'static str {
    match op {
        ImmediateOp::Add => "ADD",
        ImmediateOp::And => "AND",
        ImmediateOp::Or => "OR",
        ImmediateOp::Lda => "LDA",
    }
}
pub const fn memory_immediate_name(op: MemoryImmediateOp) -> &'static str {
    match op {
        MemoryImmediateOp::Add => "ADD",
        MemoryImmediateOp::And => "AND",
        MemoryImmediateOp::Or => "OR",
        MemoryImmediateOp::Sta => "STA",
    }
}
pub const fn no_operand_name(op: NoOperandOp) -> &'static str {
    match op {
        NoOperandOp::Cla => "CLA",
        NoOperandOp::Cle => "CLE",
        NoOperandOp::Cma => "CMA",
        NoOperandOp::Cme => "CME",
        NoOperandOp::Cir => "CIR",
        NoOperandOp::Cil => "CIL",
        NoOperandOp::Inc => "INC",
        NoOperandOp::Spa => "SPA",
        NoOperandOp::Sza => "SZA",
        NoOperandOp::Sna => "SNA",
        NoOperandOp::Sze => "SZE",
        NoOperandOp::Inp => "INP",
        NoOperandOp::Out => "OUT",
        NoOperandOp::Ski => "SKI",
        NoOperandOp::Sko => "SKO",
        NoOperandOp::Ion => "ION",
        NoOperandOp::Iof => "IOF",
        NoOperandOp::Sio => "SIO",
        NoOperandOp::Pio => "PIO",
        NoOperandOp::Imk => "IMK",
        NoOperandOp::Hlt => "HLT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn examples_encode() {
        let a = Address::new(0x123).unwrap();
        assert_eq!(
            Instruction::N1 {
                op: N1Op::Add,
                operand: a,
                indirect: false
            }
            .encode(),
            0x00001123
        );
        assert_eq!(
            Instruction::N1 {
                op: N1Op::Add,
                operand: a,
                indirect: true
            }
            .encode(),
            0x40001123
        );
        assert_eq!(
            Instruction::Immediate {
                op: ImmediateOp::Lda,
                value: Immediate12::from_signed(1).unwrap()
            }
            .encode(),
            0xc8000001
        );
    }
    #[test]
    fn all_round_trip() {
        let a = Address::new(0xabc).unwrap();
        let b = Address::new(0x123).unwrap();
        let i = Immediate12::from_signed(-1).unwrap();
        for op in [
            N1Op::Add,
            N1Op::Sub,
            N1Op::And,
            N1Op::Or,
            N1Op::Xor,
            N1Op::Lda,
            N1Op::Sta,
            N1Op::Bun,
            N1Op::Bsa,
            N1Op::Jpa,
            N1Op::Jza,
            N1Op::Jna,
            N1Op::Jze,
            N1Op::Isz,
        ] {
            for indirect in [false, true] {
                let x = Instruction::N1 {
                    op,
                    operand: a,
                    indirect,
                };
                assert_eq!(decode(x.encode()), Ok(x));
            }
        }
        for op in [
            N2Op::Add,
            N2Op::Sub,
            N2Op::And,
            N2Op::Or,
            N2Op::Xor,
            N2Op::Move,
        ] {
            for indirect in [false, true] {
                let x = Instruction::N2 {
                    op,
                    operand1: a,
                    operand2: b,
                    indirect,
                };
                assert_eq!(decode(x.encode()), Ok(x));
            }
        }
        for op in [
            ImmediateOp::Add,
            ImmediateOp::And,
            ImmediateOp::Or,
            ImmediateOp::Lda,
        ] {
            let x = Instruction::Immediate { op, value: i };
            assert_eq!(decode(x.encode()), Ok(x));
        }
        for op in [
            MemoryImmediateOp::Add,
            MemoryImmediateOp::And,
            MemoryImmediateOp::Or,
            MemoryImmediateOp::Sta,
        ] {
            for indirect in [false, true] {
                let x = Instruction::MemoryImmediate {
                    op,
                    operand: a,
                    value: i,
                    indirect,
                };
                assert_eq!(decode(x.encode()), Ok(x));
            }
        }
        for op in [
            NoOperandOp::Cla,
            NoOperandOp::Cle,
            NoOperandOp::Cma,
            NoOperandOp::Cme,
            NoOperandOp::Cir,
            NoOperandOp::Cil,
            NoOperandOp::Inc,
            NoOperandOp::Spa,
            NoOperandOp::Sza,
            NoOperandOp::Sna,
            NoOperandOp::Sze,
            NoOperandOp::Inp,
            NoOperandOp::Out,
            NoOperandOp::Ski,
            NoOperandOp::Sko,
            NoOperandOp::Ion,
            NoOperandOp::Iof,
            NoOperandOp::Sio,
            NoOperandOp::Pio,
            NoOperandOp::Imk,
            NoOperandOp::Hlt,
        ] {
            let x = Instruction::NoOperand(op);
            assert_eq!(decode(x.encode()), Ok(x));
        }
    }
    #[test]
    fn immediate_sign_extension() {
        assert_eq!(Immediate12::from_raw(0x800).unwrap().as_i32(), -2048);
        assert_eq!(Immediate12::from_raw(0xfff).unwrap().as_i32(), -1);
    }
    #[test]
    fn rejects_reserved_bits() {
        assert!(decode(0xc1001000).is_err());
        assert!(decode(0x10001000).is_err());
        assert!(decode(0).is_err());
    }
}

//! EX3 の型付き命令モデルと、32 bit 機械語のエンコード／デコード。
//!
//! アセンブラとエミュレータはこのモジュールの [`Instruction`] を共有する。
//! opcode を文字列で持たないことで、両者の命令表が食い違うのを防いでいる。

use std::{error::Error, fmt, str::FromStr};

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

/// Legal operations for each instruction format. Encoding, decoding, parsing,
/// and exhaustive tests all refer to these lists.
pub const N1_OPS: &[N1Op] = &[
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
];
pub const N2_OPS: &[N2Op] = &[
    N2Op::Add,
    N2Op::Sub,
    N2Op::And,
    N2Op::Or,
    N2Op::Xor,
    N2Op::Move,
];
pub const IMMEDIATE_OPS: &[ImmediateOp] = &[
    ImmediateOp::Add,
    ImmediateOp::And,
    ImmediateOp::Or,
    ImmediateOp::Lda,
];
pub const MEMORY_IMMEDIATE_OPS: &[MemoryImmediateOp] = &[
    MemoryImmediateOp::Add,
    MemoryImmediateOp::And,
    MemoryImmediateOp::Or,
    MemoryImmediateOp::Sta,
];
pub const NO_OPERAND_OPS: &[NoOperandOp] = &[
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
];

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
            } => {
                encode_header(
                    if indirect {
                        InstructionFormat::N1Indirect
                    } else {
                        InstructionFormat::N1
                    },
                    n1_opcode(op),
                ) | operand.get() as u32
            }
            Self::N2 {
                op,
                operand1,
                operand2,
                indirect,
            } => {
                encode_header(
                    if indirect {
                        InstructionFormat::N2Indirect
                    } else {
                        InstructionFormat::N2
                    },
                    n2_opcode(op),
                ) | ((operand1.get() as u32) << 12)
                    | operand2.get() as u32
            }
            Self::Immediate { op, value } => {
                encode_header(InstructionFormat::Immediate, immediate_opcode(op))
                    | value.raw() as u32
            }
            Self::MemoryImmediate {
                op,
                operand,
                value,
                indirect,
            } => {
                encode_header(
                    if indirect {
                        InstructionFormat::MemoryImmediateIndirect
                    } else {
                        InstructionFormat::MemoryImmediate
                    },
                    memory_immediate_opcode(op),
                ) | ((operand.get() as u32) << 12)
                    | value.raw() as u32
            }
            Self::NoOperand(op) => {
                encode_header(InstructionFormat::NoOperand, no_operand_opcode(op))
            }
        }
    }
}

const FORMAT_SHIFT: u32 = 29;
const OPCODE_SHIFT: u32 = 24;
const OPCODE_MASK: u32 = 0x1f;
const RESERVED_12_MASK: u32 = 0x00ff_f000;
const RESERVED_24_MASK: u32 = 0x00ff_ffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum InstructionFormat {
    N1 = 0,
    N1Indirect = 1,
    N2 = 2,
    N2Indirect = 3,
    Immediate = 4,
    MemoryImmediate = 5,
    MemoryImmediateIndirect = 6,
    NoOperand = 7,
}

const fn encode_header(format: InstructionFormat, opcode: u32) -> Word {
    ((format as u32) << FORMAT_SHIFT) | (opcode << OPCODE_SHIFT)
}

mod operand_opcode {
    pub const ADD: u32 = 0x00;
    pub const SUB: u32 = 0x01;
    pub const AND: u32 = 0x02;
    pub const OR: u32 = 0x03;
    pub const XOR: u32 = 0x04;
    pub const LDA: u32 = 0x05;
    pub const STA: u32 = 0x06;
    pub const MOVE: u32 = 0x07;
    pub const BUN: u32 = 0x08;
    pub const BSA: u32 = 0x09;
    pub const ISZ: u32 = 0x0a;
    pub const JPA: u32 = 0x0b;
    pub const JZA: u32 = 0x0c;
    pub const JNA: u32 = 0x0d;
    pub const JZE: u32 = 0x0e;
}

mod no_operand_opcode {
    pub const CLA: u32 = 0x00;
    pub const CLE: u32 = 0x01;
    pub const CMA: u32 = 0x02;
    pub const CME: u32 = 0x03;
    pub const CIR: u32 = 0x04;
    pub const CIL: u32 = 0x05;
    pub const INC: u32 = 0x06;
    pub const SPA: u32 = 0x07;
    pub const SZA: u32 = 0x08;
    pub const SNA: u32 = 0x09;
    pub const SZE: u32 = 0x0a;
    pub const INP: u32 = 0x0b;
    pub const OUT: u32 = 0x0c;
    pub const SKI: u32 = 0x0d;
    pub const SKO: u32 = 0x0e;
    pub const ION: u32 = 0x0f;
    pub const IOF: u32 = 0x10;
    pub const SIO: u32 = 0x11;
    pub const PIO: u32 = 0x12;
    pub const IMK: u32 = 0x13;
    pub const HLT: u32 = 0x14;
}

const fn n1_opcode(op: N1Op) -> u32 {
    use operand_opcode as opcode;
    match op {
        N1Op::Add => opcode::ADD,
        N1Op::Sub => opcode::SUB,
        N1Op::And => opcode::AND,
        N1Op::Or => opcode::OR,
        N1Op::Xor => opcode::XOR,
        N1Op::Lda => opcode::LDA,
        N1Op::Sta => opcode::STA,
        N1Op::Bun => opcode::BUN,
        N1Op::Bsa => opcode::BSA,
        N1Op::Jpa => opcode::JPA,
        N1Op::Jza => opcode::JZA,
        N1Op::Jna => opcode::JNA,
        N1Op::Jze => opcode::JZE,
        N1Op::Isz => opcode::ISZ,
    }
}
const fn n2_opcode(op: N2Op) -> u32 {
    use operand_opcode as opcode;
    match op {
        N2Op::Add => opcode::ADD,
        N2Op::Sub => opcode::SUB,
        N2Op::And => opcode::AND,
        N2Op::Or => opcode::OR,
        N2Op::Xor => opcode::XOR,
        N2Op::Move => opcode::MOVE,
    }
}
const fn immediate_opcode(op: ImmediateOp) -> u32 {
    use operand_opcode as opcode;
    match op {
        ImmediateOp::Add => opcode::ADD,
        ImmediateOp::And => opcode::AND,
        ImmediateOp::Or => opcode::OR,
        ImmediateOp::Lda => opcode::LDA,
    }
}
const fn memory_immediate_opcode(op: MemoryImmediateOp) -> u32 {
    use operand_opcode as opcode;
    match op {
        MemoryImmediateOp::Add => opcode::ADD,
        MemoryImmediateOp::And => opcode::AND,
        MemoryImmediateOp::Or => opcode::OR,
        MemoryImmediateOp::Sta => opcode::STA,
    }
}
const fn no_operand_opcode(op: NoOperandOp) -> u32 {
    use no_operand_opcode as opcode;
    match op {
        NoOperandOp::Cla => opcode::CLA,
        NoOperandOp::Cle => opcode::CLE,
        NoOperandOp::Cma => opcode::CMA,
        NoOperandOp::Cme => opcode::CME,
        NoOperandOp::Cir => opcode::CIR,
        NoOperandOp::Cil => opcode::CIL,
        NoOperandOp::Inc => opcode::INC,
        NoOperandOp::Spa => opcode::SPA,
        NoOperandOp::Sza => opcode::SZA,
        NoOperandOp::Sna => opcode::SNA,
        NoOperandOp::Sze => opcode::SZE,
        NoOperandOp::Inp => opcode::INP,
        NoOperandOp::Out => opcode::OUT,
        NoOperandOp::Ski => opcode::SKI,
        NoOperandOp::Sko => opcode::SKO,
        NoOperandOp::Ion => opcode::ION,
        NoOperandOp::Iof => opcode::IOF,
        NoOperandOp::Sio => opcode::SIO,
        NoOperandOp::Pio => opcode::PIO,
        NoOperandOp::Imk => opcode::IMK,
        NoOperandOp::Hlt => opcode::HLT,
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
    let format = (word >> FORMAT_SHIFT) & 0x7;
    let opcode = (word >> OPCODE_SHIFT) & OPCODE_MASK;

    // Format identifies both operand layout and indirect addressing. Opcode is
    // decoded independently within the operations legal for that format.
    match format {
        0 | 1 => {
            if word & RESERVED_12_MASK != 0 {
                return Err(unknown());
            }
            let op = N1_OPS
                .iter()
                .copied()
                .find(|op| n1_opcode(*op) == opcode)
                .ok_or_else(unknown)?;
            Ok(Instruction::N1 {
                op,
                operand: Address::from_low12(word),
                indirect: format == 1,
            })
        }
        2 | 3 => {
            let op = N2_OPS
                .iter()
                .copied()
                .find(|op| n2_opcode(*op) == opcode)
                .ok_or_else(unknown)?;
            Ok(Instruction::N2 {
                op,
                operand1: Address::from_low12(word >> 12),
                operand2: Address::from_low12(word),
                indirect: format == 3,
            })
        }
        4 => {
            if word & RESERVED_12_MASK != 0 {
                return Err(unknown());
            }
            let op = IMMEDIATE_OPS
                .iter()
                .copied()
                .find(|op| immediate_opcode(*op) == opcode)
                .ok_or_else(unknown)?;
            Ok(Instruction::Immediate {
                op,
                value: Immediate12::from_raw((word & 0xfff) as u16).expect("masked"),
            })
        }
        5 | 6 => {
            let op = MEMORY_IMMEDIATE_OPS
                .iter()
                .copied()
                .find(|op| memory_immediate_opcode(*op) == opcode)
                .ok_or_else(unknown)?;
            Ok(Instruction::MemoryImmediate {
                op,
                operand: Address::from_low12(word >> 12),
                value: Immediate12::from_raw((word & 0xfff) as u16).expect("masked"),
                indirect: format == 6,
            })
        }
        7 => {
            if word & RESERVED_24_MASK != 0 {
                return Err(unknown());
            }
            let op = NO_OPERAND_OPS
                .iter()
                .copied()
                .find(|op| no_operand_opcode(*op) == opcode)
                .ok_or_else(unknown)?;
            Ok(Instruction::NoOperand(op))
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
                op,
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
                op,
                operand1.get(),
                operand2.get(),
                if indirect { " I" } else { "" }
            ),
            Self::Immediate { op, value } => write!(f, "{op} {}", value.as_i32()),
            Self::MemoryImmediate {
                op,
                operand,
                value,
                indirect,
            } => write!(
                f,
                "{} {:03x} {}{}",
                op,
                operand.get(),
                value.as_i32(),
                if indirect { " I" } else { "" }
            ),
            Self::NoOperand(op) => write!(f, "{op}"),
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

/// Returned when a mnemonic does not belong to the requested instruction format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MnemonicParseError;

impl fmt::Display for MnemonicParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown mnemonic for instruction format")
    }
}
impl Error for MnemonicParseError {}

macro_rules! impl_mnemonic {
    ($op:ty, $ops:ident, $name:ident) => {
        impl FromStr for $op {
            type Err = MnemonicParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                $ops.iter()
                    .copied()
                    .find(|op| $name(*op) == value)
                    .ok_or(MnemonicParseError)
            }
        }

        impl fmt::Display for $op {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($name(*self))
            }
        }
    };
}

impl_mnemonic!(N1Op, N1_OPS, n1_name);
impl_mnemonic!(N2Op, N2_OPS, n2_name);
impl_mnemonic!(ImmediateOp, IMMEDIATE_OPS, immediate_name);
impl_mnemonic!(
    MemoryImmediateOp,
    MEMORY_IMMEDIATE_OPS,
    memory_immediate_name
);
impl_mnemonic!(NoOperandOp, NO_OPERAND_OPS, no_operand_name);

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
            0x00000123
        );
        assert_eq!(
            Instruction::N1 {
                op: N1Op::Add,
                operand: a,
                indirect: true
            }
            .encode(),
            0x20000123
        );
        assert_eq!(
            Instruction::Immediate {
                op: ImmediateOp::Lda,
                value: Immediate12::from_signed(1).unwrap()
            }
            .encode(),
            0x85000001
        );
    }
    #[test]
    fn opcode_reorganization_golden_vectors() {
        let a = Address::new(0x123).unwrap();
        let b = Address::new(0x456).unwrap();
        assert_eq!(
            Instruction::N1 {
                op: N1Op::Add,
                operand: a,
                indirect: false,
            }
            .encode(),
            0x0000_0123
        );
        assert_eq!(
            Instruction::N1 {
                op: N1Op::Add,
                operand: a,
                indirect: true,
            }
            .encode(),
            0x2000_0123
        );
        assert_eq!(
            Instruction::N2 {
                op: N2Op::Add,
                operand1: a,
                operand2: b,
                indirect: false,
            }
            .encode(),
            0x4012_3456
        );
        assert_eq!(
            Instruction::Immediate {
                op: ImmediateOp::Add,
                value: Immediate12::from_signed(5).unwrap(),
            }
            .encode(),
            0x8000_0005
        );
        assert_eq!(
            Instruction::Immediate {
                op: ImmediateOp::Lda,
                value: Immediate12::from_signed(-1).unwrap(),
            }
            .encode(),
            0x8500_0fff
        );
        assert_eq!(
            Instruction::MemoryImmediate {
                op: MemoryImmediateOp::Sta,
                operand: a,
                value: Immediate12::from_signed(42).unwrap(),
                indirect: false,
            }
            .encode(),
            0xa612_302a
        );
        assert_eq!(
            Instruction::NoOperand(NoOperandOp::Hlt).encode(),
            0xf400_0000
        );
    }
    #[test]
    fn every_mnemonic_has_the_specified_opcode() {
        let n1_expected = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x08, 0x09, 0x0b, 0x0c, 0x0d, 0x0e, 0x0a,
        ];
        let n2_expected = [0x00, 0x01, 0x02, 0x03, 0x04, 0x07];
        let immediate_expected = [0x00, 0x02, 0x03, 0x05];
        let memory_immediate_expected = [0x00, 0x02, 0x03, 0x06];

        for (&op, expected) in N1_OPS.iter().zip(n1_expected) {
            assert_eq!(n1_opcode(op), expected, "N1 {op}");
        }
        for (&op, expected) in N2_OPS.iter().zip(n2_expected) {
            assert_eq!(n2_opcode(op), expected, "N2 {op}");
        }
        for (&op, expected) in IMMEDIATE_OPS.iter().zip(immediate_expected) {
            assert_eq!(immediate_opcode(op), expected, "1N {op}");
        }
        for (&op, expected) in MEMORY_IMMEDIATE_OPS.iter().zip(memory_immediate_expected) {
            assert_eq!(memory_immediate_opcode(op), expected, "11 {op}");
        }
        for (&op, expected) in NO_OPERAND_OPS.iter().zip(0x00..=0x14) {
            assert_eq!(no_operand_opcode(op), expected, "NN {op}");
        }
    }
    #[test]
    fn format_field_encodes_indirect_addressing() {
        let address = Address::new(0xfff).unwrap();
        let immediate = Immediate12::from_raw(0xfff).unwrap();
        let formats = [
            Instruction::N1 {
                op: N1Op::Add,
                operand: address,
                indirect: false,
            },
            Instruction::N1 {
                op: N1Op::Add,
                operand: address,
                indirect: true,
            },
            Instruction::N2 {
                op: N2Op::Add,
                operand1: address,
                operand2: address,
                indirect: false,
            },
            Instruction::N2 {
                op: N2Op::Add,
                operand1: address,
                operand2: address,
                indirect: true,
            },
            Instruction::Immediate {
                op: ImmediateOp::Add,
                value: immediate,
            },
            Instruction::MemoryImmediate {
                op: MemoryImmediateOp::Add,
                operand: address,
                value: immediate,
                indirect: false,
            },
            Instruction::MemoryImmediate {
                op: MemoryImmediateOp::Add,
                operand: address,
                value: immediate,
                indirect: true,
            },
            Instruction::NoOperand(NoOperandOp::Cla),
        ];

        for (expected_format, instruction) in formats.into_iter().enumerate() {
            assert_eq!(instruction.encode() >> FORMAT_SHIFT, expected_format as u32);
            assert_eq!(decode(instruction.encode()), Ok(instruction));
        }
    }
    #[test]
    fn all_round_trip() {
        let a = Address::new(0xabc).unwrap();
        let b = Address::new(0x123).unwrap();
        let i = Immediate12::from_signed(-1).unwrap();
        for &op in N1_OPS {
            for indirect in [false, true] {
                let x = Instruction::N1 {
                    op,
                    operand: a,
                    indirect,
                };
                assert_eq!(decode(x.encode()), Ok(x));
            }
        }
        for &op in N2_OPS {
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
        for &op in IMMEDIATE_OPS {
            let x = Instruction::Immediate { op, value: i };
            assert_eq!(decode(x.encode()), Ok(x));
        }
        for &op in MEMORY_IMMEDIATE_OPS {
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
        for &op in NO_OPERAND_OPS {
            let x = Instruction::NoOperand(op);
            assert_eq!(decode(x.encode()), Ok(x));
        }
    }
    #[test]
    fn mnemonic_round_trip_uses_isa_registry() {
        for &op in N1_OPS {
            assert_eq!(op.to_string().parse::<N1Op>(), Ok(op));
        }
        for &op in N2_OPS {
            assert_eq!(op.to_string().parse::<N2Op>(), Ok(op));
        }
        for &op in IMMEDIATE_OPS {
            assert_eq!(op.to_string().parse::<ImmediateOp>(), Ok(op));
        }
        for &op in MEMORY_IMMEDIATE_OPS {
            assert_eq!(op.to_string().parse::<MemoryImmediateOp>(), Ok(op));
        }
        for &op in NO_OPERAND_OPS {
            assert_eq!(op.to_string().parse::<NoOperandOp>(), Ok(op));
        }
    }
    #[test]
    fn immediate_sign_extension() {
        assert_eq!(Immediate12::from_raw(0x000).unwrap().as_i32(), 0);
        assert_eq!(Immediate12::from_raw(0x7ff).unwrap().as_i32(), 2047);
        assert_eq!(Immediate12::from_raw(0x800).unwrap().as_i32(), -2048);
        assert_eq!(Immediate12::from_raw(0xfff).unwrap().as_i32(), -1);
    }
    #[test]
    fn address_boundaries() {
        assert_eq!(Address::new(0x000).unwrap().get(), 0x000);
        assert_eq!(Address::new(0xfff).unwrap().get(), 0xfff);
        assert!(Address::new(0x1000).is_err());
    }
    #[test]
    fn rejects_reserved_bits() {
        assert!(decode(0x0000_1000).is_err()); // N1 reserved payload
        assert!(decode(0x8000_1000).is_err()); // 1N reserved payload
        assert!(decode(0xe000_0001).is_err()); // NN reserved payload
        assert!(decode(0x8100_0000).is_err()); // immediate SUB is undefined
        assert!(decode(0x4500_0000).is_err()); // N2 LDA is undefined
        assert!(decode(0xa100_0000).is_err()); // memory-immediate SUB is undefined
        assert!(decode(0x1f00_0000).is_err()); // unassigned operand opcode
        assert!(decode(0xff00_0000).is_err()); // unassigned NN opcode
        assert!(decode(0x8010_0000).is_err()); // historical HLT encoding is incompatible
    }
    #[test]
    fn zero_word_is_new_n1_add_address_zero() {
        assert_eq!(
            decode(0),
            Ok(Instruction::N1 {
                op: N1Op::Add,
                operand: Address::ZERO,
                indirect: false,
            })
        );
    }
}

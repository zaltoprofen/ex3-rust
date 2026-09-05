//! EX3 v3.0 instruction model and 32-bit encoder/decoder.

use std::{error::Error, fmt, str::FromStr};

pub type Word = u32;
pub const MEMORY_SIZE: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Address(u16);

impl Address {
    pub const ZERO: Self = Self(0);
    pub const RESET: Self = Self(0x0010);
    pub const fn new(value: u16) -> Result<Self, ValueError> {
        Ok(Self(value))
    }
    pub const fn from_low16(value: u32) -> Self {
        Self(value as u16)
    }
    pub const fn get(self) -> u16 {
        self.0
    }
    pub const fn wrapping_add(self, rhs: u16) -> Self {
        Self(self.0.wrapping_add(rhs))
    }
    pub const fn wrapping_add_signed(self, rhs: i16) -> Self {
        Self(self.0.wrapping_add(rhs as u16))
    }
}
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Immediate16(u16);
impl Immediate16 {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }
    pub fn from_signed(value: i32) -> Result<Self, ValueError> {
        if (-32_768..=32_767).contains(&value) {
            Ok(Self(value as u16))
        } else {
            Err(ValueError::ImmediateOutOfRange(value as i64))
        }
    }
    pub const fn raw(self) -> u16 {
        self.0
    }
    pub const fn as_i16(self) -> i16 {
        self.0 as i16
    }
    pub const fn sign_extended(self) -> Word {
        self.as_i16() as i32 as Word
    }
    pub const fn zero_extended(self) -> Word {
        self.0 as Word
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
            Self::AddressOutOfRange(v) => write!(f, "address out of 16-bit range: {v}"),
            Self::ImmediateOutOfRange(v) => write!(f, "immediate out of 16-bit range: {v}"),
        }
    }
}
impl Error for ValueError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Lda,
    Sta,
    Cmp,
    Isz,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImmediateOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Lda,
    Cmp,
    Ldhi,
    Ldlo,
    Adjsp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpRelativeOp {
    Addsp,
    Subsp,
    Andsp,
    Orsp,
    Xorsp,
    Ldsp,
    Stsp,
    Cmpsp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchOp {
    Jmp,
    Call,
    Beq,
    Bne,
    Blt,
    Bge,
    Bgt,
    Ble,
    Bult,
    Buge,
    Bugt,
    Bule,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemOp {
    Cla,
    Cma,
    Ret,
    Iret,
    Hlt,
    Inp,
    Out,
    Ski,
    Sko,
    Ion,
    Iof,
    Sio,
    Pio,
    Imk,
}

pub const MEMORY_OPS: &[MemoryOp] = &[
    MemoryOp::Add,
    MemoryOp::Sub,
    MemoryOp::And,
    MemoryOp::Or,
    MemoryOp::Xor,
    MemoryOp::Lda,
    MemoryOp::Sta,
    MemoryOp::Cmp,
    MemoryOp::Isz,
];
pub const IMMEDIATE_OPS: &[ImmediateOp] = &[
    ImmediateOp::Add,
    ImmediateOp::Sub,
    ImmediateOp::And,
    ImmediateOp::Or,
    ImmediateOp::Xor,
    ImmediateOp::Lda,
    ImmediateOp::Cmp,
    ImmediateOp::Ldhi,
    ImmediateOp::Ldlo,
    ImmediateOp::Adjsp,
];
pub const SP_RELATIVE_OPS: &[SpRelativeOp] = &[
    SpRelativeOp::Addsp,
    SpRelativeOp::Subsp,
    SpRelativeOp::Andsp,
    SpRelativeOp::Orsp,
    SpRelativeOp::Xorsp,
    SpRelativeOp::Ldsp,
    SpRelativeOp::Stsp,
    SpRelativeOp::Cmpsp,
];
pub const BRANCH_OPS: &[BranchOp] = &[
    BranchOp::Jmp,
    BranchOp::Call,
    BranchOp::Beq,
    BranchOp::Bne,
    BranchOp::Blt,
    BranchOp::Bge,
    BranchOp::Bgt,
    BranchOp::Ble,
    BranchOp::Bult,
    BranchOp::Buge,
    BranchOp::Bugt,
    BranchOp::Bule,
];
pub const SYSTEM_OPS: &[SystemOp] = &[
    SystemOp::Cla,
    SystemOp::Cma,
    SystemOp::Ret,
    SystemOp::Iret,
    SystemOp::Hlt,
    SystemOp::Inp,
    SystemOp::Out,
    SystemOp::Ski,
    SystemOp::Sko,
    SystemOp::Ion,
    SystemOp::Iof,
    SystemOp::Sio,
    SystemOp::Pio,
    SystemOp::Imk,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instruction {
    Memory {
        op: MemoryOp,
        address: Address,
        indirect: bool,
    },
    Immediate {
        op: ImmediateOp,
        value: Immediate16,
    },
    SpRelative {
        op: SpRelativeOp,
        offset: Immediate16,
    },
    Branch {
        op: BranchOp,
        target: Address,
    },
    System(SystemOp),
}

const FORMAT_SHIFT: u32 = 29;
const OPCODE_SHIFT: u32 = 24;
const MODIFIER_SHIFT: u32 = 16;
const FORMAT_MEM: u32 = 0;
const FORMAT_IMM: u32 = 1;
const FORMAT_SPREL: u32 = 2;
const FORMAT_BRANCH: u32 = 3;
const FORMAT_SYS: u32 = 4;
const fn memory_opcode(op: MemoryOp) -> u32 {
    match op {
        MemoryOp::Add => 0,
        MemoryOp::Sub => 1,
        MemoryOp::And => 2,
        MemoryOp::Or => 3,
        MemoryOp::Xor => 4,
        MemoryOp::Lda => 5,
        MemoryOp::Sta => 6,
        MemoryOp::Cmp => 7,
        MemoryOp::Isz => 8,
    }
}
const fn immediate_opcode(op: ImmediateOp) -> u32 {
    match op {
        ImmediateOp::Add => 0,
        ImmediateOp::Sub => 1,
        ImmediateOp::And => 2,
        ImmediateOp::Or => 3,
        ImmediateOp::Xor => 4,
        ImmediateOp::Lda => 5,
        ImmediateOp::Cmp => 7,
        ImmediateOp::Ldhi => 9,
        ImmediateOp::Ldlo => 10,
        ImmediateOp::Adjsp => 11,
    }
}
const fn sp_opcode(op: SpRelativeOp) -> u32 {
    match op {
        SpRelativeOp::Addsp => 0,
        SpRelativeOp::Subsp => 1,
        SpRelativeOp::Andsp => 2,
        SpRelativeOp::Orsp => 3,
        SpRelativeOp::Xorsp => 4,
        SpRelativeOp::Ldsp => 5,
        SpRelativeOp::Stsp => 6,
        SpRelativeOp::Cmpsp => 7,
    }
}
const fn branch_opcode(op: BranchOp) -> u32 {
    match op {
        BranchOp::Jmp => 0,
        BranchOp::Call => 1,
        BranchOp::Beq => 2,
        BranchOp::Bne => 3,
        BranchOp::Blt => 4,
        BranchOp::Bge => 5,
        BranchOp::Bgt => 6,
        BranchOp::Ble => 7,
        BranchOp::Bult => 8,
        BranchOp::Buge => 9,
        BranchOp::Bugt => 10,
        BranchOp::Bule => 11,
    }
}
const fn system_opcode(op: SystemOp) -> u32 {
    match op {
        SystemOp::Cla => 0,
        SystemOp::Cma => 1,
        SystemOp::Ret => 2,
        SystemOp::Iret => 3,
        SystemOp::Hlt => 4,
        SystemOp::Inp => 5,
        SystemOp::Out => 6,
        SystemOp::Ski => 7,
        SystemOp::Sko => 8,
        SystemOp::Ion => 9,
        SystemOp::Iof => 10,
        SystemOp::Sio => 11,
        SystemOp::Pio => 12,
        SystemOp::Imk => 13,
    }
}

impl Instruction {
    pub const fn encode(self) -> Word {
        match self {
            Self::Memory {
                op,
                address,
                indirect,
            } => {
                (FORMAT_MEM << FORMAT_SHIFT)
                    | (memory_opcode(op) << OPCODE_SHIFT)
                    | ((indirect as u32) << MODIFIER_SHIFT)
                    | address.get() as u32
            }
            Self::Immediate { op, value } => {
                (FORMAT_IMM << FORMAT_SHIFT)
                    | (immediate_opcode(op) << OPCODE_SHIFT)
                    | value.raw() as u32
            }
            Self::SpRelative { op, offset } => {
                (FORMAT_SPREL << FORMAT_SHIFT)
                    | (sp_opcode(op) << OPCODE_SHIFT)
                    | offset.raw() as u32
            }
            Self::Branch { op, target } => {
                (FORMAT_BRANCH << FORMAT_SHIFT)
                    | (branch_opcode(op) << OPCODE_SHIFT)
                    | target.get() as u32
            }
            Self::System(op) => (FORMAT_SYS << FORMAT_SHIFT) | (system_opcode(op) << OPCODE_SHIFT),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub word: Word,
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "illegal EX3 v3 instruction: 0x{:08x}", self.word)
    }
}
impl Error for DecodeError {}

pub fn decode(word: Word) -> Result<Instruction, DecodeError> {
    let fail = || DecodeError { word };
    let format = word >> FORMAT_SHIFT;
    let opcode = (word >> OPCODE_SHIFT) & 0x1f;
    let modifier = ((word >> MODIFIER_SHIFT) & 0xff) as u8;
    let operand = Immediate16::from_raw(word as u16);
    match format {
        FORMAT_MEM => {
            if modifier & !1 != 0 {
                return Err(fail());
            }
            let op = MEMORY_OPS
                .iter()
                .copied()
                .find(|x| memory_opcode(*x) == opcode)
                .ok_or_else(fail)?;
            Ok(Instruction::Memory {
                op,
                address: Address::from_low16(word),
                indirect: modifier == 1,
            })
        }
        FORMAT_IMM => {
            if modifier != 0 {
                return Err(fail());
            }
            let op = IMMEDIATE_OPS
                .iter()
                .copied()
                .find(|x| immediate_opcode(*x) == opcode)
                .ok_or_else(fail)?;
            Ok(Instruction::Immediate { op, value: operand })
        }
        FORMAT_SPREL => {
            if modifier != 0 {
                return Err(fail());
            }
            let op = SP_RELATIVE_OPS
                .iter()
                .copied()
                .find(|x| sp_opcode(*x) == opcode)
                .ok_or_else(fail)?;
            Ok(Instruction::SpRelative {
                op,
                offset: operand,
            })
        }
        FORMAT_BRANCH => {
            if modifier != 0 {
                return Err(fail());
            }
            let op = BRANCH_OPS
                .iter()
                .copied()
                .find(|x| branch_opcode(*x) == opcode)
                .ok_or_else(fail)?;
            Ok(Instruction::Branch {
                op,
                target: Address::from_low16(word),
            })
        }
        FORMAT_SYS => {
            if word & 0x00ff_ffff != 0 {
                return Err(fail());
            }
            let op = SYSTEM_OPS
                .iter()
                .copied()
                .find(|x| system_opcode(*x) == opcode)
                .ok_or_else(fail)?;
            Ok(Instruction::System(op))
        }
        _ => Err(fail()),
    }
}

macro_rules! named_enum { ($ty:ty, $list:ident, $name:ident, {$($variant:path => $text:literal),+ $(,)?}) => {
    pub const fn $name(op: $ty) -> &'static str { match op { $($variant => $text),+ } }
    impl FromStr for $ty { type Err=MnemonicParseError; fn from_str(s:&str)->Result<Self,Self::Err>{ $list.iter().copied().find(|x|$name(*x)==s).ok_or(MnemonicParseError) } }
    impl fmt::Display for $ty { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str($name(*self))} }
}; }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MnemonicParseError;
impl fmt::Display for MnemonicParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown EX3 v3 mnemonic")
    }
}
impl Error for MnemonicParseError {}
named_enum!(MemoryOp,MEMORY_OPS,memory_name,{MemoryOp::Add=>"ADD",MemoryOp::Sub=>"SUB",MemoryOp::And=>"AND",MemoryOp::Or=>"OR",MemoryOp::Xor=>"XOR",MemoryOp::Lda=>"LDA",MemoryOp::Sta=>"STA",MemoryOp::Cmp=>"CMP",MemoryOp::Isz=>"ISZ"});
named_enum!(ImmediateOp,IMMEDIATE_OPS,immediate_name,{ImmediateOp::Add=>"ADD",ImmediateOp::Sub=>"SUB",ImmediateOp::And=>"AND",ImmediateOp::Or=>"OR",ImmediateOp::Xor=>"XOR",ImmediateOp::Lda=>"LDA",ImmediateOp::Cmp=>"CMP",ImmediateOp::Ldhi=>"LDHI",ImmediateOp::Ldlo=>"LDLO",ImmediateOp::Adjsp=>"ADJSP"});
named_enum!(SpRelativeOp,SP_RELATIVE_OPS,sp_relative_name,{SpRelativeOp::Addsp=>"ADDSP",SpRelativeOp::Subsp=>"SUBSP",SpRelativeOp::Andsp=>"ANDSP",SpRelativeOp::Orsp=>"ORSP",SpRelativeOp::Xorsp=>"XORSP",SpRelativeOp::Ldsp=>"LDSP",SpRelativeOp::Stsp=>"STSP",SpRelativeOp::Cmpsp=>"CMPSP"});
named_enum!(BranchOp,BRANCH_OPS,branch_name,{BranchOp::Jmp=>"JMP",BranchOp::Call=>"CALL",BranchOp::Beq=>"BEQ",BranchOp::Bne=>"BNE",BranchOp::Blt=>"BLT",BranchOp::Bge=>"BGE",BranchOp::Bgt=>"BGT",BranchOp::Ble=>"BLE",BranchOp::Bult=>"BULT",BranchOp::Buge=>"BUGE",BranchOp::Bugt=>"BUGT",BranchOp::Bule=>"BULE"});
named_enum!(SystemOp,SYSTEM_OPS,system_name,{SystemOp::Cla=>"CLA",SystemOp::Cma=>"CMA",SystemOp::Ret=>"RET",SystemOp::Iret=>"IRET",SystemOp::Hlt=>"HLT",SystemOp::Inp=>"INP",SystemOp::Out=>"OUT",SystemOp::Ski=>"SKI",SystemOp::Sko=>"SKO",SystemOp::Ion=>"ION",SystemOp::Iof=>"IOF",SystemOp::Sio=>"SIO",SystemOp::Pio=>"PIO",SystemOp::Imk=>"IMK"});

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Memory {
                op,
                address,
                indirect,
            } => write!(f, "{op} {address}{}", if indirect { " I" } else { "" }),
            Self::Immediate { op, value } => match op {
                ImmediateOp::And
                | ImmediateOp::Or
                | ImmediateOp::Xor
                | ImmediateOp::Ldhi
                | ImmediateOp::Ldlo => write!(f, "{op} 0x{:04x}", value.raw()),
                _ => write!(f, "{op} {}", value.as_i16()),
            },
            Self::SpRelative { op, offset } => write!(f, "{op} {}", offset.as_i16()),
            Self::Branch { op, target } => write!(f, "{op} {target}"),
            Self::System(op) => write!(f, "{op}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_encodings() {
        let a = Address::new(0x1234).unwrap();
        assert_eq!(
            Instruction::Memory {
                op: MemoryOp::Add,
                address: a,
                indirect: false
            }
            .encode(),
            0x0000_1234
        );
        assert_eq!(
            Instruction::Memory {
                op: MemoryOp::Lda,
                address: a,
                indirect: true
            }
            .encode(),
            0x0501_1234
        );
        assert_eq!(
            Instruction::Immediate {
                op: ImmediateOp::Lda,
                value: Immediate16::from_raw(0xffff)
            }
            .encode(),
            0x2500_ffff
        );
        assert_eq!(
            Instruction::SpRelative {
                op: SpRelativeOp::Ldsp,
                offset: Immediate16::from_raw(0xffff)
            }
            .encode(),
            0x4500_ffff
        );
        assert_eq!(
            Instruction::Branch {
                op: BranchOp::Call,
                target: a
            }
            .encode(),
            0x6100_1234
        );
        assert_eq!(Instruction::System(SystemOp::Hlt).encode(), 0x8400_0000);
    }
    #[test]
    fn all_legal_round_trip() {
        let a = Address::new(0xabcd).unwrap();
        let i = Immediate16::from_raw(0x8123);
        for &op in MEMORY_OPS {
            for indirect in [false, true] {
                let x = Instruction::Memory {
                    op,
                    address: a,
                    indirect,
                };
                assert_eq!(decode(x.encode()), Ok(x));
            }
        }
        for &op in IMMEDIATE_OPS {
            let x = Instruction::Immediate { op, value: i };
            assert_eq!(decode(x.encode()), Ok(x));
        }
        for &op in SP_RELATIVE_OPS {
            let x = Instruction::SpRelative { op, offset: i };
            assert_eq!(decode(x.encode()), Ok(x));
        }
        for &op in BRANCH_OPS {
            let x = Instruction::Branch { op, target: a };
            assert_eq!(decode(x.encode()), Ok(x));
        }
        for &op in SYSTEM_OPS {
            let x = Instruction::System(op);
            assert_eq!(decode(x.encode()), Ok(x));
        }
    }
    #[test]
    fn rejects_illegal() {
        assert!(decode(0x0002_0000).is_err());
        assert!(decode(0x2001_0000).is_err());
        assert!(decode(0x8000_0001).is_err());
        assert!(decode(0xa000_0000).is_err());
        assert!(decode(0xc000_0000).is_err());
    }
}

//! EX3 v3.0 CPU, unified memory, and peripheral interfaces.

use crate::{
    assembler::MemoryImage,
    isa::{
        decode, Address, BranchOp, DecodeError, ImmediateOp, Instruction, MemoryOp, SpRelativeOp,
        SystemOp, Word, MEMORY_SIZE,
    },
};
use std::{collections::VecDeque, error::Error, fmt};

pub const PSR_V: Word = 1 << 0;
pub const PSR_C: Word = 1 << 1;
pub const PSR_Z: Word = 1 << 2;
pub const PSR_N: Word = 1 << 3;
pub const PSR_IEN: Word = 1 << 4;
pub const PSR_MASK: Word = PSR_V | PSR_C | PSR_Z | PSR_N | PSR_IEN;

pub trait Memory {
    fn read(&self, addr: Address) -> Word;
    fn write(&mut self, addr: Address, value: Word);
}
#[derive(Clone)]
pub struct ArrayMemory {
    words: Box<[Word; MEMORY_SIZE]>,
}
impl Default for ArrayMemory {
    fn default() -> Self {
        Self {
            words: Box::new([0; MEMORY_SIZE]),
        }
    }
}
impl ArrayMemory {
    pub fn from_image(image: &MemoryImage) -> Self {
        let mut m = Self::default();
        for c in &image.cells {
            m.write(c.address, c.word)
        }
        m
    }
    pub fn from_cells(cells: &[(Address, Word)]) -> Self {
        let mut m = Self::default();
        for &(a, w) in cells {
            m.write(a, w)
        }
        m
    }
    pub fn words(&self) -> &[Word; MEMORY_SIZE] {
        &self.words
    }
}
impl Memory for ArrayMemory {
    fn read(&self, a: Address) -> Word {
        self.words[a.get() as usize]
    }
    fn write(&mut self, a: Address, v: Word) {
        self.words[a.get() as usize] = v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoKind {
    Serial,
    Parallel,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoTickContext {
    pub interrupt_enabled: bool,
    pub interrupt_mask: u8,
}
pub trait IoBus {
    fn tick(&mut self, _context: IoTickContext) {}
    fn interrupt_pending(&self) -> bool {
        false
    }
    fn read_input(&mut self, kind: IoKind) -> Option<u8>;
    fn write_output(&mut self, kind: IoKind, value: u8);
    fn input_ready(&self, kind: IoKind) -> bool;
    fn output_ready(&self, kind: IoKind) -> bool;
}
#[derive(Default)]
pub struct NullIoBus;
impl IoBus for NullIoBus {
    fn read_input(&mut self, _: IoKind) -> Option<u8> {
        None
    }
    fn write_output(&mut self, _: IoKind, _: u8) {}
    fn input_ready(&self, _: IoKind) -> bool {
        false
    }
    fn output_ready(&self, _: IoKind) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
pub struct DeterministicIoBus {
    serial_input: VecDeque<u8>,
    parallel_input: VecDeque<u8>,
    serial_output: Vec<u8>,
    parallel_output: Vec<u8>,
    serial_output_ready: bool,
    parallel_output_ready: bool,
    force_interrupt: bool,
}
impl Default for DeterministicIoBus {
    fn default() -> Self {
        Self {
            serial_input: VecDeque::new(),
            parallel_input: VecDeque::new(),
            serial_output: Vec::new(),
            parallel_output: Vec::new(),
            serial_output_ready: true,
            parallel_output_ready: true,
            force_interrupt: false,
        }
    }
}
impl DeterministicIoBus {
    pub fn push_input(&mut self, k: IoKind, v: u8) {
        match k {
            IoKind::Serial => self.serial_input.push_back(v),
            IoKind::Parallel => self.parallel_input.push_back(v),
        }
    }
    pub fn output(&self, k: IoKind) -> &[u8] {
        match k {
            IoKind::Serial => &self.serial_output,
            IoKind::Parallel => &self.parallel_output,
        }
    }
    pub fn request_interrupt(&mut self) {
        self.force_interrupt = true
    }
    pub fn clear_interrupt(&mut self) {
        self.force_interrupt = false
    }
}
impl IoBus for DeterministicIoBus {
    fn tick(&mut self, _: IoTickContext) {
        self.serial_output_ready = true;
        self.parallel_output_ready = true
    }
    fn interrupt_pending(&self) -> bool {
        self.force_interrupt
    }
    fn read_input(&mut self, k: IoKind) -> Option<u8> {
        match k {
            IoKind::Serial => self.serial_input.pop_front(),
            IoKind::Parallel => self.parallel_input.pop_front(),
        }
    }
    fn write_output(&mut self, k: IoKind, v: u8) {
        match k {
            IoKind::Serial => {
                self.serial_output.push(v);
                self.serial_output_ready = false
            }
            IoKind::Parallel => {
                self.parallel_output.push(v);
                self.parallel_output_ready = false
            }
        }
    }
    fn input_ready(&self, k: IoKind) -> bool {
        match k {
            IoKind::Serial => !self.serial_input.is_empty(),
            IoKind::Parallel => !self.parallel_input.is_empty(),
        }
    }
    fn output_ready(&self, k: IoKind) -> bool {
        match k {
            IoKind::Serial => self.serial_output_ready,
            IoKind::Parallel => self.parallel_output_ready,
        }
    }
}

/// Seeded peripheral model retained as a CLI I/O backend; CPU semantics are always v3.
#[derive(Clone, Debug)]
pub struct LegacyIoBus {
    inner: DeterministicIoBus,
    rng: u64,
    serial_pending: VecDeque<u8>,
    parallel_pending: VecDeque<u8>,
    serial_delay: Option<u8>,
    parallel_delay: Option<u8>,
}
impl LegacyIoBus {
    pub fn new(seed: u64) -> Self {
        Self {
            inner: DeterministicIoBus::default(),
            rng: seed,
            serial_pending: VecDeque::new(),
            parallel_pending: VecDeque::new(),
            serial_delay: None,
            parallel_delay: None,
        }
    }
    pub fn push_input(&mut self, k: IoKind, v: u8) {
        match k {
            IoKind::Serial => self.serial_pending.push_back(v),
            IoKind::Parallel => self.parallel_pending.push_back(v),
        }
    }
    pub fn output(&self, k: IoKind) -> &[u8] {
        self.inner.output(k)
    }
    fn delay(&mut self) -> u8 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.rng >> 32) % 50 + 1) as u8
    }
}
impl Default for LegacyIoBus {
    fn default() -> Self {
        Self::new(0)
    }
}
impl IoBus for LegacyIoBus {
    fn tick(&mut self, c: IoTickContext) {
        self.inner.tick(c);
        if c.interrupt_enabled
            && c.interrupt_mask & 8 != 0
            && !self.serial_pending.is_empty()
            && self.inner.serial_input.is_empty()
        {
            let delay = self.delay();
            advance_input(
                &mut self.serial_delay,
                &mut self.serial_pending,
                &mut self.inner.serial_input,
                delay,
            )
        }
        if c.interrupt_enabled
            && c.interrupt_mask & 2 != 0
            && !self.parallel_pending.is_empty()
            && self.inner.parallel_input.is_empty()
        {
            let delay = self.delay();
            advance_input(
                &mut self.parallel_delay,
                &mut self.parallel_pending,
                &mut self.inner.parallel_input,
                delay,
            )
        }
    }
    fn read_input(&mut self, k: IoKind) -> Option<u8> {
        self.inner.read_input(k)
    }
    fn write_output(&mut self, k: IoKind, v: u8) {
        self.inner.write_output(k, v)
    }
    fn input_ready(&self, k: IoKind) -> bool {
        self.inner.input_ready(k)
    }
    fn output_ready(&self, k: IoKind) -> bool {
        self.inner.output_ready(k)
    }
}
fn advance_input(
    delay: &mut Option<u8>,
    pending: &mut VecDeque<u8>,
    ready: &mut VecDeque<u8>,
    new_delay: u8,
) {
    match *delay {
        None => *delay = Some(new_delay),
        Some(0) => {
            if let Some(v) = pending.pop_front() {
                ready.push_back(v)
            }
            *delay = None
        }
        Some(n) => *delay = Some(n - 1),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoState {
    pub serial_selected: bool,
    pub interrupt_mask: u8,
    pub input_register: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuState {
    pub pc: Address,
    pub sp: Address,
    pub ac: Word,
    pub psr: Word,
    pub halted: bool,
    pub interrupt_pending: bool,
    pub ir: Word,
    pub executed_instructions: u64,
}
impl Default for CpuState {
    fn default() -> Self {
        Self {
            pc: Address::RESET,
            sp: Address::ZERO,
            ac: 0,
            psr: 0,
            halted: false,
            interrupt_pending: false,
            ir: 0,
            executed_instructions: 0,
        }
    }
}
impl CpuState {
    pub const fn negative(&self) -> bool {
        self.psr & PSR_N != 0
    }
    pub const fn zero(&self) -> bool {
        self.psr & PSR_Z != 0
    }
    pub const fn carry(&self) -> bool {
        self.psr & PSR_C != 0
    }
    pub const fn overflow(&self) -> bool {
        self.psr & PSR_V != 0
    }
    pub const fn interrupt_enabled(&self) -> bool {
        self.psr & PSR_IEN != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Executed {
        pc_before: Address,
        instruction: Instruction,
    },
    Interrupted,
    Halted,
}
#[derive(Debug)]
pub enum CpuError {
    Decode(DecodeError),
    StepLimitExceeded,
}
impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "{e}"),
            Self::StepLimitExceeded => f.write_str("instruction step limit exceeded"),
        }
    }
}
impl Error for CpuError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode(e) => Some(e),
            _ => None,
        }
    }
}
impl From<DecodeError> for CpuError {
    fn from(v: DecodeError) -> Self {
        Self::Decode(v)
    }
}

pub struct Cpu {
    state: CpuState,
    io: IoState,
}
impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}
impl Cpu {
    pub fn new() -> Self {
        Self {
            state: CpuState::default(),
            io: IoState::default(),
        }
    }
    pub fn reset(&mut self) {
        self.state = CpuState::default();
        self.io = IoState::default()
    }
    pub const fn state(&self) -> &CpuState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut CpuState {
        &mut self.state
    }
    pub const fn io_state(&self) -> &IoState {
        &self.io
    }
    pub fn io_state_mut(&mut self) -> &mut IoState {
        &mut self.io
    }
    pub fn step(
        &mut self,
        memory: &mut impl Memory,
        io: &mut impl IoBus,
    ) -> Result<StepOutcome, CpuError> {
        if self.state.halted {
            return Ok(StepOutcome::Halted);
        }
        let ien = self.state.interrupt_enabled();
        io.tick(IoTickContext {
            interrupt_enabled: ien,
            interrupt_mask: self.io.interrupt_mask,
        });
        let masked = (self.io.interrupt_mask & 8 != 0 && io.input_ready(IoKind::Serial))
            || (self.io.interrupt_mask & 4 != 0 && io.output_ready(IoKind::Serial))
            || (self.io.interrupt_mask & 2 != 0 && io.input_ready(IoKind::Parallel))
            || (self.io.interrupt_mask & 1 != 0 && io.output_ready(IoKind::Parallel));
        self.state.interrupt_pending = ien && (masked || io.interrupt_pending());
        if self.state.interrupt_pending {
            self.state.interrupt_pending = false;
            let saved_psr = self.state.psr;
            self.push(memory, saved_psr);
            self.push(memory, self.state.pc.get() as u32);
            self.set_flag(PSR_IEN, false);
            self.state.pc = Address::ZERO;
            return Ok(StepOutcome::Interrupted);
        }
        let pc_before = self.state.pc;
        self.state.ir = memory.read(pc_before);
        self.state.pc = self.state.pc.wrapping_add(1);
        self.state.executed_instructions = self.state.executed_instructions.wrapping_add(1);
        let instruction = match decode(self.state.ir) {
            Ok(i) => i,
            Err(e) => {
                self.state.halted = true;
                return Err(CpuError::Decode(e));
            }
        };
        self.execute(instruction, memory, io);
        Ok(StepOutcome::Executed {
            pc_before,
            instruction,
        })
    }
    pub fn run(
        &mut self,
        memory: &mut impl Memory,
        io: &mut impl IoBus,
        max_steps: u64,
    ) -> Result<u64, CpuError> {
        let start = self.state.executed_instructions;
        while !self.state.halted {
            if self.state.executed_instructions.wrapping_sub(start) >= max_steps {
                return Err(CpuError::StepLimitExceeded);
            }
            self.step(memory, io)?;
        }
        Ok(self.state.executed_instructions.wrapping_sub(start))
    }
    fn set_flag(&mut self, mask: Word, value: bool) {
        if value {
            self.state.psr |= mask
        } else {
            self.state.psr &= !mask
        }
        self.state.psr &= PSR_MASK
    }
    fn update_nz(&mut self, v: Word) {
        self.set_flag(PSR_N, v & 0x8000_0000 != 0);
        self.set_flag(PSR_Z, v == 0)
    }
    fn add(&mut self, b: Word) {
        let a = self.state.ac;
        let (r, c) = a.overflowing_add(b);
        let v = ((!(a ^ b) & (a ^ r)) & 0x8000_0000) != 0;
        self.state.ac = r;
        self.update_nz(r);
        self.set_flag(PSR_C, c);
        self.set_flag(PSR_V, v)
    }
    fn subtraction_flags(&mut self, a: Word, b: Word) -> Word {
        let r = a.wrapping_sub(b);
        self.update_nz(r);
        self.set_flag(PSR_C, a >= b);
        self.set_flag(PSR_V, (((a ^ b) & (a ^ r)) & 0x8000_0000) != 0);
        r
    }
    fn sub(&mut self, b: Word) {
        self.state.ac = self.subtraction_flags(self.state.ac, b)
    }
    fn logic(&mut self, v: Word) {
        self.state.ac = v;
        self.update_nz(v)
    }
    fn push(&mut self, m: &mut impl Memory, v: Word) {
        self.state.sp = self.state.sp.wrapping_add_signed(-1);
        m.write(self.state.sp, v)
    }
    fn pop(&mut self, m: &impl Memory) -> Word {
        let v = m.read(self.state.sp);
        self.state.sp = self.state.sp.wrapping_add(1);
        v
    }
    fn skip(&mut self) {
        self.state.pc = self.state.pc.wrapping_add(1)
    }
    fn selected(&self) -> IoKind {
        if self.io.serial_selected {
            IoKind::Serial
        } else {
            IoKind::Parallel
        }
    }
    fn execute(&mut self, i: Instruction, m: &mut impl Memory, io: &mut impl IoBus) {
        match i {
            Instruction::Memory {
                op,
                address,
                indirect,
            } => {
                let ea = if indirect {
                    Address::from_low16(m.read(address))
                } else {
                    address
                };
                let b = m.read(ea);
                match op {
                    MemoryOp::Add => self.add(b),
                    MemoryOp::Sub => self.sub(b),
                    MemoryOp::And => self.logic(self.state.ac & b),
                    MemoryOp::Or => self.logic(self.state.ac | b),
                    MemoryOp::Xor => self.logic(self.state.ac ^ b),
                    MemoryOp::Lda => self.logic(b),
                    MemoryOp::Sta => m.write(ea, self.state.ac),
                    MemoryOp::Cmp => {
                        self.subtraction_flags(self.state.ac, b);
                    }
                    MemoryOp::Isz => {
                        let v = b.wrapping_add(1);
                        m.write(ea, v);
                        if v == 0 {
                            self.skip()
                        }
                    }
                }
            }
            Instruction::Immediate { op, value } => match op {
                ImmediateOp::Add => self.add(value.sign_extended()),
                ImmediateOp::Sub => self.sub(value.sign_extended()),
                ImmediateOp::And => self.logic(self.state.ac & value.zero_extended()),
                ImmediateOp::Or => self.logic(self.state.ac | value.zero_extended()),
                ImmediateOp::Xor => self.logic(self.state.ac ^ value.zero_extended()),
                ImmediateOp::Lda => self.logic(value.sign_extended()),
                ImmediateOp::Cmp => {
                    self.subtraction_flags(self.state.ac, value.sign_extended());
                }
                ImmediateOp::Ldhi => {
                    self.logic((self.state.ac & 0x0000_ffff) | ((value.raw() as u32) << 16))
                }
                ImmediateOp::Ldlo => self.logic((self.state.ac & 0xffff_0000) | value.raw() as u32),
                ImmediateOp::Adjsp => {
                    self.state.sp = self.state.sp.wrapping_add_signed(value.as_i16())
                }
            },
            Instruction::SpRelative { op, offset } => {
                let ea = self.state.sp.wrapping_add_signed(offset.as_i16());
                let b = m.read(ea);
                match op {
                    SpRelativeOp::Addsp => self.add(b),
                    SpRelativeOp::Subsp => self.sub(b),
                    SpRelativeOp::Andsp => self.logic(self.state.ac & b),
                    SpRelativeOp::Orsp => self.logic(self.state.ac | b),
                    SpRelativeOp::Xorsp => self.logic(self.state.ac ^ b),
                    SpRelativeOp::Ldsp => self.logic(b),
                    SpRelativeOp::Stsp => m.write(ea, self.state.ac),
                    SpRelativeOp::Cmpsp => {
                        self.subtraction_flags(self.state.ac, b);
                    }
                }
            }
            Instruction::Branch { op, target } => {
                let take = match op {
                    BranchOp::Jmp | BranchOp::Call => true,
                    BranchOp::Beq => self.state.zero(),
                    BranchOp::Bne => !self.state.zero(),
                    BranchOp::Blt => self.state.negative() != self.state.overflow(),
                    BranchOp::Bge => self.state.negative() == self.state.overflow(),
                    BranchOp::Bgt => {
                        !self.state.zero() && self.state.negative() == self.state.overflow()
                    }
                    BranchOp::Ble => {
                        self.state.zero() || self.state.negative() != self.state.overflow()
                    }
                    BranchOp::Bult => !self.state.carry(),
                    BranchOp::Buge => self.state.carry(),
                    BranchOp::Bugt => self.state.carry() && !self.state.zero(),
                    BranchOp::Bule => !self.state.carry() || self.state.zero(),
                };
                if take {
                    if op == BranchOp::Call {
                        self.push(m, self.state.pc.get() as u32)
                    }
                    self.state.pc = target
                }
            }
            Instruction::System(op) => match op {
                SystemOp::Cla => self.logic(0),
                SystemOp::Cma => self.logic(!self.state.ac),
                SystemOp::Ret => self.state.pc = Address::from_low16(self.pop(m)),
                SystemOp::Iret => {
                    self.state.pc = Address::from_low16(self.pop(m));
                    self.state.psr = self.pop(m) & PSR_MASK
                }
                SystemOp::Hlt => self.state.halted = true,
                SystemOp::Inp => {
                    if let Some(v) = io.read_input(self.selected()) {
                        self.io.input_register = v;
                        self.state.ac = (self.state.ac & 0xffff_ff00) | v as u32
                    }
                }
                SystemOp::Out => io.write_output(self.selected(), self.state.ac as u8),
                SystemOp::Ski => {
                    if io.input_ready(self.selected()) {
                        self.skip()
                    }
                }
                SystemOp::Sko => {
                    if io.output_ready(self.selected()) {
                        self.skip()
                    }
                }
                SystemOp::Ion => self.set_flag(PSR_IEN, true),
                SystemOp::Iof => self.set_flag(PSR_IEN, false),
                SystemOp::Sio => self.io.serial_selected = true,
                SystemOp::Pio => self.io.serial_selected = false,
                SystemOp::Imk => self.io.interrupt_mask = self.state.ac as u8 & 0xf,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::Immediate16;
    fn at(m: &mut ArrayMemory, a: u16, i: Instruction) {
        m.write(Address::new(a).unwrap(), i.encode())
    }
    #[test]
    fn reset_and_halt() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        at(&mut m, 0x10, Instruction::System(SystemOp::Hlt));
        c.step(&mut m, &mut NullIoBus).unwrap();
        assert!(c.state.halted);
        assert_eq!(c.state.sp, Address::ZERO)
    }
    #[test]
    fn flags_and_signed_unsigned_branches() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        c.state.ac = 0x7fff_ffff;
        at(
            &mut m,
            0x10,
            Instruction::Immediate {
                op: ImmediateOp::Add,
                value: Immediate16::from_raw(1),
            },
        );
        c.step(&mut m, &mut NullIoBus).unwrap();
        assert!(c.state.negative());
        assert!(c.state.overflow());
        assert!(!c.state.carry());
    }
    #[test]
    fn every_branch_condition_uses_nzcv() {
        let cases = [
            (BranchOp::Jmp, 0, true),
            (BranchOp::Beq, PSR_Z, true),
            (BranchOp::Beq, 0, false),
            (BranchOp::Bne, 0, true),
            (BranchOp::Blt, PSR_N, true),
            (BranchOp::Blt, PSR_N | PSR_V, false),
            (BranchOp::Bge, PSR_N | PSR_V, true),
            (BranchOp::Bgt, 0, true),
            (BranchOp::Bgt, PSR_Z, false),
            (BranchOp::Ble, PSR_Z, true),
            (BranchOp::Bult, 0, true),
            (BranchOp::Buge, PSR_C, true),
            (BranchOp::Bugt, PSR_C, true),
            (BranchOp::Bugt, PSR_C | PSR_Z, false),
            (BranchOp::Bule, 0, true),
            (BranchOp::Bule, PSR_C | PSR_Z, true),
        ];
        for (op, psr, expected) in cases {
            let mut cpu = Cpu::default();
            let mut memory = ArrayMemory::default();
            cpu.state.psr = psr;
            at(
                &mut memory,
                0x10,
                Instruction::Branch {
                    op,
                    target: Address::new(0x2222).unwrap(),
                },
            );
            cpu.step(&mut memory, &mut NullIoBus).unwrap();
            assert_eq!(cpu.state.pc.get() == 0x2222, expected, "{op:?} {psr:#x}");
        }
    }
    #[test]
    fn subtraction_cmp_and_logical_flag_rules() {
        let mut cpu = Cpu::default();
        cpu.state.ac = 0x8000_0000;
        cpu.sub(1);
        assert_eq!(cpu.state.ac, 0x7fff_ffff);
        assert!(cpu.state.carry());
        assert!(cpu.state.overflow());

        cpu.state.ac = 0;
        cpu.sub(1);
        assert_eq!(cpu.state.ac, u32::MAX);
        assert!(!cpu.state.carry());
        assert!(!cpu.state.overflow());

        cpu.state.ac = 7;
        let result = cpu.subtraction_flags(cpu.state.ac, 7);
        assert_eq!(result, 0);
        assert_eq!(cpu.state.ac, 7);
        assert!(cpu.state.zero());
        assert!(cpu.state.carry());

        cpu.state.psr = PSR_C | PSR_V;
        cpu.logic(0);
        assert_eq!(cpu.state.psr, PSR_C | PSR_V | PSR_Z);
    }
    #[test]
    fn partial_load_isz_and_sp_relative_follow_flag_rules() {
        let mut cpu = Cpu::default();
        let mut memory = ArrayMemory::default();
        let mut io = NullIoBus;
        cpu.state.ac = 0x1234_5678;
        cpu.state.psr = PSR_C | PSR_V;
        at(
            &mut memory,
            0x10,
            Instruction::Immediate {
                op: ImmediateOp::Ldhi,
                value: Immediate16::from_raw(0x8000),
            },
        );
        cpu.step(&mut memory, &mut io).unwrap();
        assert_eq!(cpu.state.ac, 0x8000_5678);
        assert_eq!(cpu.state.psr, PSR_N | PSR_C | PSR_V);

        let cell = Address::new(0x3333).unwrap();
        memory.write(cell, u32::MAX);
        at(
            &mut memory,
            0x11,
            Instruction::Memory {
                op: MemoryOp::Isz,
                address: cell,
                indirect: false,
            },
        );
        cpu.step(&mut memory, &mut io).unwrap();
        assert_eq!(memory.read(cell), 0);
        assert_eq!(cpu.state.pc.get(), 0x13);
        assert_eq!(cpu.state.psr, PSR_N | PSR_C | PSR_V);

        cpu.state.pc = Address::new(0x20).unwrap();
        cpu.state.sp = Address::ZERO;
        memory.write(Address::new(0xffff).unwrap(), 42);
        at(
            &mut memory,
            0x20,
            Instruction::SpRelative {
                op: SpRelativeOp::Ldsp,
                offset: Immediate16::from_raw(0xffff),
            },
        );
        cpu.step(&mut memory, &mut io).unwrap();
        assert_eq!(cpu.state.ac, 42);
    }
    #[test]
    fn call_ret_uses_stack() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        c.state.sp = Address::new(0x8000).unwrap();
        at(
            &mut m,
            0x10,
            Instruction::Branch {
                op: BranchOp::Call,
                target: Address::new(0x20).unwrap(),
            },
        );
        at(&mut m, 0x20, Instruction::System(SystemOp::Ret));
        c.step(&mut m, &mut NullIoBus).unwrap();
        assert_eq!(c.state.sp.get(), 0x7fff);
        assert_eq!(m.read(c.state.sp), 0x11);
        c.step(&mut m, &mut NullIoBus).unwrap();
        assert_eq!(c.state.pc.get(), 0x11);
        assert_eq!(c.state.sp.get(), 0x8000)
    }
    #[test]
    fn interrupt_frame_and_iret() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        let mut io = DeterministicIoBus::default();
        c.state.sp = Address::new(0x9000).unwrap();
        c.state.psr = PSR_IEN | PSR_C;
        c.io.interrupt_mask = 2;
        io.push_input(IoKind::Parallel, 1);
        assert_eq!(c.step(&mut m, &mut io).unwrap(), StepOutcome::Interrupted);
        assert_eq!(c.state.pc, Address::ZERO);
        assert_eq!(c.state.sp.get(), 0x8ffe);
        assert_eq!(m.read(c.state.sp), 0x10);
        assert_eq!(m.read(c.state.sp.wrapping_add(1)), PSR_IEN | PSR_C);
        at(&mut m, 0, Instruction::System(SystemOp::Iret));
        io.read_input(IoKind::Parallel);
        c.step(&mut m, &mut io).unwrap();
        assert_eq!(c.state.pc.get(), 0x10);
        assert_eq!(c.state.sp.get(), 0x9000);
        assert_eq!(c.state.psr, PSR_IEN | PSR_C)
    }
    #[test]
    fn illegal_instruction_halts() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        m.write(Address::RESET, 0xe000_0000);
        assert!(c.step(&mut m, &mut NullIoBus).is_err());
        assert!(c.state.halted)
    }
}

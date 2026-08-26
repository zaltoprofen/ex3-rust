//! EX3 CPU、メモリ、周辺I/Oの実行モデル。
//!
//! CPUコアは端末やファイルへ直接アクセスせず、[`Memory`] と [`IoBus`] を
//! 通じて外部状態を操作する。この分離により、テストでは決定論的なI/Oを
//! 使用し、CLIでは別の実装へ差し替えられる。

use crate::{
    assembler::MemoryImage,
    isa::{
        decode, Address, DecodeError, ImmediateOp, Instruction, MemoryImmediateOp, N1Op, N2Op,
        NoOperandOp, Word, MEMORY_SIZE,
    },
    CompatibilityMode,
};
use std::{collections::VecDeque, error::Error, fmt};

/// CPUがアクセスするワード単位メモリ。
pub trait Memory {
    fn read(&self, addr: Address) -> Word;
    fn write(&mut self, addr: Address, value: Word);
}

/// EX3の4096ワードを固定長配列で保持する標準メモリ。
#[derive(Clone)]
pub struct ArrayMemory {
    words: [Word; MEMORY_SIZE],
}
impl Default for ArrayMemory {
    fn default() -> Self {
        Self {
            words: [0; MEMORY_SIZE],
        }
    }
}
impl ArrayMemory {
    pub fn from_image(image: &MemoryImage) -> Self {
        let mut m = Self::default();
        for cell in &image.cells {
            m.write(cell.address, cell.word);
        }
        m
    }
    pub fn from_cells(cells: &[(Address, Word)]) -> Self {
        let mut m = Self::default();
        for &(a, w) in cells {
            m.write(a, w);
        }
        m
    }
    pub fn words(&self) -> &[Word; MEMORY_SIZE] {
        &self.words
    }
}
impl Memory for ArrayMemory {
    fn read(&self, addr: Address) -> Word {
        self.words[addr.get() as usize]
    }
    fn write(&mut self, addr: Address, value: Word) {
        self.words[addr.get() as usize] = value
    }
}

/// I/O命令が選択できるポート種別。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoKind {
    Serial,
    Parallel,
}
/// CPUコアから周辺機器を分離するためのI/Oインターフェース。
pub trait IoBus {
    fn tick(&mut self) {}
    fn interrupt_pending(&self) -> bool {
        false
    }
    fn read_input(&mut self, kind: IoKind) -> Option<u8>;
    fn write_output(&mut self, kind: IoKind, value: u8);
    fn input_ready(&self, kind: IoKind) -> bool;
    fn output_ready(&self, kind: IoKind) -> bool;
}

/// 入力を持たず、出力を破棄する非対話実行用I/O。
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

/// キューへ投入した入力を即時に読み出せる、テスト向けI/O。
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
    pub fn push_input(&mut self, kind: IoKind, value: u8) {
        match kind {
            IoKind::Serial => self.serial_input.push_back(value),
            IoKind::Parallel => self.parallel_input.push_back(value),
        }
    }
    pub fn output(&self, kind: IoKind) -> &[u8] {
        match kind {
            IoKind::Serial => &self.serial_output,
            IoKind::Parallel => &self.parallel_output,
        }
    }
    pub fn request_interrupt(&mut self) {
        self.force_interrupt = true
    }
}
impl IoBus for DeterministicIoBus {
    fn tick(&mut self) {
        self.serial_output_ready = true;
        self.parallel_output_ready = true
    }
    fn interrupt_pending(&self) -> bool {
        self.force_interrupt
    }
    fn read_input(&mut self, kind: IoKind) -> Option<u8> {
        match kind {
            IoKind::Serial => self.serial_input.pop_front(),
            IoKind::Parallel => self.parallel_input.pop_front(),
        }
    }
    fn write_output(&mut self, kind: IoKind, value: u8) {
        match kind {
            IoKind::Serial => {
                self.serial_output.push(value);
                self.serial_output_ready = false
            }
            IoKind::Parallel => {
                self.parallel_output.push(value);
                self.parallel_output_ready = false
            }
        }
    }
    fn input_ready(&self, kind: IoKind) -> bool {
        match kind {
            IoKind::Serial => !self.serial_input.is_empty(),
            IoKind::Parallel => !self.parallel_input.is_empty(),
        }
    }
    fn output_ready(&self, kind: IoKind) -> bool {
        match kind {
            IoKind::Serial => self.serial_output_ready,
            IoKind::Parallel => self.parallel_output_ready,
        }
    }
}

/// 旧シミュレータのタイミングを再現するseed指定可能なI/O。
///
/// 入力は1..=50 tick、出力は1 tickでreadyになる。legacy CPUモードでは、
/// 旧実装と同様に割り込みが有効な間だけ周辺機器のtickが進む。
#[derive(Clone, Debug)]
pub struct LegacyIoBus {
    rng: u64,
    serial_pending: VecDeque<u8>,
    parallel_pending: VecDeque<u8>,
    serial_ready: Option<u8>,
    parallel_ready: Option<u8>,
    serial_delay: u8,
    parallel_delay: u8,
    serial_output: Vec<u8>,
    parallel_output: Vec<u8>,
    serial_output_ready: bool,
    parallel_output_ready: bool,
}
impl LegacyIoBus {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: seed,
            serial_pending: VecDeque::new(),
            parallel_pending: VecDeque::new(),
            serial_ready: None,
            parallel_ready: None,
            serial_delay: 1,
            parallel_delay: 1,
            serial_output: Vec::new(),
            parallel_output: Vec::new(),
            serial_output_ready: true,
            parallel_output_ready: true,
        }
    }
    pub fn push_input(&mut self, kind: IoKind, value: u8) {
        match kind {
            IoKind::Serial => self.serial_pending.push_back(value),
            IoKind::Parallel => self.parallel_pending.push_back(value),
        }
    }
    pub fn output(&self, kind: IoKind) -> &[u8] {
        match kind {
            IoKind::Serial => &self.serial_output,
            IoKind::Parallel => &self.parallel_output,
        }
    }
    fn next_delay(&mut self) -> u8 {
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
    fn tick(&mut self) {
        self.serial_output_ready = true;
        self.parallel_output_ready = true;
        if self.serial_ready.is_none() && !self.serial_pending.is_empty() {
            if self.serial_delay > 1 {
                self.serial_delay -= 1;
            } else {
                self.serial_ready = self.serial_pending.pop_front();
                self.serial_delay = self.next_delay();
            }
        }
        if self.parallel_ready.is_none() && !self.parallel_pending.is_empty() {
            if self.parallel_delay > 1 {
                self.parallel_delay -= 1;
            } else {
                self.parallel_ready = self.parallel_pending.pop_front();
                self.parallel_delay = self.next_delay();
            }
        }
    }
    fn read_input(&mut self, kind: IoKind) -> Option<u8> {
        match kind {
            IoKind::Serial => self.serial_ready.take(),
            IoKind::Parallel => self.parallel_ready.take(),
        }
    }
    fn write_output(&mut self, kind: IoKind, value: u8) {
        match kind {
            IoKind::Serial => {
                self.serial_output.push(value);
                self.serial_output_ready = false;
            }
            IoKind::Parallel => {
                self.parallel_output.push(value);
                self.parallel_output_ready = false;
            }
        }
    }
    fn input_ready(&self, kind: IoKind) -> bool {
        match kind {
            IoKind::Serial => self.serial_ready.is_some(),
            IoKind::Parallel => self.parallel_ready.is_some(),
        }
    }
    fn output_ready(&self, kind: IoKind) -> bool {
        match kind {
            IoKind::Serial => self.serial_output_ready,
            IoKind::Parallel => self.parallel_output_ready,
        }
    }
}

/// CPU内部にあるI/O制御レジスタ。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoState {
    pub serial_selected: bool,
    pub interrupt_enabled: bool,
    pub interrupt_mask: u8,
    pub input_register: u8,
}
/// デバッガやテストから観測できるCPUレジスタ群。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuState {
    pub pc: Address,
    pub ac: Word,
    pub e: bool,
    pub halted: bool,
    pub interrupt_pending: bool,
    pub ir: Word,
    pub executed_instructions: u64,
}
impl Default for CpuState {
    fn default() -> Self {
        Self {
            pc: Address::RESET,
            ac: 0,
            e: false,
            halted: false,
            interrupt_pending: false,
            ir: 0,
            executed_instructions: 0,
        }
    }
}

/// 1回の[`Cpu::step`]がどの経路を通ったかを表す。
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

/// EX3 CPU本体。メモリとI/Oは所有せず、実行時に借用する。
pub struct Cpu {
    state: CpuState,
    io: IoState,
    mode: CompatibilityMode,
}
impl Default for Cpu {
    fn default() -> Self {
        Self::new(CompatibilityMode::Strict)
    }
}
impl Cpu {
    pub fn new(mode: CompatibilityMode) -> Self {
        Self {
            state: CpuState::default(),
            io: IoState::default(),
            mode,
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

    pub fn step(
        &mut self,
        memory: &mut impl Memory,
        io: &mut impl IoBus,
    ) -> Result<StepOutcome, CpuError> {
        if self.state.halted {
            return Ok(StepOutcome::Halted);
        }

        // 旧実装ではIEN=0の間、CPUだけでなく周辺機器の時間も停止する。
        if self.mode == CompatibilityMode::Strict || self.io.interrupt_enabled {
            io.tick();
        }

        let masked_interrupt = (self.io.interrupt_mask & 0x8 != 0
            && io.input_ready(IoKind::Serial))
            || (self.io.interrupt_mask & 0x4 != 0 && io.output_ready(IoKind::Serial))
            || (self.io.interrupt_mask & 0x2 != 0 && io.input_ready(IoKind::Parallel))
            || (self.io.interrupt_mask & 1 != 0 && io.output_ready(IoKind::Parallel));
        self.state.interrupt_pending =
            self.io.interrupt_enabled && (masked_interrupt || io.interrupt_pending());

        // 割り込みentryは通常の命令fetchより先に行い、命令数に含めない。
        // M[0]に復帰PCを保存し、固定vector 0x001へ分岐する。
        if self.state.interrupt_pending {
            self.state.interrupt_pending = false;
            self.io.interrupt_enabled = false;
            memory.write(Address::ZERO, self.state.pc.get() as u32);
            self.state.pc = Address::new(1).expect("constant");
            return Ok(StepOutcome::Interrupted);
        }
        // 通常のfetch/decode/execute。PCはexecute前に次命令を指すため、
        // BSAが保存する値はそのまま正しいreturn addressになる。
        let pc_before = self.state.pc;
        self.state.ir = memory.read(pc_before);
        self.state.pc = self.state.pc.wrapping_add(1);
        self.state.executed_instructions = self.state.executed_instructions.wrapping_add(1);
        let instruction = decode(self.state.ir)?;
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
            if self.state.executed_instructions - start >= max_steps {
                return Err(CpuError::StepLimitExceeded);
            }
            self.step(memory, io)?;
        }
        Ok(self.state.executed_instructions - start)
    }

    fn ea(&self, memory: &impl Memory, operand: Address, indirect: bool) -> Address {
        if indirect {
            Address::from_low12(memory.read(operand))
        } else {
            operand
        }
    }
    fn add(&mut self, lhs: u32, rhs: u32) {
        let (result, carry) = lhs.overflowing_add(rhs);
        self.state.ac = result;
        self.state.e = if self.mode == CompatibilityMode::Legacy {
            // Scala版は符号付きLongの上位32 bitでEを判定していた。
            ((lhs as i32 as i64) + (rhs as i32 as i64)) >> 32 != 0
        } else {
            carry
        }
    }
    fn sub(&mut self, lhs: u32, rhs: u32) {
        let (result, borrow) = lhs.overflowing_sub(rhs);
        self.state.ac = result;
        self.state.e = if self.mode == CompatibilityMode::Legacy {
            // strictではE=borrow。legacyではScala版の上位bit判定を再現する。
            ((lhs as i32 as i64) - (rhs as i32 as i64)) >> 32 != 0
        } else {
            borrow
        }
    }
    fn skip(&mut self) {
        self.state.pc = self.state.pc.wrapping_add(1)
    }
    fn execute(&mut self, i: Instruction, m: &mut impl Memory, io: &mut impl IoBus) {
        match i {
            Instruction::N1 {
                op,
                operand,
                indirect,
            } => {
                let ea = self.ea(m, operand, indirect);
                let value = m.read(ea);
                match op {
                    N1Op::Add => self.add(self.state.ac, value),
                    N1Op::Sub => self.sub(self.state.ac, value),
                    N1Op::And => self.state.ac &= value,
                    N1Op::Or => self.state.ac |= value,
                    N1Op::Xor => self.state.ac ^= value,
                    N1Op::Lda => self.state.ac = value,
                    N1Op::Sta => m.write(ea, self.state.ac),
                    N1Op::Bun => self.state.pc = ea,
                    N1Op::Bsa => {
                        m.write(ea, self.state.pc.get() as u32);
                        self.state.pc = ea.wrapping_add(1)
                    }
                    N1Op::Jpa => {
                        if self.state.ac as i32 >= 0 {
                            self.state.pc = ea
                        }
                    }
                    N1Op::Jza => {
                        // 旧Scala版はJZAとJNAのdispatchだけが入れ替わっていた。
                        let yes = if self.mode == CompatibilityMode::Legacy {
                            (self.state.ac as i32) < 0
                        } else {
                            self.state.ac == 0
                        };
                        if yes {
                            self.state.pc = ea
                        }
                    }
                    N1Op::Jna => {
                        let yes = if self.mode == CompatibilityMode::Legacy {
                            self.state.ac == 0
                        } else {
                            (self.state.ac as i32) < 0
                        };
                        if yes {
                            self.state.pc = ea
                        }
                    }
                    N1Op::Jze => {
                        if !self.state.e {
                            self.state.pc = ea
                        }
                    }
                    N1Op::Isz => {
                        let result = value.wrapping_add(1);
                        m.write(ea, result);
                        if result == 0 {
                            self.skip()
                        }
                    }
                }
            }
            Instruction::N2 {
                op,
                operand1,
                operand2,
                indirect,
            } => {
                let lhs = m.read(operand1);
                let rhs = m.read(self.ea(m, operand2, indirect));
                match op {
                    N2Op::Add => self.add(lhs, rhs),
                    // EX3のN2 SUBはoperand2 - operand1の順序である。
                    N2Op::Sub => self.sub(rhs, lhs),
                    N2Op::And => self.state.ac = lhs & rhs,
                    N2Op::Or => self.state.ac = lhs | rhs,
                    N2Op::Xor => self.state.ac = lhs ^ rhs,
                    N2Op::Move => m.write(operand1, rhs),
                }
            }
            Instruction::Immediate { op, value } => {
                let v = value.as_word();
                match op {
                    ImmediateOp::Add => self.add(self.state.ac, v),
                    ImmediateOp::And => self.state.ac &= v,
                    ImmediateOp::Or => self.state.ac |= v,
                    ImmediateOp::Lda => self.state.ac = v,
                }
            }
            Instruction::MemoryImmediate {
                op,
                operand,
                value,
                indirect,
            } => {
                let ea = self.ea(m, operand, indirect);
                let memory_value = m.read(ea);
                let immediate = value.as_word();
                match op {
                    MemoryImmediateOp::Add => self.add(memory_value, immediate),
                    MemoryImmediateOp::And => self.state.ac = memory_value & immediate,
                    MemoryImmediateOp::Or => self.state.ac = memory_value | immediate,
                    MemoryImmediateOp::Sta => m.write(ea, immediate),
                }
            }
            Instruction::NoOperand(op) => match op {
                NoOperandOp::Cla => self.state.ac = 0,
                NoOperandOp::Cle => self.state.e = false,
                NoOperandOp::Cma => self.state.ac = !self.state.ac,
                NoOperandOp::Cme => self.state.e = !self.state.e,
                NoOperandOp::Cir => {
                    let old_e = self.state.e;
                    self.state.e = self.state.ac & 1 != 0;
                    // legacyのみScalaの算術右シフトを維持する。
                    self.state.ac = if self.mode == CompatibilityMode::Legacy {
                        ((self.state.ac as i32) >> 1) as u32
                    } else {
                        self.state.ac >> 1
                    } | if old_e { 0x80000000 } else { 0 }
                }
                NoOperandOp::Cil => {
                    let old_e = self.state.e;
                    self.state.e = self.state.ac & 0x80000000 != 0;
                    self.state.ac = (self.state.ac << 1) | u32::from(old_e)
                }
                NoOperandOp::Inc => self.state.ac = self.state.ac.wrapping_add(1),
                NoOperandOp::Spa => {
                    if self.state.ac as i32 >= 0 {
                        self.skip()
                    }
                }
                NoOperandOp::Sza => {
                    if self.state.ac == 0 {
                        self.skip()
                    }
                }
                NoOperandOp::Sna => {
                    if (self.state.ac as i32) < 0 {
                        self.skip()
                    }
                }
                NoOperandOp::Sze => {
                    if !self.state.e {
                        self.skip()
                    }
                }
                NoOperandOp::Inp => {
                    let kind = self.selected();
                    if let Some(v) = io.read_input(kind) {
                        self.io.input_register = v;
                        self.state.ac = if self.mode == CompatibilityMode::Legacy {
                            // Scala ByteからIntへの符号拡張を互換再現する。
                            (v as i8 as i32) as u32
                        } else {
                            (self.state.ac & 0xffffff00) | v as u32
                        }
                    }
                }
                NoOperandOp::Out => io.write_output(self.selected(), self.state.ac as u8),
                NoOperandOp::Ski => {
                    if io.input_ready(self.selected()) {
                        self.skip()
                    }
                }
                NoOperandOp::Sko => {
                    if io.output_ready(self.selected()) {
                        self.skip()
                    }
                }
                NoOperandOp::Ion => self.io.interrupt_enabled = true,
                NoOperandOp::Iof => self.io.interrupt_enabled = false,
                NoOperandOp::Sio => self.io.serial_selected = true,
                NoOperandOp::Pio => self.io.serial_selected = false,
                NoOperandOp::Imk => self.io.interrupt_mask = self.state.ac as u8 & 0xf,
                NoOperandOp::Hlt => self.state.halted = true,
            },
        }
    }
    fn selected(&self) -> IoKind {
        if self.io.serial_selected {
            IoKind::Serial
        } else {
            IoKind::Parallel
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(m: &mut ArrayMemory, a: u16, i: Instruction) {
        m.write(Address::new(a).unwrap(), i.encode())
    }
    #[test]
    fn reset_and_halt() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        let mut io = NullIoBus;
        at(&mut m, 0x10, Instruction::NoOperand(NoOperandOp::Hlt));
        c.step(&mut m, &mut io).unwrap();
        assert!(c.state.halted);
        assert_eq!(c.state.executed_instructions, 1)
    }
    #[test]
    fn arithmetic_and_carry() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        let mut io = NullIoBus;
        c.state.ac = u32::MAX;
        at(
            &mut m,
            0x10,
            Instruction::Immediate {
                op: ImmediateOp::Add,
                value: crate::isa::Immediate12::from_signed(1).unwrap(),
            },
        );
        c.step(&mut m, &mut io).unwrap();
        assert_eq!(c.state.ac, 0);
        assert!(c.state.e)
    }
    #[test]
    fn indirect_load_store() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        let mut io = NullIoBus;
        let p = Address::new(0x20).unwrap();
        m.write(p, 0x1021);
        m.write(Address::new(0x21).unwrap(), 42);
        at(
            &mut m,
            0x10,
            Instruction::N1 {
                op: N1Op::Lda,
                operand: p,
                indirect: true,
            },
        );
        c.step(&mut m, &mut io).unwrap();
        assert_eq!(c.state.ac, 42)
    }
    #[test]
    fn bsa_return() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        let mut io = NullIoBus;
        let sub = Address::new(0x20).unwrap();
        at(
            &mut m,
            0x10,
            Instruction::N1 {
                op: N1Op::Bsa,
                operand: sub,
                indirect: false,
            },
        );
        at(
            &mut m,
            0x21,
            Instruction::N1 {
                op: N1Op::Bun,
                operand: sub,
                indirect: true,
            },
        );
        c.step(&mut m, &mut io).unwrap();
        assert_eq!(m.read(sub), 0x11);
        c.step(&mut m, &mut io).unwrap();
        assert_eq!(c.state.pc.get(), 0x11)
    }
    #[test]
    fn rotate_modes() {
        let mut m = ArrayMemory::default();
        let mut io = NullIoBus;
        at(&mut m, 0x10, Instruction::NoOperand(NoOperandOp::Cir));
        let mut strict = Cpu::default();
        strict.state.ac = 0x80000000;
        strict.step(&mut m, &mut io).unwrap();
        assert_eq!(strict.state.ac, 0x40000000);
        let mut legacy = Cpu::new(CompatibilityMode::Legacy);
        legacy.state.ac = 0x80000000;
        legacy.step(&mut m, &mut io).unwrap();
        assert_eq!(legacy.state.ac, 0xc0000000)
    }
    #[test]
    fn io_and_interrupt() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        let mut io = DeterministicIoBus::default();
        io.push_input(IoKind::Parallel, 0xab);
        c.state.ac = 0x12340000;
        at(&mut m, 0x10, Instruction::NoOperand(NoOperandOp::Inp));
        c.step(&mut m, &mut io).unwrap();
        assert_eq!(c.state.ac, 0x123400ab);
        c.io.interrupt_enabled = true;
        c.io.interrupt_mask = 2;
        io.push_input(IoKind::Parallel, 1);
        assert_eq!(c.step(&mut m, &mut io).unwrap(), StepOutcome::Interrupted);
        assert_eq!(m.read(Address::ZERO), 0x11);
        assert_eq!(c.state.pc.get(), 1)
    }
    #[test]
    fn branch_legacy_swap() {
        let mut m = ArrayMemory::default();
        let mut io = NullIoBus;
        at(
            &mut m,
            0x10,
            Instruction::N1 {
                op: N1Op::Jza,
                operand: Address::new(0x20).unwrap(),
                indirect: false,
            },
        );
        let mut strict = Cpu::default();
        strict.step(&mut m, &mut io).unwrap();
        assert_eq!(strict.state.pc.get(), 0x20);
        let mut legacy = Cpu::new(CompatibilityMode::Legacy);
        legacy.step(&mut m, &mut io).unwrap();
        assert_eq!(legacy.state.pc.get(), 0x11)
    }
}

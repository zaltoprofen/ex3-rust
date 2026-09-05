//! CPUコアから独立したブレークポイント管理と状態表示。

use crate::{
    emulator::{Cpu, CpuError, IoBus, Memory, StepOutcome},
    isa::{decode, Address},
};
use std::collections::BTreeSet;

/// 連続実行が停止した理由。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStop {
    Halted,
    Breakpoint(Address),
    StepLimit,
}
/// 命令fetchアドレスに対するブレークポイント集合。
#[derive(Default)]
pub struct Debugger {
    breakpoints: BTreeSet<Address>,
}
impl Debugger {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn breakpoints(&self) -> &BTreeSet<Address> {
        &self.breakpoints
    }
    pub fn toggle_breakpoint(&mut self, address: Address) -> bool {
        // 戻り値は操作後にbreakpointが存在するかどうか。
        if self.breakpoints.remove(&address) {
            false
        } else {
            self.breakpoints.insert(address);
            true
        }
    }
    pub fn add_breakpoint(&mut self, address: Address) {
        self.breakpoints.insert(address);
    }
    pub fn remove_breakpoint(&mut self, address: Address) {
        self.breakpoints.remove(&address);
    }
    pub fn step(
        &self,
        cpu: &mut Cpu,
        memory: &mut impl Memory,
        io: &mut impl IoBus,
    ) -> Result<StepOutcome, CpuError> {
        cpu.step(memory, io)
    }
    pub fn run(
        &self,
        cpu: &mut Cpu,
        memory: &mut impl Memory,
        io: &mut impl IoBus,
        max_steps: u64,
    ) -> Result<RunStop, CpuError> {
        let mut steps = 0;
        loop {
            if cpu.state().halted {
                return Ok(RunStop::Halted);
            }
            if self.breakpoints.contains(&cpu.state().pc) {
                return Ok(RunStop::Breakpoint(cpu.state().pc));
            }
            if steps >= max_steps {
                return Ok(RunStop::StepLimit);
            }
            // 割り込みentryはCPUの命令数を増やさないため、step呼び出し回数
            // ではなく実際にfetchした命令数を上限に数える。
            let before = cpu.state().executed_instructions;
            cpu.step(memory, io)?;
            steps += cpu.state().executed_instructions - before;
        }
    }
}

/// レジスタとI/O制御状態を1行のtrace向け文字列にする。
pub fn format_registers(cpu: &Cpu) -> String {
    let s = cpu.state();
    let io = cpu.io_state();
    format!(
        "PC={:04x} SP={:04x} AC={:08x} PSR={:02x} [I={} N={} Z={} C={} V={}] IR={:08x} count={} halted={} IRQ={} IMSK={:x} port={}",
        s.pc.get(),
        s.sp.get(),
        s.ac,
        s.psr,
        u8::from(s.interrupt_enabled()),
        u8::from(s.negative()),
        u8::from(s.zero()),
        u8::from(s.carry()),
        u8::from(s.overflow()),
        s.ir,
        s.executed_instructions,
        s.halted,
        s.interrupt_pending,
        io.interrupt_mask,
        if io.serial_selected {
            "serial"
        } else {
            "parallel"
        }
    )
}
/// 現在のPCが指すワードを逆アセンブルする。
/// 不正なワードでもpanicせず、生の値と`<invalid>`を表示する。
pub fn format_current(cpu: &Cpu, memory: &impl Memory) -> String {
    let pc = cpu.state().pc;
    let word = memory.read(pc);
    match decode(word) {
        Ok(i) => format!("@{:04x} {:08x}  {i}", pc.get(), word),
        Err(_) => format!("@{:04x} {:08x}  <invalid>", pc.get(), word),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        emulator::{ArrayMemory, NullIoBus},
        isa::{Instruction, SystemOp},
    };
    #[test]
    fn breakpoint_stops_before_fetch() {
        let mut c = Cpu::default();
        let mut m = ArrayMemory::default();
        m.write(Address::RESET, Instruction::System(SystemOp::Hlt).encode());
        let mut io = NullIoBus;
        let mut d = Debugger::new();
        d.add_breakpoint(Address::RESET);
        assert_eq!(
            d.run(&mut c, &mut m, &mut io, 10).unwrap(),
            RunStop::Breakpoint(Address::RESET)
        );
        assert_eq!(c.state().executed_instructions, 0);
    }
}

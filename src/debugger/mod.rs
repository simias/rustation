use cpu::Cpu;

/// Trait defining the debugger interface
pub trait Debugger {
    /// Signal a "break" which will put the emulator in debug mode at
    /// the next instruction
    fn trigger_break(&mut self);

    /// Called by the CPU when it's about to execute a new
    /// instruction. This function is called before *all* CPU
    /// instructions so it needs to be as fast as possible.
    fn pc_change(&mut self, cpu: &mut Cpu);

    /// Called by the CPU when it's about to load a value from memory.
    fn memory_read(&mut self, cpu: &mut Cpu, addr: u32);

    /// Called by the CPU when it's about to write a value to memory.
    fn memory_write(&mut self, cpu: &mut Cpu, addr: u32);
}


/// Dummy debugger implementation that does nothing. Can be used when
/// debugging is disabled.
impl Debugger for () {
    fn trigger_break(&mut self) {
    }

    fn pc_change(&mut self, _: &mut Cpu) {
    }

    fn memory_read(&mut self, _: &mut Cpu, _: u32) {
    }

    fn memory_write(&mut self, _: &mut Cpu, _: u32) {
    }
}

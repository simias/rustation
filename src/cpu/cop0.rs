use interrupt::InterruptState;

/// Coprocessor 0: System control
pub struct Cop0 {
    /// Cop0 register 12: Status register
    sr: u32,
    /// Cop0 register 13: Cause register
    cause:  u32,
    /// Cop0 register 14: Exception PC
    epc: u32,
}

impl Cop0 {

    pub fn new() -> Cop0 {
        Cop0 {
            sr:    0,
            cause: 0,
            epc:   0,
        }
    }

    pub fn sr(&self) -> u32 {
        self.sr
    }

    pub fn set_sr(&mut self, sr: u32) {
        self.sr = sr;
    }

    /// Retreive the value of the CAUSE register. We need the
    /// InterruptState because bit 10 is wired to the current external
    /// interrupt (no latch, ack'ing the interrupt in the external
    /// controller resets the value in this register) .
    pub fn cause(&self, irq_state: InterruptState) -> u32 {
        self.cause | ((irq_state.active() as u32) << 10)
    }

    pub fn epc(&self) -> u32 {
        self.epc
    }

    pub fn cache_isolated(&self) -> bool {
        self.sr & 0x10000 != 0
    }

    /// Update SR, CAUSE and EPC when an exception is
    /// triggered. Returns the address of the exception handler.
    pub fn enter_exception(&mut self,
                           cause: Exception,
                           pc: u32,
                           in_delay_slot: bool) -> u32 {
        // Shift bits [5:0] of `SR` two places to the left. Those bits
        // are three pairs of Interrupt Enable/User Mode bits behaving
        // like a stack 3 entries deep. Entering an exception pushes a
        // pair of zeroes by left shifting the stack which disables
        // interrupts and puts the CPU in kernel mode. The original
        // third entry is discarded (it's up to the kernel to handle
        // more than two recursive exception levels).
        let mode = self.sr & 0x3f;

        self.sr &= !0x3f;
        self.sr |= (mode << 2) & 0x3f;

        // Update `CAUSE` register with the exception code (bits
        // [6:2])
        self.cause &= !0x7c;
        self.cause |= (cause as u32) << 2;

        if in_delay_slot {
            // When an exception occurs in a delay slot `EPC` points
            // to the branch instruction and bit 31 of `CAUSE` is set.
            self.epc = pc.wrapping_sub(4);
            self.cause |= 1 << 31;
        } else {
            self.epc = pc;
            self.cause &= !(1 << 31);
        }

        // The address of the exception handler address depends on the
        // value of the BEV bit in SR
        match self.sr & (1 << 22) != 0 {
            true  => 0xbfc00180,
            false => 0x80000080,
        }
    }

    /// The counterpart to "enter_exception": shift SR's mode back
    /// into place. Doesn't touch CAUSE or EPC however.
    pub fn return_from_exception(&mut self) {
        let mode = self.sr & 0x3f;

        // Bits [5:4] (the third and last mode in the stack) remains
        // untouched and is therefore duplicated in the 2nd entry.
        self.sr &= !0xf;
        self.sr |= mode >> 2;
    }

    /// Return true if the interrupts are enabled on the CPU (SR
    /// "Current Interrupt Enable" bit is set)
    fn irq_enabled(&self) -> bool {
        self.sr & 1 != 0
    }

    /// Return true if an interrupt (either software or hardware) is
    /// pending
    pub fn irq_active(&self, irq_state: InterruptState) -> bool {
        let cause = self.cause(irq_state);

        // Bits [8:9] of CAUSE contain the two software interrupts
        // (that the software can use by writing to the CAUSE
        // register) while bit 10 is wired to the external interrupt's
        // state. They can be individually masked using SR's bits
        // [8:10]
        let pending = (cause & self.sr) & 0x700;

        // Finally bit 0 of SR can be used to mask interrupts globally
        // on the CPU so we must check that.
        self.irq_enabled() && pending != 0
    }
}

/// Exception types (as stored in the `CAUSE` register)
#[derive(Clone,Copy)]
pub enum Exception {
    /// Interrupt Request
    Interrupt = 0x0,
    /// Address error on load
    LoadAddressError = 0x4,
    /// Address error on store
    StoreAddressError = 0x5,
    /// System call (caused by the SYSCALL opcode)
    SysCall = 0x8,
    /// Breakpoint (caused by the BREAK opcode)
    Break = 0x9,
    /// CPU encountered an unknown instruction
    IllegalInstruction = 0xa,
    /// Unsupported coprocessor operation
    CoprocessorError = 0xb,
    /// Arithmetic overflow
    Overflow = 0xc,
}

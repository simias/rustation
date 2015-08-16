//! XXX: All of this is very much *not* overflow tolerant. I'm just
//! hoping that a u64 will work for the time being but with the fixed
//! point representations shifting things around it's probably going
//! to be a problem sooner or later.

/// Struct keeping track of the various peripheral's emulation advancement.
pub struct TimeKeeper {
    /// Counter keeping track of the current date. Unit is a period of
    /// the CPU clock at 33.8685MHz (~29.5ns)
    now: Cycles,
    /// Time sheets for keeping track of the various peripherals
    timesheets: [TimeSheet; 4],
}

impl TimeKeeper {
    pub fn new() -> TimeKeeper {
        TimeKeeper {
            now: 0,
            timesheets: [TimeSheet::new(); 4],
        }
    }

    pub fn tick(&mut self, cycles: Cycles) {
        self.now += cycles;
    }

    /// Synchronize the timesheet for the given peripheral and return
    /// the elapsed time synce the last sync.
    pub fn sync(&mut self, who: Peripheral) -> Cycles {
        self.timesheets[who as usize].sync(self.now)
    }

    pub fn set_next_sync_delta(&mut self, who: Peripheral, delta: Cycles) {
        self.timesheets[who as usize].set_next_sync(self.now + delta)
    }

    pub fn needs_sync(&self, who: Peripheral) -> bool {
        self.timesheets[who as usize].needs_sync(self.now)
    }
}

#[derive(Clone,Copy)]
/// Struct used to keep track of individual peripherals
struct TimeSheet {
    /// Date of the last synchronization
    last_sync: Cycles,
    /// Date of the next "forced" sync
    next_sync: Cycles,
}

impl TimeSheet {

    fn new() -> TimeSheet {
        TimeSheet {
            last_sync: 0,
            // We force a synchronization at startup to initialize
            // everything
            next_sync: 0,
        }
    }

    /// Forward the time sheet to the current date and return the
    /// elapsed time since the last sync.
    fn sync(&mut self, now: Cycles) -> Cycles {
        let delta = now - self.last_sync;

        self.last_sync = now;

        delta
    }

    fn set_next_sync(&mut self, when: Cycles) {
        self.next_sync = when;
    }

    fn needs_sync(&self, now: Cycles) -> bool {
        self.next_sync <= now
    }
}

/// List of all peripherals requiring a TimeSheet. The value of the
/// enum is used as the index in the table
#[derive(Clone,Copy,Debug)]
pub enum Peripheral {
    /// Graphics Processing Unit
    Gpu,
    Timer0,
    Timer1,
    Timer2,
}

/// 64bit timestamps will wrap in roughly 17271 years with a CPU clock
/// at 33.8685MHz so it should be plenty enough.
pub type Cycles = u64;

/// Fixed point representation of a cycle counter used to store
/// non-integer cycle counts. Required because the CPU and GPU clocks
/// have a non-integer ratio.
#[derive(Clone, Copy)]
pub struct FracCycles(Cycles);

impl FracCycles {
    pub fn from_fp(val: Cycles) -> FracCycles {
        FracCycles(val)
    }

    pub fn from_f32(val: f32) -> FracCycles {
        let precision = (1u32 << FracCycles::frac_bits()) as f32;

        FracCycles((val * precision) as Cycles)
    }

    pub fn from_cycles(val: Cycles) -> FracCycles {
        FracCycles(val << FracCycles::frac_bits())
    }

    /// Return the raw fixed point value
    pub fn get_fp(self) -> Cycles {
        self.0
    }

    /// Return the number of fractional bits in the fixed point
    /// representation
    pub fn frac_bits() -> Cycles {
        16
    }

    pub fn add(self, val: FracCycles) -> FracCycles {
        FracCycles(self.get_fp() + val.get_fp())
    }

    pub fn multiply(self, mul: FracCycles) -> FracCycles {
        let v = self.get_fp() * mul.get_fp();

        // The shift amount is doubled during the multiplication so we
        // have to shift it back to its normal position.
        FracCycles(v >> FracCycles::frac_bits())
    }

    pub fn divide(self, denominator: FracCycles) -> FracCycles {
        // In order not to lose precision we must shift the numerator
        // once more *before* the division. Otherwise the division of
        // the two shifted value would only give us the integer part
        // of the result.
        let numerator = self.get_fp() << FracCycles::frac_bits();

        FracCycles(numerator / denominator.get_fp())
    }
}

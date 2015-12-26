//! XXX: All of this is very much *not* overflow tolerant. I'm just
//! hoping that a u64 will work for the time being but with the fixed
//! point representations shifting things around it's probably going
//! to be a problem sooner or later.

use std::{fmt};

/// List of all peripherals requiring a TimeSheet. The value of the
/// enum is used as the index in the timesheet table
#[derive(Clone, Copy, Debug)]
pub enum Peripheral {
    /// Graphics Processing Unit
    Gpu,
    /// Timers
    Timer0,
    Timer1,
    Timer2,
    /// Gamepad/Memory Card controller
    PadMemCard,
    /// CD-ROM controller
    CdRom,
}


/// Struct keeping track of the various peripheral's emulation advancement.
pub struct TimeKeeper {
    /// Counter keeping track of the current date. Unit is a period of
    /// the CPU clock at 33.8685MHz (~29.5ns)
    now: Cycles,
    /// Next time a peripheral needs an update
    next_sync: Cycles,
    /// Time sheets for keeping track of the various peripherals
    timesheets: [TimeSheet; 6],
}

impl TimeKeeper {
    pub fn new() -> TimeKeeper {
        TimeKeeper {
            now: 0,
            // Force a sync at the start to initialize evrything
            next_sync: 0,
            timesheets: [TimeSheet::new(); 6],
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
        let date = self.now + delta;

        self.timesheets[who as usize].set_next_sync(date);

        if date < self.next_sync {
            self.next_sync = date;
        }
    }

    /// Set next sync *only* if it's closer than what's already
    /// configured.
    pub fn set_next_sync_delta_if_sooner(&mut self,
                                         who: Peripheral,
                                         delta: Cycles) {
        let date = self.now + delta;

        let timesheet = &mut self.timesheets[who as usize];

        let next_sync = timesheet.next_sync();

        if next_sync > date {
            timesheet.set_next_sync(date);
        }
    }

    /// Called by a peripheral when there's no asynchronous event
    /// scheduled.
    pub fn no_sync_needed(&mut self, who: Peripheral) {
        // Instead of disabling the sync completely we can just use a
        // distant date. Peripheral's syncs should be idempotent
        // anyway.
        self.timesheets[who as usize].set_next_sync(Cycles::max_value());
    }

    pub fn sync_pending(&self) -> bool{
        self.next_sync <= self.now
    }

    pub fn needs_sync(&self, who: Peripheral) -> bool {
        self.timesheets[who as usize].needs_sync(self.now)
    }

    pub fn update_sync_pending(&mut self) {
        self.next_sync =
            self.timesheets.iter().map(|t| t.next_sync).min().unwrap();
    }
}

impl fmt::Display for TimeKeeper {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let now = self.now;
        let cpu_freq = ::cpu::CPU_FREQ_HZ as Cycles;

        let seconds = now / cpu_freq;
        let rem = now % cpu_freq;

        write!(fmt, "{}s+{:08}", seconds, rem)
    }
}


#[derive(Clone, Copy, Debug)]
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

    fn next_sync(&self) -> Cycles {
        self.next_sync
    }

    fn set_next_sync(&mut self, when: Cycles) {
        self.next_sync = when;
    }

    fn needs_sync(&self, now: Cycles) -> bool {
        self.next_sync <= now
    }
}

/// 64bit timestamps will wrap in roughly 17271 years with a CPU clock
/// at 33.8685MHz so it should be plenty enough.
pub type Cycles = u64;

/// Fixed point representation of a cycle counter used to store
/// non-integer cycle counts. Required because the CPU and GPU clocks
/// have a non-integer ratio.
#[derive(Clone, Copy, Debug)]
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

    pub fn ceil(self) -> Cycles {
        let shift = FracCycles::frac_bits();

        let align = (1 << shift) - 1;

        (self.0 + align) >> shift
    }
}

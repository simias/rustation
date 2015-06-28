/// Struct keeping track of the various peripheral's emulation advancement.
pub struct TimeKeeper {
    /// Counter keeping track of the current date. Unit is a period of
    /// the CPU clock at 33.8685MHz (~29.5ns)
    now: Cycles,
    /// Time sheets for keeping track of the various peripherals
    timesheets: [TimeSheet; 1],
}

impl TimeKeeper {
    pub fn new() -> TimeKeeper {
        TimeKeeper {
            now: 0,
            timesheets: [TimeSheet::new(); 1],
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
pub enum Peripheral {
    /// Graphics Processing Unit
    Gpu = 0,
}

/// 64bit timestamps will wrap in roughly 17271 years with a CPU clock
/// at 33.8685MHz so it should be plenty enough.
pub type Cycles = u64;

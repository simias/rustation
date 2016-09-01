use timekeeper::TimeKeeper;
use interrupt::InterruptState;

/// State shared between various modules
#[derive(RustcDecodable, RustcEncodable)]
pub struct SharedState {
    tk: TimeKeeper,
    irq_state: InterruptState,
    counters: Counters,
}

impl SharedState {
    pub fn new() -> SharedState {
        SharedState {
            tk: TimeKeeper::new(),
            irq_state: InterruptState::new(),
            counters: Counters::new(),
        }
    }

    pub fn tk(&mut self) -> &mut TimeKeeper {
        &mut self.tk
    }

    pub fn irq_state(&mut self) -> &InterruptState {
        &self.irq_state
    }

    pub fn irq_state_mut(&mut self) -> &mut InterruptState {
        &mut self.irq_state
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    pub fn counters_mut(&mut self) -> &mut Counters {
        &mut self.counters
    }
}

/// Struct holding various counters for debugging and profiling
#[derive(RustcDecodable, RustcEncodable)]
pub struct Counters {
    /// Increments at each frame drawn to the display. It will wrap in
    /// a little more than 2 years at 60Hz.
    pub frame: Counter,
    /// Increments when the game sets the display area coordinates, it
    /// usually means that the game wants to display a new frame and
    /// can be used to compute the internal FPS.
    pub framebuffer_swap: Counter,
    /// Incremented when the CPU is preempted by an external
    /// interrupt.
    pub cpu_interrupt: Counter,
}

impl Counters {
    pub fn new() -> Counters {
        Counters {
            frame: Counter(0),
            framebuffer_swap: Counter(0),
            cpu_interrupt: Counter(0),
        }
    }
}

/// Simple wrapper around a `u32` to implement a counter interface
#[derive(Copy, Clone, RustcEncodable, RustcDecodable)]
pub struct Counter(u32);

impl Counter {
    pub fn reset(&mut self) {
        self.0 = 0
    }

    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1)
    }

    pub fn get(&self) -> u32 {
        self.0
    }
}

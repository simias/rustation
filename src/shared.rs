use timekeeper::TimeKeeper;
use interrupt::InterruptState;
use debugger::Debugger;

/// State shared between various modules
pub struct SharedState {
    tk: TimeKeeper,
    debugger: Debugger,
    irq_state: InterruptState,
    frame: u32,
}

impl SharedState {
    pub fn new() -> SharedState {
        SharedState {
            tk: TimeKeeper::new(),
            debugger: Debugger::new(),
            irq_state: InterruptState::new(),
            frame: 0,
        }
    }

    pub fn tk(&mut self) -> &mut TimeKeeper {
        &mut self.tk
    }

    pub fn debugger(&mut self) -> &mut Debugger {
        &mut self.debugger
    }

    pub fn irq_state(&mut self) -> &mut InterruptState {
        &mut self.irq_state
    }

    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn new_frame(&mut self) {
        // It will wrap in a little more than 2 years at 60Hz
        self.frame = self.frame.wrapping_add(1);
    }

    /// Cleanup the shared state except for the debugger
    pub fn reset(&mut self) {
        self.tk = TimeKeeper::new();
        self.irq_state = InterruptState::new();
        self.frame = 0;
    }
}

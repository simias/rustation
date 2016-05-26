use timekeeper::TimeKeeper;
use interrupt::InterruptState;

/// State shared between various modules
#[derive(RustcDecodable, RustcEncodable)]
pub struct SharedState {
    tk: TimeKeeper,
    irq_state: InterruptState,
    frame: u32,
}

impl SharedState {
    pub fn new() -> SharedState {
        SharedState {
            tk: TimeKeeper::new(),
            irq_state: InterruptState::new(),
            frame: 0,
        }
    }

    pub fn tk(&mut self) -> &mut TimeKeeper {
        &mut self.tk
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
}

use timekeeper::{TimeKeeper, Cycles, FracCycles, Peripheral};
use gpu::Gpu;
use super::{Addressable, AccessWidth};
use super::interrupts::InterruptState;

pub struct Timers {
    /// The three timers. They're mostly identical except that they
    /// can each select a unique clock source besides the regular
    /// system clock:
    ///
    /// * Timer 0: GPU pixel clock
    /// * Timer 1: GPU horizontal blanking
    /// * Timer 2: System clock / 8
    /// The also each have a
    timers: [Timer; 3],
}

impl Timers {
    pub fn new() -> Timers {
        Timers {
            timers: [Timer::new(Peripheral::Timer0),
                     Timer::new(Peripheral::Timer1),
                     Timer::new(Peripheral::Timer2),
                     ],
        }
    }

    pub fn store<T: Addressable>(&mut self,
                                 tk: &mut TimeKeeper,
                                 irq_state: &mut InterruptState,
                                 gpu: &mut Gpu,
                                 offset: u32,
                                 val: T) {

        if T::width() != AccessWidth::Word &&
           T::width() != AccessWidth::Halfword {
            panic!("Unhandled {:?} timer store", T::width());
        }

        let val = val.as_u16();

        let instance = offset >> 4;

        let timer = &mut self.timers[instance as usize];

        timer.sync(tk, irq_state);

        match offset & 0xf {
            0 => timer.set_counter(val),
            4 => timer.set_mode(val),
            8 => timer.set_target(val),
            n => panic!("Unhandled timer register {}", n),
        }

        if timer.needs_gpu() {
            gpu.sync(tk, irq_state);
        }

        timer.reconfigure(gpu);
    }

    pub fn load<T: Addressable>(&mut self,
                                tk: &mut TimeKeeper,
                                irq_state: &mut InterruptState,
                                offset: u32) -> T {

        if T::width() != AccessWidth::Word &&
           T::width() != AccessWidth::Halfword {
            panic!("Unhandled {:?} timer load", T::width());
        }

        let instance = offset >> 4;

        let timer = &mut self.timers[instance as usize];

        timer.sync(tk, irq_state);

        let val = match offset & 0xf {
            0 => timer.counter(),
            4 => timer.mode(),
            8 => timer.target(),
            n => panic!("Unhandled timer register {}", n),
        };

        T::from_u32(val as u32)
    }

    /// Called by the GPU when the video timings change since it can
    /// affect the timers that use them.
    pub fn video_timings_changed(&mut self,
                                 tk: &mut TimeKeeper,
                                 irq_state: &mut InterruptState,
                                 gpu: &Gpu) {

        for t in &mut self.timers {
            if t.needs_gpu() {
                t.sync(tk, irq_state);
                t.reconfigure(gpu);
            }
        }
    }
}

struct Timer {
    /// Timer instance (Timer0, 1 or 2)
    instance: Peripheral,
    /// Counter value
    counter: u16,
    /// Counter target
    target: u16,
    /// If true do not synchronize the timer with an external signal
    free_run: bool,
    /// The synchronization mode when `free_run` is false. Each one of
    /// the three timers interprets this mode differently.
    sync: Sync,
    /// If true the counter is reset when it reaches the `target`
    /// value. Otherwise let it count all the way to `0xffff` and wrap
    /// around.
    target_wrap: bool,
    /// Raise interrupt when the counter reaches the `target`
    target_irq: bool,
    /// Raise interrupt when the counter passes 0xffff and wraps
    /// around
    wrap_irq: bool,
    /// If true the interrupt is automatically cleared and will
    /// re-trigger when one of the interrupt conditions occurs again
    repeat_irq: bool,
    /// XXX Not sure what this bit does. Does it simply invert the IRQ
    /// signal each time an interrupt condition is encountered?
    pulse_irq: bool,
    /// Clock source (2bits). Each timer can either use the CPU
    /// SysClock or an alternative clock source.
    clock_source: ClockSource,
    /// XXX Not sure what this does exactly
    request_interrupt: bool,
    /// True if the target has been reached since the last read
    target_reached: bool,
    /// True if the counter reached 0xffff and overflowed since the
    /// last read
    overflow_reached: bool,
    /// Period of a counter tick. Stored as a fractional cycle count
    /// since the GPU can be used as a source.
    period: FracCycles,
    /// Current position within a period of a counter tick.
    phase: FracCycles,
}

impl Timer {
    fn new(instance: Peripheral) -> Timer {
        Timer {
            instance: instance,
            counter: 0,
            target: 0,
            free_run: false,
            sync: Sync::from_field(0),
            target_wrap: false,
            target_irq: false,
            wrap_irq: false,
            repeat_irq: false,
            pulse_irq: false,
            clock_source: ClockSource::from_field(0),
            request_interrupt: false,
            target_reached: false,
            overflow_reached: false,
            period: FracCycles::from_cycles(1),
            phase: FracCycles::from_cycles(0),
        }
    }

    /// Recomputes the entire timer's internal state. Must be called
    /// when the timer's config changes *or* when the timer relies on
    /// the GPU's video timings and those timings change.
    ///
    /// If the GPU is needed for the timings it must be synchronized
    /// before this function is called.
    fn reconfigure(&mut self, gpu: &Gpu) {

        match self.clock_source.clock(self.instance) {
            Clock::SysClock => {
                self.period = FracCycles::from_cycles(1);
                self.phase  = FracCycles::from_cycles(0);
            },
            Clock::SysClockDiv8 => {
                self.period = FracCycles::from_cycles(8);
                // XXX When does the divider get reset exactly?
                // Maybe it's running continuously?
                self.phase  = FracCycles::from_cycles(0);
            },
            Clock::GpuDotClock => {
                self.period = gpu.dotclock_period();
                self.phase  = gpu.dotclock_phase();
            },
            Clock::GpuHSync => {
                self.period = gpu.hsync_period();
                self.phase  = gpu.hsync_phase();
            }
        }
    }

    /// Synchronize this timer.
    /// XXX Handle interrupts
    fn sync(&mut self,
            tk: &mut TimeKeeper,
            _: &mut InterruptState) {
        let delta = tk.sync(self.instance);

        let delta_frac = FracCycles::from_cycles(delta);

        let ticks = delta_frac.add(self.phase);

        let mut count = ticks.get_fp() / self.period.get_fp();
        let phase     = ticks.get_fp() % self.period.get_fp();

        // Store the new phase
        self.phase = FracCycles::from_fp(phase);

        let target = match self.target_wrap {
            // We wrap *after* the target is reached, so we need to
            // add 1 to it for our modulo to work correctly later.
            true  => (self.target as Cycles) + 1,
            false => 0x10000,
        };

        count += self.counter as Cycles;

        if count >= target {
            count %= target;

            // XXX I'm not sure if those flags are set when the
            // target/0xffff are reached or at the beginning of the
            // next tick.
            self.target_reached = true;

            // XXX check that this flag is set even when we're using
            // `target_wrap` and target is set to 0xffff or if it's
            // just in "targetless" mode.
            if target == 0x10000 {
                self.overflow_reached = true;
            }
        }

        self.counter = count as u16;
    }

    /// Return true if the timer relies on the GPU for the clock
    /// source or synchronization
    pub fn needs_gpu(&self) -> bool {
        if !self.free_run {
            panic!("Sync mode not supported!");
        }

        self.clock_source.clock(self.instance).needs_gpu()
    }

    fn mode(&mut self) -> u16 {
        let mut r = 0u16;

        r |= self.free_run as u16;
        r |= (self.sync as u16) << 1;
        r |= (self.target_wrap as u16) << 3;
        r |= (self.wrap_irq as u16) << 5;
        r |= (self.repeat_irq as u16) << 6;
        r |= (self.pulse_irq as u16) << 7;
        r |= (self.clock_source.0 as u16) << 8;
        r |= (self.request_interrupt as u16) << 10;
        r |= (self.target_reached as u16) << 11;
        r |= (self.overflow_reached as u16) << 12;

        // Reading mode resets those flags
        self.target_reached   = false;
        self.overflow_reached = false;

        r
    }

    /// Set the value of the mode register
    fn set_mode(&mut self, val: u16) {
        self.free_run = (val & 1) == 0;
        self.sync = Sync::from_field((val >> 1) & 3);
        self.target_wrap = (val >> 3) & 1 != 0;
        self.target_irq = (val >> 4) & 1 != 0;
        self.wrap_irq = (val >> 5) & 1 != 0;
        self.repeat_irq = (val >> 6) & 1 != 0;
        self.pulse_irq = (val >> 7) & 1 != 0;
        self.clock_source = ClockSource::from_field((val >> 8) & 3);
        // Polarity of this flag appears to be reversed. I'm still not
        // sure what it does though...
        self.request_interrupt = (val >> 10) & 1 != 0;

        // Writing to mode resets the counter
        self.counter = 0;

        if self.request_interrupt {
            panic!("Unsupported timer IRQ request");
        }

        if !self.free_run {
            panic!("{:?}: Only free run is supported!", self.instance);
        }
    }

    fn target(&self) -> u16 {
        self.target
    }

    fn set_target(&mut self, val: u16) {
        self.target = val;
    }

    fn counter(&self) -> u16 {
        self.counter
    }

    fn set_counter(&mut self, val: u16) {
        self.counter = val;
    }
}

/// Various synchronization modes when the timer is not in
/// free-run.
#[derive(Clone, Copy)]
enum Sync {
    /// For timer 1/2: Pause during H/VBlank. For timer 3: Stop counter
    Pause = 0,
    /// For timer 1/2: Reset counter at H/VBlank. For timer 3: Free run
    Reset = 1,
    /// For timer 1/2: Reset counter at H/VBlank and pause outside of
    /// it. For timer 3: Free run
    ResetAndPause = 2,
    /// For timer 1/2: Wait for H/VBlank and then free-run. For timer
    /// 3: Stop counter
    WaitForSync = 3,
}

impl Sync {
    fn from_field(field: u16) -> Sync {
        match field {
            0 => Sync::Pause,
            1 => Sync::Reset,
            2 => Sync::ResetAndPause,
            3 => Sync::WaitForSync,
            _ => panic!("Invalid sync mode {}", field),
        }
    }
}

/// Clock source is stored on two bits. The meaning of those bits
/// depends on the timer instance.
#[derive(Clone, Copy)]
struct ClockSource(u8);

impl ClockSource {
    fn from_field(field: u16) -> ClockSource {
        if (field & !3) != 0 {
            panic!("Invalid clock source: {:x}", field);
        }

        ClockSource(field as u8)
    }

    fn clock(self, instance: Peripheral) -> Clock {
        // Annoyingly timers 0 and 1 use values 0 or 2 for the
        // sysclock (1 and 3 for the alternative source) while timer 2
        // uses 0 and *1* for the sysclock (2 and 3 for the
        // alternative source). I don't understand why they needed two
        // bits to begin with, they could at least have made the
        // encoding consistent. Maybe there's more to it than that?
        let lookup = [
            // Timer 0
            [ Clock::SysClock, Clock::GpuDotClock,
              Clock::SysClock, Clock::GpuDotClock, ],
            // Timer 1
            [ Clock::SysClock, Clock::GpuHSync,
              Clock::SysClock, Clock::GpuHSync, ],
            // Timer 2
            [ Clock::SysClock,     Clock::SysClock,
              Clock::SysClockDiv8, Clock::SysClockDiv8, ],
        ];

        let source = self.0 as usize;

        match instance {
            Peripheral::Timer0 => lookup[0][source],
            Peripheral::Timer1 => lookup[1][source],
            Peripheral::Timer2 => lookup[2][source],
            _                  => unreachable!(),
        }
    }
}


/// The four possible clock sources for the timers
#[derive(Clone, Copy)]
enum Clock {
    /// The CPU clock at ~33.87MHz
    SysClock,
    /// The CPU clock divided by 8 (~4.23MHz)
    SysClockDiv8,
    /// The GPU's dotclock (depends on hardware, around 53Mhz)
    GpuDotClock,
    /// The GPU's HSync signal (deponds on hardware and video timings)
    GpuHSync,
}

impl Clock {
    /// Returns true if the clock comes from the GPU
    fn needs_gpu(self) -> bool {
        match self {
            Clock::GpuDotClock | Clock::GpuHSync => true,
            _ => false,
        }
    }
}

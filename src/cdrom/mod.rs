use memory::{Addressable, AccessWidth};
use timekeeper::TimeKeeper;
use memory::interrupts::{Interrupt, InterruptState};

/// CDROM Controller
pub struct CdRom {
    /// Some of the memory mapped registers change meaning depending
    /// on the value of the index.
    index: u8,
    /// Command arguments FIFO
    params: Fifo,
    /// Command response FIFO
    response: Fifo,
    /// Interrupt mask (5 bits)
    irq_mask: u8,
    /// Interrupt flag (5 bits)
    irq_flags: u8,
}

impl CdRom {
    pub fn new() -> CdRom {
        CdRom {
            index: 0,
            params: Fifo::new(),
            response: Fifo::new(),
            irq_mask: 0,
            irq_flags: 0,
        }
    }

    pub fn load<T: Addressable>(&mut self,
                                _: &mut TimeKeeper,
                                _: &mut InterruptState,
                                offset: u32) -> T {

        if T::width() != AccessWidth::Byte {
            panic!("Unhandled {:?} CDROM load", T::width());
        }

        let index = self.index;

        let unimplemented = || {
            panic!("read CDROM register {}.{}",
                   offset,
                   index)
        };

        let val =
            match offset {
                0 => self.status(),
                1 => {
                    if self.response.empty() {
                        println!("CDROM response FIFO underflow");
                    }
                    self.response.pop()
                }
                3 =>
                    match index {
                        1 => self.irq_flags,
                        _ => unimplemented(),
                    },
                _ => unimplemented(),
            };

        Addressable::from_u32(val as u32)
    }

    pub fn store<T: Addressable>(&mut self,
                                 _: &mut TimeKeeper,
                                 irq_state: &mut InterruptState,
                                 offset: u32,
                                 val: T) {

        if T::width() != AccessWidth::Byte {
            panic!("Unhandled {:?} CDROM store", T::width());
        }

        // All writeable registers are 8bit wide
        let val = val.as_u8();

        let index = self.index;

        let unimplemented = || {
            panic!("write CDROM register {}.{} {:x}",
                   offset,
                   index,
                   val)
        };

        let prev_irq = self.irq();

        match offset {
            0 => self.set_index(val),
            1 =>
                match index {
                    0 => self.command(val),
                    _ => unimplemented(),
                },
            2 =>
                match index {
                    0 => self.push_param(val),
                    1 => self.irq_mask(val),
                    _ => unimplemented(),
                },
            3 =>
                match index {
                    1 => {
                        self.irq_ack(val & 0x1f);

                        if val & 0x40 != 0 {
                            self.params.clear();
                        }

                        if val & 0xa0 != 0 {
                            panic!("Unhandled CDROM 3.1: {:02x}", val);
                        }
                    }
                    _ => unimplemented(),
                },
            _ => unimplemented(),
        }

        if !prev_irq && self.irq() {
            // Interrupt rising edge
            irq_state.assert(Interrupt::CdRom);
        }
    }

    fn status(&mut self) -> u8 {
        let mut r = self.index;

        // TODO: "XA-ADPCM fifo empty"
        r |= 0 << 2;
        r |= (self.params.empty() as u8) << 3;
        r |= (!self.params.full() as u8) << 4;
        r |= (!self.response.empty() as u8) << 5;
        // TODO: "Data FIFO not empty"
        r |= 0 << 6;
        // TODO: "Command busy"
        r |= 0 << 7;

        r
    }

    fn irq(&self) -> bool {
        self.irq_flags & self.irq_mask != 0
    }

    fn trigger_irq(&mut self, irq: IrqCode) {
        if self.irq_flags != 0 {
            panic!("Unsupported nested CDROM interrupt");
        }

        self.irq_flags = irq as u8;
    }

    fn set_index(&mut self, index: u8) {
        self.index = index & 3;
    }

    fn irq_ack(&mut self, v: u8) {
        self.irq_flags &= !v
    }

    fn irq_mask(&mut self, val: u8) {
        self.irq_mask = val & 0x1f;
    }

    fn command(&mut self, cmd: u8) {
        // TODO: is this really accurate? Need to run more tests.
        self.response.clear();

        match cmd {
            0x01 => self.cmd_get_stat(),
            0x19 => self.cmd_test(),
            _    => panic!("Unhandled CDROM command 0x{:02x}", cmd),
        }

        // It seems that the parameters get cleared in all cases (even
        // if an error occurs). I should run more tests to make sure...
        self.params.clear();
    }

    fn cmd_get_stat(&mut self) {
        if !self.params.empty() {
            // If this occurs on real hardware it should set bit 1 of
            // the result byte and then put a 2nd byte "0x20" to
            // signal the wrong number of params. It should also
            // trigger IRQ 5 instead of 3.
            //
            // For now I'm going to assume that if this occurs it
            // means that the emulator is buggy rather than the game.
            panic!("Unexected parameters for CDROM GetStat command");
        }

        // For now pretend that the tray is open (bit 4)
        self.response.push(0x10);

        self.trigger_irq(IrqCode::Ok);
    }

    fn cmd_test(&mut self) {
        if self.params.len() != 1 {
            panic!("Unexpected number of parameters for CDROM test command: {}",
                   self.params.len());
        }

        match self.params.pop() {
            0x20 => self.test_version(),
            n    => panic!("Unhandled CDROM test subcommand 0x{:02x}", n),
        }
    }

    fn test_version(&mut self) {
        // Values returned by my PAL SCPH-7502:
        // Year
        self.response.push(0x98);
        // Month
        self.response.push(0x06);
        // Day
        self.response.push(0x10);
        // Version
        self.response.push(0xc3);

        self.trigger_irq(IrqCode::Ok);
    }

    fn push_param(&mut self, param: u8) {
        if self.params.full() {
            println!("CDROM parameter FIFO overflow");
        }

        self.params.push(param);
    }
}

// Various IRQ codes used by the CDROM controller and their
// signification.
#[derive(Clone,Copy)]
enum IrqCode {
    Ok = 3,
}

/// 16byte FIFO used to store command arguments and results
struct Fifo {
    /// Data buffer
    buffer: [u8; 16],
    /// Write pointer (4bits + carry)
    write_idx: u8,
    /// Read pointer (4bits + carry)
    read_idx: u8,
}

impl Fifo {
    fn new() -> Fifo {
        Fifo {
            buffer: [0; 16],
            write_idx: 0,
            read_idx: 0,
        }
    }

    fn empty(&self) -> bool {
	// If both pointers point at the same cell and have the same carry the
	// FIFO is empty.
        (self.write_idx ^ self.read_idx) & 0x1f == 0
    }

    fn full(&self) -> bool {
        // The FIFO is full if both indexes point to the same cell
        // while having a different carry.
        (self.read_idx ^ self.write_idx ^ 0x10) & 0x1f == 0
    }

    fn clear(&mut self) {
        self.read_idx = self.write_idx;
        self.buffer = [0; 16];
    }

    // Retrieve the number of elements in the FIFO. This number is in
    // the range [0; 31] so it's potentially bogus if an overflow
    // occured. This does seem to match the behaviour of the actual
    // hardware though. For instance command 0x19 ("Test") takes a
    // single parameter. If you send 0 or more than one parameter you
    // get an error code back. However if you push 33 parameters in
    // the FIFO only the last one is actually used by the command and
    // it works as expected.
    fn len(&self) -> u8 {
        (self.write_idx.wrapping_sub(self.read_idx)) & 0x1f
    }

    fn push(&mut self, val: u8) {
        let idx = (self.write_idx & 0xf) as usize;

        self.buffer[idx] = val;

        self.write_idx = self.write_idx.wrapping_add(1);
    }

    fn pop(&mut self) -> u8 {
        let idx = (self.read_idx & 0xf) as usize;

        self.read_idx = self.read_idx.wrapping_add(1);

        self.buffer[idx]
    }
}

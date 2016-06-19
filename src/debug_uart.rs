//! Emulation of the dual debugging UART on expansion 2, used to get
//! debugging messages from various games and programs
//!
//! I only implemented the bare minimum to catch debug messages sent
//! by some applications.

use memory::Addressable;
use shared::SharedState;

#[derive(RustcDecodable, RustcEncodable)]
pub struct DebugUart {
    /// We don't want to display the TX data one character at a time
    /// so we attempt to line buffer it.
    tx_buffers: [String; 2],
}

impl DebugUart {
    pub fn new() -> DebugUart {
        DebugUart {
            tx_buffers: [String::with_capacity(TX_BUFFER_LEN),
                         String::with_capacity(TX_BUFFER_LEN)],
        }
    }

    pub fn load<A: Addressable>(&mut self,
                                _: &mut SharedState,
                                offset: u32) -> u32 {
        if A::size() != 1 {
            panic!("Unhandled debug UART load ({})", A::size())
        }

        match offset {
            // UART status register A. Return "Tx ready" bit set.
            0x21 => 1 << 2,
            _ => panic!("Unhandled debug UART store: {:x}",
                        offset),
        }
    }

    pub fn store<A: Addressable>(&mut self,
                                 _: &mut SharedState,
                                 offset: u32,
                                 val: u32) {

        if A::size() != 1 {
            panic!("Unhandled debug UART store ({})", A::size())
        }

        let val = val as u8;

        match offset {
            // UART mode register 2.A
            0x20 => (),
            // Clock select register A
            0x21 => (),
            // Command register A
            0x22 => (),
            // UART Tx register A
            0x23 => self.push_char(0, val as char),
            // Control register
            0x24 => (),
            // Command register B
            0x2a => (),
            // Output port configuration register
            0x2d => (),
            // Set output port bits
            0x2e => (),
            // Interrupt mask register
            0x25 => {
                // We don't implement interrupts for now
                if val != 0 {
                    panic!("Unhandled debug UART interrupt mask: {:02x}",
                           val);
                }
            }
            // Boot status register, is incremented by the BIOS during
            // bootup
            0x41 => debug!("BIOS boot status: {}", val),
            _ => panic!("Unhandled debug UART store: {:x} {:02x}",
                        offset, val),
        }
    }

    fn push_char(&mut self, port: usize, c: char) {
        let buffer = &mut self.tx_buffers[port];

        if c == '\n' || buffer.len() == buffer.capacity() {
            let uart =
                match port {
                    0 => 'A',
                    1 => 'B',
                    _ => unreachable!(),
                };

            debug!("Debug UART {}: {}", uart, buffer);
            buffer.clear();
        } else {
            buffer.push(c);
        }
    }
}

/// Maximum length of the Tx buffer before displaying the message even
/// if no newline is encountered
const TX_BUFFER_LEN: usize = 1024;

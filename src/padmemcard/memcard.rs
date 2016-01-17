pub struct MemCard {
    /// Counter keeping track of the current position in the reply
    /// sequence
    seq:    u8,
    /// False if the pad is done processing the current command
    active: bool,
    addr: u16,
    csum: u8,
    new: bool,
    read: bool,
    pre: u8,
}

impl MemCard {
    pub fn new() -> MemCard {

        MemCard {
            seq: 0,
            active: true,
            addr: 0,
            csum: 0,
            new: true,
            read: true,
            pre: 0,
        }
    }

    /// Called when the "select" line goes down.
    pub fn select(&mut self) {
        // Prepare for incomming command
        self.active = true;
        self.seq = 0;
    }

    /// The first return value is true if the gamepad issues a DSR
    /// pulse after the byte is read to notify the controller that
    /// more data can be read. The 2nd return value is the response
    /// byte.
    pub fn send_command(&mut self, cmd: u8) -> (u8, bool) {
        if !self.active {
            return (0xff, false);
        }

        let seq = self.seq;

        let (resp, dsr) = self.handle_command(seq, cmd);

        // If we're not asserting DSR it either means that we've
        // encountered an error or that we have nothing else to
        // reply. In either case we won't be handling any more command
        // bytes in this transaction.
        self.active = dsr;

        self.seq += 1;

        (resp, dsr)
    }

    pub fn handle_command(&mut self, seq: u8, cmd: u8) -> (u8, bool) {
        let (res, d) =
        match seq {
            // First byte should be 0x81 if the command targets
            // the memcard
            0 => (0xff, (cmd == 0x81)),
            1 => {
                let r = if self.new {
                    0x08
                } else {
                    0x00
                };

                self.read = cmd == b'R';

                (r, cmd == b'R' || cmd == b'W')
            }
            2 => (0x5a, true),
            3 => (0x5d, true),
            4 => {
                self.addr = (cmd as u16) << 8;

                self.csum = cmd;

                (0x00, true)
            }
            5 => {
                self.addr |= cmd as u16;

                self.csum ^= cmd;

                println!("MEMCARD READ {:x}", self.addr);

                if !self.read {
                    self.seq = 9;
                }

                ((self.addr >> 8) as u8, true)
            }
            6 => (0x5c, true),
            7 => {
                if !self.read {
                    self.seq = 138;
                }

                (0x5d, true)
            }
            8 => ((self.addr >> 8) as u8, true),
            9 => (self.addr as u8, true),
            10...137 => {
                if self.read {
                    let mut addr = (self.addr as usize) * 128;

                    addr += (seq - 10) as usize;

                    let b = MEMCARD[addr as usize];

                    self.csum ^= b;

                    (b, true)
                } else {
                    (self.pre, true)
                }
            }
            138 => {
                if self.read {
                    (self.csum, true)
                } else {
                    self.seq = 5;
                    (self.pre, true)
                }
            }
            139 => {
                if !self.read {
                    self.new = false;
                }

                (b'G', true)
            }
            // First button state byte: direction cross, start and
            // select.
            //3 => (self.0 as u8, true),
            // 2nd button state byte: shoulder buttons and "shape"
            // buttons. We don't asert DSR for the last byte.
            //4 => ((self.0 >> 8) as u8, false),
            // Shouldn't be reached
            _ => (0xff, false),
        };

        self.pre = cmd;

        if seq > 0 || d == true {
            println!("MEMCARD CMD: {} {:x} -> {:x} {}", seq, cmd, res, d);
        }

        (res, d)
    }
}

static MEMCARD: &'static[u8] = include_bytes!("memcard.img");

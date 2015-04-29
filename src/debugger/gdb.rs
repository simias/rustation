 use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

use debugger::Debugger;
use cpu::Cpu;

pub struct GdbRemote {
    remote: TcpStream,
    /// Checksum for the current response
    csum: u8,
}

impl GdbRemote {
    pub fn new(listener: &TcpListener) -> GdbRemote {
        println!("Debugger waiting for gdb connection...");

        let remote =
            match listener.accept() {
                Ok((stream, sockaddr)) => {
                    println!("Connection from {}", sockaddr);
                    stream
                }
                Err(e) => panic!("Accept failed: {}", e),
            };

        GdbRemote {
            remote: remote,
            csum: 0,
        }
    }

    pub fn serve(&mut self,
                 debugger: &mut Debugger,
                 cpu: &mut Cpu) -> Result<(), ()> {

        loop {
            match self.next_packet() {
                PacketResult::Ok(packet) => {
                    try!(self.ack());
                    try!(self.handle_packet(debugger, cpu, &packet));
                }
                PacketResult::BadChecksum(_) => {
                    // Request retransmission
                    try!(self.nack());
                }
                PacketResult::EndOfStream => {
                    // Session over
                    break;
                }
            }
        }

        Ok(())
    }

    /// Attempt to return a single GDB packet.
    fn next_packet(&mut self) -> PacketResult {

        enum State {
            WaitForStart,
            InPacket,
            WaitForCheckSum,
            WaitForCheckSum2(u8),
        };

        let mut state = State::WaitForStart;

        let mut packet = Vec::new();
        let mut csum = 0u8;

        for r in (&self.remote).bytes() {

            let byte =
                match r {
                    Ok(b)  => b,
                    Err(e) => {
                        println!("GDB remote error: {}", e);
                        return PacketResult::EndOfStream;
                    }
                };

            match state {
                State::WaitForStart => {
                    if byte == b'$' {
                        // Start of packet
                        state = State::InPacket;
                    }
                }
                State::InPacket => {
                    if byte == b'#' {
                        // End of packet
                        state = State::WaitForCheckSum;
                    } else {
                        // Append byte to the packet
                        packet.push(byte);
                        // Update checksum
                        csum = csum.wrapping_add(byte);
                    }
                }
                State::WaitForCheckSum => {
                    match ascii_hex(byte) {
                        Some(b) => {
                            state = State::WaitForCheckSum2(b);
                        }
                        None => {
                            println!("Got invalid GDB checksum char {}",
                                     byte);
                            return PacketResult::BadChecksum(packet);
                        }
                    }
                }
                State::WaitForCheckSum2(c1) => {
                    match ascii_hex(byte) {
                        Some(c2) => {
                            let expected = (c1 << 4) | c2;

                            if expected != csum {
                                println!("Got invalid GDB checksum: {:x} {:x}",
                                         expected, csum);
                                return PacketResult::BadChecksum(packet);
                            }

                            // Checksum is good, we're done!
                            return PacketResult::Ok(packet);
                        }
                        None => {
                            println!("Got invalid GDB checksum char {}",
                                     byte);
                            return PacketResult::BadChecksum(packet);
                        }
                    }
                }
            }
        }

        println!("GDB remote end of stream");
        return PacketResult::EndOfStream;
    }

    /// Acknowledge packet reception
    fn ack(&mut self) -> Result<(), ()> {
        if let Err(e) = self.remote.write(b"+") {
            println!("Couldn't send ACK to GDB remote: {}", e);
            Err(())
        } else {
            Ok(())
        }
    }

    /// Request packet retransmission
    fn nack(&mut self) -> Result<(), ()> {
        if let Err(e) = self.remote.write(b"-") {
            println!("Couldn't send NACK to GDB remote: {}", e);
            Err(())
        } else {
            Ok(())
        }
    }

    fn handle_packet(&mut self,
                     _: &mut Debugger,
                     cpu: &mut Cpu,
                     packet: &[u8]) -> Result<(), ()> {

        // Start response packet
        try!(self.write(b"$"));
        // Clear Checksum
        self.csum = 0;

        let res =
            match packet[0] {
                b'?' => self.reply(b"S00"),
                b'm' => self.read_memory(cpu, &packet[1..]),
                b'g' => self.read_registers(cpu),
                // Send empty response for unsupported packets
                _ => Ok(()),
            };

        try!(res);

        // Each packet ends with '$' followed by the checksum as two
        // hexadecimal digits.
        try!(self.write(b"#"));
        let csum = self.csum;
        try!(self.reply_u8(csum));

        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<(), ()> {
        match self.remote.write(data) {
            // XXX Should we check the number of bytes written? What
            // do we do if it's less than we expected, retransmit?
            Ok(_) => Ok(()),
            Err(e) => {
                println!("Couldn't send data to GDB remote: {}", e);
                Err(())
            }
        }
    }

    /// Send data to the remote GDB instance and update the
    /// checksum.
    fn reply(&mut self, response: &[u8]) -> Result<(), ()> {

        // Validate the response and update the checksum
        self.csum =
            response.iter().fold(self.csum, |csum, &b| {
                if b == b'#' || b == b'$' {
                    panic!("Invalid character {} in GDB response",
                           b as char);
                }

                // The checksum is a simple sum of all bytes
                csum.wrapping_add(b)
            });

        try!(self.write(response));

        Ok(())
    }

    /// Send an u32 as 4 little endian bytes
    fn reply_u32(&mut self, v: u32) -> Result<(), ()> {
        for i in 0..4 {
            try!(self.reply_u8((v >> (i * 8)) as u8));
        }

        Ok(())
    }

    /// Send an u16 as 2 little endian bytes
    fn reply_u16(&mut self, v: u16) -> Result<(), ()> {
        for i in 0..2 {
            try!(self.reply_u8((v >> (i * 8)) as u8));
        }

        Ok(())
    }

    /// Convert an u8 into an hexadecimal string and send it to the
    /// remote
    fn reply_u8(&mut self, v: u8) -> Result<(), ()> {
        let to_hex = b"0123456789abcdef";

        self.reply(&[
            to_hex[(v >> 4) as usize],
            to_hex[(v & 0xf) as usize],
            ])
    }

    fn read_registers(&mut self, cpu: &mut Cpu) -> Result<(), ()> {

        // Send general purpose registers
        for &r in cpu.regs() {
            try!(self.reply_u32(r));
        }

        // Send control registers
        for &r in &[ cpu.sr(),
                     cpu.lo(),
                     cpu.hi(),
                     cpu.bad(),
                     cpu.cause(),
                     cpu.pc() ] {
            try!(self.reply_u32(r));
        }

        // GDB expects 73 registers for the MIPS architecture: the 38
        // above plus all the floating point registers. Since the
        // playstation doesn't support those we just return `x`s to
        // notify GDB that those registers are unavailable.
        //
        // The doc says that it's normally used for core dumps however
        // (when the value of a register can't be found in a trace) so
        // I'm not sure it's the right thing to do. If it causes
        // problems we might just return 0 (or some sane default
        // value) instead.
        for _ in 38..73 {
            try!(self.reply(b"xxxxxxxx"));
        }

        Ok(())
    }

    /// Read a region of memory. The packet format should be
    /// `ADDR,LEN`, both in hexadecimal
    fn read_memory(&mut self, cpu: &mut Cpu, packet: &[u8]) -> Result<(), ()> {

        let (addr, len) =
            match parse_addr_len(packet) {
                Some(r) => r,
                // Bad format
                None => return self.reply(b"E00"),
            };

        if len == 0 {
            // Should we reply with an empty string here? Probably
            // doesn't matter
            return self.reply(b"E00");
        }

        // We can now fetch the data. First we handle the case where
        // addr is not aligned using an ad-hoc heuristic. A better way
        // to do this might be to figure out which peripheral we're
        // accessing and select the most meaningful access width.
        let align = addr % 4;

        let sent =
            match align {
                1|3 => {
                    // If we fall on the first or third byte of a word
                    // we use byte accesses until we reach the next
                    // word or the end of the requested length
                    let count = ::std::cmp::min(len, 4 - align);

                    for i in 0..count {
                        try!(self.reply_u8(cpu.load(addr + i)));
                    }
                    count
                }
                2 => {
                    if len == 1 {
                        // Only one byte to read
                        try!(self.reply_u8(cpu.load(addr)));
                        1
                    } else {
                        try!(self.reply_u16(cpu.load(addr)));
                        2
                    }
                }
                _ => 0,
            };

        let addr = addr + sent;
        let len = len + sent;

        // We can now deal with the word-aligned portion of the
        // transfer (if any). It's possible that addr is not word
        // aligned here if we entered the case "align == 2, len == 1"
        // above but it doesn't matter because in this case "nwords"
        // will be 0 so nothing will be fetched.
        let nwords = len / 4;

        for i in 0..nwords {
            try!(self.reply_u32(cpu.load(addr + i * 4)));
        }

        // See if we have anything remaining
        let addr = addr + nwords * 4;
        let rem = len - nwords * 4;

        match rem {
            1|3 => {
                for i in 0..rem {
                    try!(self.reply_u8(cpu.load(addr + i)));
                }
            }
            2 => {
                try!(self.reply_u16(cpu.load(addr)));
            }
            _ => ()
        }

        Ok(())
    }
}

enum PacketResult {
    Ok(Vec<u8>),
    BadChecksum(Vec<u8>),
    EndOfStream,
}

/// Parse a string in the format `addr,len` (both as hexadecimal
/// strings) and return the values as a tuple. Returns `None` if
/// the format is bogus.
fn parse_addr_len(string: &[u8]) -> Option<(u32, u32)> {

    // Look for the comma separator
    let addr_end =
        match string.iter().position(|&b| b == b',') {
            Some(p) => p,
            // Bad format
            None => return None,
        };

    if addr_end == 0 || addr_end == string.len() - 1 {
        // No address or length
        return None;
    }

    let mut addr = 0;

    // Parse address
    for &b in &string[0..addr_end] {
        addr = addr << 4;
        addr |=
            match ascii_hex(b) {
                Some(v) => v as u32,
                // Bad hex
                None => return None,
            };
    }

    let mut len = 0;

    // Parse length
    for &b in &string[addr_end + 1..] {
        len = len << 4;
        len |=
            match ascii_hex(b) {
                Some(v) => v as u32,
                // Bad hex
                None => return None,
            };
    }

    Some((addr, len))
}

/// Get the value of an integer encoded in single lowercase
/// hexadecimal ASCII digit. Return None if the character is not valid
/// hexadecimal
fn ascii_hex(b: u8) -> Option<u8> {
    if b >= b'0' && b <= b'9' {
        Some(b - b'0')
    } else if b >= b'a' && b <= b'f' {
        Some(10 + (b - b'a'))
    } else {
        // Invalid
        None
    }
}

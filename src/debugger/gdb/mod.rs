use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

use debugger::Debugger;
use cpu::Cpu;
use memory::{Byte, HalfWord, Word};
use interrupt::InterruptState;

use self::reply::Reply;

mod reply;

type GdbResult = Result<(), ()>;

pub struct GdbRemote {
    remote: TcpStream,
}

impl GdbRemote {
    pub fn new(listener: &TcpListener) -> GdbRemote {
        info!("Debugger waiting for gdb connection...");

        let remote =
            match listener.accept() {
                Ok((stream, sockaddr)) => {
                    info!("Connection from {}", sockaddr);
                    stream
                }
                Err(e) => panic!("Accept failed: {}", e),
            };

        GdbRemote {
            remote: remote,
        }
    }

    // Serve a single remote request
    pub fn serve(&mut self,
                 debugger: &mut Debugger,
                 cpu: &mut Cpu) -> GdbResult {

        match self.next_packet() {
            PacketResult::Ok(packet) => {
                try!(self.ack());
                self.handle_packet(debugger, cpu, &packet)
            }
            PacketResult::BadChecksum(_) => {
                // Request retransmission
                self.nack()
            }
            PacketResult::EndOfStream => {
                // Session over
                Err(())
            }
        }
    }

    /// Attempt to return a single GDB packet.
    fn next_packet(&mut self) -> PacketResult {

        // Parser state machine
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
                        warn!("GDB remote error: {}", e);
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
                            warn!("Got invalid GDB checksum char {}",
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
                                warn!("Got invalid GDB checksum: {:x} {:x}",
                                      expected, csum);
                                return PacketResult::BadChecksum(packet);
                            }

                            // Checksum is good, we're done!
                            return PacketResult::Ok(packet);
                        }
                        None => {
                            warn!("Got invalid GDB checksum char {}",
                                  byte);
                            return PacketResult::BadChecksum(packet);
                        }
                    }
                }
            }
        }

        warn!("GDB remote end of stream");
        return PacketResult::EndOfStream;
    }

    /// Acknowledge packet reception
    fn ack(&mut self) -> GdbResult {
        if let Err(e) = self.remote.write(b"+") {
            warn!("Couldn't send ACK to GDB remote: {}", e);
            Err(())
        } else {
            Ok(())
        }
    }

    /// Request packet retransmission
    fn nack(&mut self) -> GdbResult {
        if let Err(e) = self.remote.write(b"-") {
            warn!("Couldn't send NACK to GDB remote: {}", e);
            Err(())
        } else {
            Ok(())
        }
    }

    fn handle_packet(&mut self,
                     debugger: &mut Debugger,
                     cpu: &mut Cpu,
                     packet: &[u8]) -> GdbResult {

        let command = packet[0];
        let args = &packet[1..];

        let res =
            match command {
                b'?' => self.send_status(),
                b'm' => self.read_memory(cpu, args),
                b'g' => self.read_registers(cpu),
                b'c' => self.resume(debugger, cpu, args),
                b's' => self.step(debugger, cpu, args),
                b'Z' => self.add_breakpoint(debugger, args),
                b'z' => self.del_breakpoint(debugger, args),
                // Send empty response for unsupported packets
                _ => self.send_empty_reply(),
            };

        // Check for errors
        try!(res);

        Ok(())
    }

    fn send_reply(&mut self, reply: Reply) -> GdbResult {
        match self.remote.write(&reply.into_packet()) {
            // XXX Should we check the number of bytes written? What
            // do we do if it's less than we expected, retransmit?
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Couldn't send data to GDB remote: {}", e);
                Err(())
            }
        }
    }

    fn send_empty_reply(&mut self) -> GdbResult {
        self.send_reply(Reply::new())
    }

    fn send_string(&mut self, string: &[u8]) -> GdbResult {
        let mut reply = Reply::new();

        reply.push(string);

        self.send_reply(reply)
    }

    fn send_error(&mut self) -> GdbResult {
        // GDB remote doesn't specify what the error codes should
        // be. Should be bother coming up with our own convention?
        self.send_string(b"E00")
    }

    pub fn send_status(&mut self) -> GdbResult {
        // Maybe we should return different values depending on the
        // cause of the "break"?
        self.send_string(b"S00")
    }

    pub fn send_ok(&mut self) -> GdbResult {
        self.send_string(b"OK")
    }

    fn read_registers(&mut self, cpu: &mut Cpu) -> GdbResult {

        let mut reply = Reply::new();

        // Send general purpose registers
        for &r in cpu.regs() {
            reply.push_u32(r);
        }

        // Send control registers
        for &r in &[ cpu.sr(),
                     cpu.lo(),
                     cpu.hi(),
                     cpu.bad(),
                     // XXX We should figure out a way to get the real
                     // irq_state over here...
                     cpu.cause(InterruptState::new()),
                     cpu.pc() ] {
            reply.push_u32(r);
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
            reply.push(b"xxxxxxxx");
        }

        self.send_reply(reply)
    }

    /// Read a region of memory. The packet format should be
    /// `ADDR,LEN`, both in hexadecimal
    fn read_memory(&mut self, cpu: &mut Cpu, args: &[u8]) -> GdbResult {

        let mut reply = Reply::new();

        let (addr, len) = try!(parse_addr_len(args));

        if len == 0 {
            // Should we reply with an empty string here? Probably
            // doesn't matter
            return self.send_error();
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
                        reply.push_u8(cpu.examine::<Byte>(addr + i) as u8);
                    }
                    count
                }
                2 => {
                    if len == 1 {
                        // Only one byte to read
                        reply.push_u8(cpu.examine::<Byte>(addr) as u8);
                        1
                    } else {
                        reply.push_u16(cpu.examine::<HalfWord>(addr) as u16);
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
            reply.push_u32(cpu.examine::<Word>(addr + i * 4));
        }

        // See if we have anything remaining
        let addr = addr + nwords * 4;
        let rem = len - nwords * 4;

        match rem {
            1|3 => {
                for i in 0..rem {
                    reply.push_u8(cpu.examine::<Byte>(addr + i) as u8);
                }
            }
            2 => {
                reply.push_u16(cpu.examine::<HalfWord>(addr) as u16);
            }
            _ => ()
        }

        self.send_reply(reply)
    }

    /// Continue execution
    fn resume(&mut self,
              debugger: &mut Debugger,
              cpu: &mut Cpu,
              args: &[u8]) -> GdbResult {

        if args.len() > 0 {
            // If an address is provided we restart from there
            let addr = try!(parse_hex(args));

            cpu.force_pc(addr);
        }

        // Tell the debugger we want to resume execution.
        debugger.resume();

        Ok(())
    }

    // Step works exactly like continue except that we're only
    // supposed to execute a single instruction.
    fn step(&mut self,
            debugger: &mut Debugger,
            cpu: &mut Cpu,
            args: &[u8]) -> GdbResult {

        debugger.set_step();

        self.resume(debugger, cpu, args)
    }

    // Add a breakpoint or watchpoint
    fn add_breakpoint(&mut self,
                      debugger: &mut Debugger,
                      args: &[u8]) -> GdbResult {

        // Check if the request contains a command list
        if args.iter().any(|&b| b == b';') {
            // Not sure if I should signal an error or send an empty
            // reply here to signal that command lists are not
            // supported. I think GDB will think that we don't support
            // this breakpoint type *at all* if we return an empty
            // reply. I don't know how it handles errors however.
            return self.send_error();
        };

        let (btype, addr, kind) = try!(parse_breakpoint(args));

        // Only kind "4" makes sense for us: 32bits standard MIPS mode
        // breakpoint. The MIPS-specific kinds are defined here:
        // https://sourceware.org/gdb/onlinedocs/gdb/MIPS-Breakpoint-Kinds.html
        if kind != b'4' {
            // Same question as above, should I signal an error?
            return self.send_error();
        }

        match btype {
            b'0' => debugger.add_breakpoint(addr),
            b'2' => debugger.add_write_watchpoint(addr),
            b'3' => debugger.add_read_watchpoint(addr),
            // Unsupported breakpoint type
            _ => return self.send_empty_reply(),
        }

        self.send_ok()
    }

    // Delete a breakpoint or watchpoint
    fn del_breakpoint(&mut self,
                      debugger: &mut Debugger,
                      args: &[u8]) -> GdbResult {

        let (btype, addr, kind) = try!(parse_breakpoint(args));

        // Only 32bits standard MIPS mode breakpoint supported
        if kind != b'4' {
            return self.send_error();
        }

        match btype {
            b'0' => debugger.del_breakpoint(addr),
            b'2' => debugger.del_write_watchpoint(addr),
            b'3' => debugger.del_read_watchpoint(addr),
            // Unsupported breakpoint type
            _ => return self.send_empty_reply(),
        }

        self.send_ok()
    }

}

enum PacketResult {
    Ok(Vec<u8>),
    BadChecksum(Vec<u8>),
    EndOfStream,
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

/// Parse an hexadecimal string and return the value as an
/// integer. Return `None` if the string is invalid.
fn parse_hex(hex: &[u8]) -> Result<u32, ()> {
    let mut v = 0u32;

    for &b in hex {
        v <<= 4;

        v |=
            match ascii_hex(b) {
                Some(h) => h as u32,
                // Bad hex
                None => return Err(()),
            };
    }

    Ok(v)
}

/// Parse a string in the format `addr,len` (both as hexadecimal
/// strings) and return the values as a tuple. Returns `None` if
/// the format is bogus.
fn parse_addr_len(args: &[u8]) -> Result<(u32, u32), ()> {

    // split around the comma
    let args: Vec<_> = args.split(|&b| b == b',').collect();

    if args.len() != 2 {
        // Invalid number of arguments
        return Err(());
    }

    let addr = args[0];
    let len = args[1];

    if addr.len() == 0 || len.len() == 0 {
        // Missing parameter
        return Err(());
    }

    // Parse address
    let addr = try!(parse_hex(addr));
    let len = try!(parse_hex(len));

    Ok((addr, len))
}

/// Parse breakpoint arguments: the format is
/// `type,addr,kind`. Returns the three parameters in a tuple or an
/// error if a format error has been encountered.
fn parse_breakpoint(args: &[u8]) -> Result<(u8, u32, u8), ()> {
    // split around the comma
    let args: Vec<_> = args.split(|&b| b == b',').collect();

    if args.len() != 3 {
        // Invalid number of arguments
        return Err(());
    }

    let btype = args[0];
    let addr = args[1];
    let kind = args[2];

    if btype.len() != 1 || kind.len() != 1 {
        // Type and kind should only be one character each
        return Err(());
    }

    let btype = btype[0];
    let kind = kind[0];

    let addr = try!(parse_hex(addr));

    Ok((btype, addr, kind))
}

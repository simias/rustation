use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

use debugger::Debugger;
use cpu::Cpu;

pub struct GdbRemote {
    remote: TcpStream,
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
        }
    }

    pub fn serve(&mut self,
                 debugger: &mut Debugger,
                 cpu: &mut Cpu) -> Result<(), ()> {

        loop {
            match self.next_packet() {
                                PacketResult::Ok(packet) => {
                    try!(self.ack());
                    try!(self.handle_packet(debugger, cpu, packet));
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
                     packet: Vec<u8>) -> Result<(), ()> {

        match packet[0] {
            b'?' => self.response(b"S00"),
            b'm' => self.response(b"00000000"),
            b'g' => self.read_registers(cpu),
            // Send empty response for unsupported packets
            _ => self.response(b""),
        }
    }

    fn response(&mut self, response: &[u8]) -> Result<(), ()> {
        let mut csum = 0u8;
        let mut r = Vec::with_capacity(response.len() + 4);

        // Start of response
        r.push(b'$');

        for &b in response {
            // XXX we could handle RLE response encoding if we wanted

            if b == b'#' || b == b'$' {
                panic!("Invalid character {} in GDB response",
                       b as char);
            }

            // Update checksum
            csum = csum.wrapping_add(b);

            r.push(b);
        }

        // End of response
        r.push(b'#');

        let to_hex = b"0123456780abcdef";

        // Append checksum
        r.push(to_hex[(csum >> 4) as usize]);
        r.push(to_hex[(csum & 0xf) as usize]);

        // Send response
        if let Err(e) = self.remote.write(&r) {
            println!("Couldn't send GDB remote response: {}", e);
            return Err(());
        }

        // XXX We should check for remote ack/nack
        Ok(())
    }

    fn read_registers(&mut self, cpu: &mut Cpu) -> Result<(), ()> {
        let mut s = String::new();

        for &r in cpu.regs() {
            s = s + &u32_to_hexle(r);
        }

        for &r in &[ cpu.sr(),
                     cpu.lo(),
                     cpu.hi(),
                     cpu.bad(),
                     cpu.cause(),
                     cpu.pc() ] {
            s = s + &u32_to_hexle(r);
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
            s = s + &format!("xxxxxxxx");
        }

        self.response(&s.into_bytes())
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

fn u32_to_hexle(v: u32) -> String {
    format!("{:02x}", v as u8)
        + &format!("{:02x}", (v >> 8) as u8)
        + &format!("{:02x}", (v >> 16) as u8)
        + &format!("{:02x}", (v >> 24) as u8)
}

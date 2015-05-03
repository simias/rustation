/// GDB remote reply
pub struct Reply {
    /// Packet data
    data: Vec<u8>,
    /// Checksum
    csum: u8,
}

impl Reply {
    pub fn new() -> Reply {
        // 32bytes is probably sufficient for the majority of replies
        let mut data = Vec::with_capacity(32);

        // Each reply begins with a dollar sign
        data.push(b'$');

        Reply {
            data: data,
            csum: 0,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        // Update checksum
        for &b in data {
            self.csum = self.csum.wrapping_add(b);

            if b == b'$' || b == b'$' {
                panic!("Invalid char in GDB response");
            }
        }

        self.data.extend(data.iter().cloned());
    }

    pub fn push_u8(&mut self, byte: u8) {
        let to_hex = b"0123456789abcdef";

        self.push(&[
            to_hex[(byte >> 4) as usize],
            to_hex[(byte & 0xf) as usize],
            ])
    }

    /// Push an u16 as 2 little endian bytes
    pub fn push_u16(&mut self, v: u16) {
        for i in 0..2 {
            self.push_u8((v >> (i * 8)) as u8);
        }
    }

    /// Push an u32 as 4 little endian bytes
    pub fn push_u32(&mut self, v: u32) {
        for i in 0..4 {
            self.push_u8((v >> (i * 8)) as u8);
        }
    }

    /// Finalize the reply: append the checksum and return the
    /// complete packet
    pub fn into_packet(mut self) -> Vec<u8> {
        // End of packet
        self.data.push(b'#');
        // Append checksum.
        let csum = self.csum;
        self.push_u8(csum);

        self.data
    }
}

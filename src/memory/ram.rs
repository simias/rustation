use std::iter;

/// RAM
pub struct Ram {
    /// RAM buffer
    data: Vec<u8>
}

impl Ram {

    /// Instantiate main RAM with garbage values
    pub fn new() -> Ram {

        let size = 2 * 1024 * 1024;

        // Default RAM contents are garbage
        let data = iter::repeat(0xca).take(size).collect();

        Ram { data: data }
    }

    /// Fetch the 32bit little endian word at `offset`
    pub fn load32(&self, offset: u32) -> u32 {
        let offset = offset as usize;

        let b0 = self.data[offset + 0] as u32;
        let b1 = self.data[offset + 1] as u32;
        let b2 = self.data[offset + 2] as u32;
        let b3 = self.data[offset + 3] as u32;

        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }

    /// Fetch the 16bit little endian halfword at `offset`
    pub fn load16(&self, offset: u32) -> u16 {
        let offset = offset as usize;

        let b0 = self.data[offset + 0] as u16;
        let b1 = self.data[offset + 1] as u16;

        b0 | (b1 << 8)
    }

    /// Fetch the byte at `offset`
    pub fn load8(&self, offset: u32) -> u8 {
        self.data[offset as usize]
    }

    /// Store the 32bit little endian word `val` into `offset`
    pub fn store32(&mut self, offset: u32, val: u32) {
        let offset = offset as usize;

        let b0 = val as u8;
        let b1 = (val >> 8) as u8;
        let b2 = (val >> 16) as u8;
        let b3 = (val >> 24) as u8;

        self.data[offset + 0] = b0;
        self.data[offset + 1] = b1;
        self.data[offset + 2] = b2;
        self.data[offset + 3] = b3;
    }

    /// Store the 16bit little endian halfword `val` into `offset`
    pub fn store16(&mut self, offset: u32, val: u16) {
        let offset = offset as usize;

        let b0 = val as u8;
        let b1 = (val >> 8) as u8;

        self.data[offset + 0] = b0;
        self.data[offset + 1] = b1;
    }

    /// Store the byte `val` into `offset`
    pub fn store8(&mut self, offset: u32, val: u8) {
        self.data[offset as usize] = val;
    }
}

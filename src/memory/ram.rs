use std::iter;
use super::Addressable;

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

    /// Fetch the little endian value at `offset`
    pub fn load<T: Addressable>(&self, offset: u32) -> T {
        let offset = offset as usize;

        let mut v = 0;

        for i in 0..T::width() as usize {
            v |= (self.data[offset + i] as u32) << (i * 8)
        }

        Addressable::from_u32(v)
    }

    /// Store the 32bit little endian word `val` into `offset`
    pub fn store<T: Addressable>(&mut self, offset: u32, val: T) {
        let offset = offset as usize;

        let val = val.as_u32();

        for i in 0..T::width() as usize {
            self.data[offset + i] = (val >> (i * 8)) as u8;
        }
    }
}

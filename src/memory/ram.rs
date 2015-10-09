use super::Addressable;

/// RAM
pub struct Ram {
    /// RAM buffer. Boxed in order not to overflow the stack at the
    /// construction site. Might change once "placement new" is
    /// available.
    data: Box<[u8; RAM_SIZE]>
}

impl Ram {

    /// Instantiate main RAM with garbage values
    pub fn new() -> Ram {

        Ram { data: Box::new([0xca; RAM_SIZE]) }
    }


    /// Fetch the little endian value at `offset`
    pub fn load<T: Addressable>(&self, offset: u32) -> T {
        // The two MSB are ignored, the 2MB RAM is mirorred four times
        // over the first 8MB of address space
        let offset = (offset & 0x1fffff) as usize;

        let mut v = 0;

        for i in 0..T::width() as usize {
            v |= (self.data[offset + i] as u32) << (i * 8)
        }

        Addressable::from_u32(v)
    }

    /// Store the 32bit little endian word `val` into `offset`
    pub fn store<T: Addressable>(&mut self, offset: u32, val: T) {
        // The two MSB are ignored, the 2MB RAM is mirorred four times
        // over the first 8MB of address space
        let offset = (offset & 0x1fffff) as usize;

        let val = val.as_u32();

        for i in 0..T::width() as usize {
            self.data[offset + i] = (val >> (i * 8)) as u8;
        }
    }
}

/// ScratchPad memory
pub struct ScratchPad {
    data: [u8; SCRATCH_PAD_SIZE]
}

impl ScratchPad {

    /// Instantiate scratchpad with garbage values
    pub fn new() -> ScratchPad {
        ScratchPad { data: [0xdb; SCRATCH_PAD_SIZE] }
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

/// Main PlayStation RAM: 2Megabytes
const RAM_SIZE: usize = 2 * 1024 * 1024;

/// ScatchPad (data cache used as fast RAM): 1Kilobyte
const SCRATCH_PAD_SIZE: usize = 1024;

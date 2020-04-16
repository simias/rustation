use bigarray::{BigArrayBox, BigArray};

use super::Addressable;

/// RAM
#[derive(Serialize,Deserialize)]
pub struct Ram {
    /// RAM buffer. Boxed in order not to overflow the stack at the
    /// construction site. Might change once "placement new" is
    /// available.
    #[serde(with = "BigArrayBox")]
    data: Box<[u8; RAM_SIZE]>
}

impl Ram {

    /// Instantiate main RAM with garbage values
    pub fn new() -> Ram {

        Ram { data: box_array![0xca; RAM_SIZE] }
    }

    /// Fetch the little endian value at `offset`
    pub fn load<T: Addressable>(&self, offset: u32) -> u32 {
        // The two MSB are ignored, the 2MB RAM is mirorred four times
        // over the first 8MB of address space
        let offset = (offset & 0x1fffff) as usize;

        let mut v = 0;

        for i in 0..T::size() as usize {
            v |= (self.data[offset + i] as u32) << (i * 8)
        }

        v
    }

    /// Store the 32bit little endian word `val` into `offset`
    pub fn store<T: Addressable>(&mut self, offset: u32, val: u32) {
        // The two MSB are ignored, the 2MB RAM is mirorred four times
        // over the first 8MB of address space
        let offset = (offset & 0x1fffff) as usize;

        for i in 0..T::size() as usize {
            self.data[offset + i] = (val >> (i * 8)) as u8;
        }
    }
}


/// ScratchPad memory
#[derive(Serialize,Deserialize)]
pub struct ScratchPad {
    #[serde(with = "BigArray")]
    data: [u8; SCRATCH_PAD_SIZE]
}

impl ScratchPad {

    /// Instantiate scratchpad with garbage values
    pub fn new() -> ScratchPad {
        ScratchPad { data: [0xdb; SCRATCH_PAD_SIZE] }
    }

    /// Fetch the little endian value at `offset`
    pub fn load<T: Addressable>(&self, offset: u32) -> u32 {
        let offset = offset as usize;

        let mut v = 0;

        for i in 0..T::size() as usize {
            v |= (self.data[offset + i] as u32) << (i * 8)
        }

        v
    }

    /// Store the 32bit little endian word `val` into `offset`
    pub fn store<T: Addressable>(&mut self, offset: u32, val: u32) {
        let offset = offset as usize;

        for i in 0..T::size() as usize {
            self.data[offset + i] = (val >> (i * 8)) as u8;
        }
    }
}

/// Main PlayStation RAM: 2Megabytes
const RAM_SIZE: usize = 2 * 1024 * 1024;

/// ScatchPad (data cache used as fast RAM): 1Kilobyte
const SCRATCH_PAD_SIZE: usize = 1024;

#[test]
fn ram_read() {
    use super::{Word, HalfWord, Byte};

    let mut ram = Ram::new();

    ram.store::<Word>(0, 0x12345678);
    ram.store::<Word>(32, 0x0abcdef0);

    assert!(ram.load::<Word>(0) == 0x12345678);

    assert!(ram.load::<Word>(32) == 0x0abcdef0);

    assert!(ram.load::<HalfWord>(0) == 0x5678);
    assert!(ram.load::<HalfWord>(2) == 0x1234);

    assert!(ram.load::<HalfWord>(32) == 0xdef0);
    assert!(ram.load::<HalfWord>(34) == 0x0abc);

    assert!(ram.load::<Byte>(0) == 0x78);
    assert!(ram.load::<Byte>(1) == 0x56);
    assert!(ram.load::<Byte>(2) == 0x34);
    assert!(ram.load::<Byte>(3) == 0x12);

    assert!(ram.load::<Byte>(32) == 0xf0);
    assert!(ram.load::<Byte>(33) == 0xde);
    assert!(ram.load::<Byte>(34) == 0xbc);
    assert!(ram.load::<Byte>(35) == 0x0a);
}

#[test]
fn ram_write() {
    use super::{Word, HalfWord, Byte};

    let mut ram = Ram::new();

    ram.store::<Word>(32, 0x12345678);
    ram.store::<HalfWord>(32, 0xabcd);
    assert!(ram.load::<Word>(32) == 0x1234abcd);

    ram.store::<Word>(32, 0x12345678);
    ram.store::<HalfWord>(34, 0xabcd);
    assert!(ram.load::<Word>(32) == 0xabcd5678);

    ram.store::<Word>(32, 0x12345678);
    ram.store::<Byte>(32, 0xab);
    assert!(ram.load::<Word>(32) == 0x123456ab);

    ram.store::<Word>(32, 0x12345678);
    ram.store::<Byte>(33, 0xab);
    assert!(ram.load::<Word>(32) == 0x1234ab78);

    ram.store::<Word>(32, 0x12345678);
    ram.store::<Byte>(34, 0xab);
    assert!(ram.load::<Word>(32) == 0x12ab5678);

    ram.store::<Word>(32, 0x12345678);
    ram.store::<Byte>(35, 0xab);
    assert!(ram.load::<Word>(32) == 0xab345678);
}

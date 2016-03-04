use cpu::gte::precision::SubpixelPrecision;

use super::Addressable;

/// RAM
pub struct Ram<T> {
    /// RAM buffer. Boxed in order not to overflow the stack at the
    /// construction site. Might change once "placement new" is
    /// available.
    data: Box<[(u32, T); RAM_SIZE_WORDS]>
}

impl<T: SubpixelPrecision> Ram<T> {
    /// Instantiate main RAM with garbage values
    pub fn new() -> Ram<T> {
        Ram { data: box_array![(0xca, T::empty()); RAM_SIZE_WORDS] }
    }

    /// Store the 32bit little endian word `val` into `offset`
    pub fn store<A: Addressable>(&mut self, offset: u32, val: u32) {
        // The two MSB are ignored, the 2MB RAM is mirorred four times
        // over the first 8MB of address space
        let offset = (offset & 0x1fffff) as usize;

        let word_addr = offset >> 2;
        let align = (offset & 3) * 8;

        let mask = A::mask() << align;
        let val = (val << align) & mask;

        let word = self.data[word_addr].0;

        self.data[word_addr] = ((word & !mask) | val, T::empty());
    }

    /// Store a word in RAM alongside its associated subpixel data
    pub fn store_precise(&mut self, offset: u32, val: (u32, T)) {
        let offset = (offset & 0x1fffff) as usize;

        let word_addr = offset >> 2;

        self.data[word_addr] = val;
    }
}

impl<T> Ram<T> {
    /// Fetch the little endian value at `offset`
    pub fn load<A: Addressable>(&self, offset: u32) -> u32 {
        // The two MSB are ignored, the 2MB RAM is mirorred four times
        // over the first 8MB of address space
        let offset = (offset & 0x1fffff) as usize;

        let word_addr = offset >> 2;
        let align = (offset & 3) * 8;

        let word = self.data[word_addr].0 >> align;

        word & A::mask()
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

/// RAM_SIZE in 32bit word unit
const RAM_SIZE_WORDS: usize = RAM_SIZE / 4;

/// ScatchPad (data cache used as fast RAM): 1Kilobyte
const SCRATCH_PAD_SIZE: usize = 1024;

#[test]
fn ram_read() {
    use cpu::gte::precision::NativeVertex;
    use super::{Word, HalfWord, Byte};

    let mut ram: Ram<NativeVertex> = Ram::new();

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
    use cpu::gte::precision::NativeVertex;
    use super::{Word, HalfWord, Byte};

    let mut ram: Ram<NativeVertex> = Ram::new();

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

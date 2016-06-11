//! Emulation of the parallel interface (the Parallel I/O connector on
//! the back of older PlayStation models)

use rustc_serialize::{Decodable, Encodable, Decoder, Encoder};

use memory::Addressable;
use shared::SharedState;

pub mod exe_loader;

pub struct ParallelIo {
    module: Box<ParallelIoModule>,
}

impl ParallelIo {

    pub fn disconnected() -> ParallelIo {
        ParallelIo {
            module: Box::new(Disconnected),
        }
    }

    pub fn set_module(&mut self, module: Box<ParallelIoModule>) {
        self.module = module;
    }

    pub fn load<T: Addressable>(&mut self,
                                shared: &mut SharedState,
                                offset: u32) -> u32 {
        let mut r = 0;

        for i in 0..T::size() {
            let b = self.module.load(shared, offset + i as u32);

            r |= (b as u32) << (8 * i);
        }

        r
    }
}

impl Encodable for ParallelIo {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        // Since the parallel interface is supposed to support a whole
        // bunch of modules (some of which potentially defined outside
        // of this crate) it's not trivial to serialize them. For not
        // let's ignore it and not include it in the savestate.
        s.emit_nil()
    }
}

impl Decodable for ParallelIo {
    fn decode<D: Decoder>(d: &mut D) -> Result<ParallelIo, D::Error> {
        try!(d.read_nil());

        Ok(ParallelIo::disconnected())
    }
}

/// Since there can be all sorts of hardware connected to the Parallel
/// I/O port I abstract it behind a trait interface
pub trait ParallelIoModule {
    /// Parallel I/O load 8bits at offset `offset` (within the
    /// expansion 1 memory region)
    fn load(&mut self, shared: &mut SharedState, offset: u32) -> u8;

    /// Parallel I/O byte store at offset `offset` (within the expansion 1
    /// memory region)
    fn store(&mut self, shared: &mut SharedState, offset: u32, val: u8);
}

/// A dummy implementation of ParallelIo when nothing is connected
pub struct Disconnected;

impl ParallelIoModule for Disconnected {
    fn load(&mut self, _: &mut SharedState, _: u32) -> u8 {
        // When no expansion is present the CPU reads full ones
        !0
    }

    fn store(&mut self, _: &mut SharedState, _: u32, _: u8) {
        // NOP
    }
}

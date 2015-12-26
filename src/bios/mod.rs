use memory::Addressable;

use self::db::Metadata;

pub mod db;

/// BIOS image
pub struct Bios {
    /// BIOS memory. Boxed in order not to overflow the stack at the
    /// construction site. Might change once "placement new" is
    /// available.
    data: Box<[u8; BIOS_SIZE]>,
    metadata: &'static Metadata,
}

impl Bios {

    /// Create a BIOS image from `binary` and attempt to match it with
    /// an entry in the database. If no match can be found return
    /// `None`.
    pub fn new(binary: Box<[u8; BIOS_SIZE]>) -> Option<Bios> {
        match db::lookup(&*binary) {
            Some(metadata) => Some(Bios {
                data: binary,
                metadata: metadata,
            }),
            None => None,
        }
    }

    /// Fetch the little endian value at `offset`
    pub fn load<T: Addressable>(&self, offset: u32) -> u32 {
        let offset = offset as usize;

        let mut r = 0;

        for i in 0..T::size() as usize {
            r |= (self.data[offset + i] as u32) << (8 * i)
        }

        r
    }

    pub fn metadata(&self) -> &'static Metadata {
        self.metadata
    }
}

/// BIOS images are always 512KB in length
pub const BIOS_SIZE: usize = 512 * 1024;

use rustc_serialize::{Decodable, Encodable, Decoder, Encoder};

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

    /// Return a static pointer to the BIOS's Metadata
    pub fn metadata(&self) -> &'static Metadata {
        self.metadata
    }
}

impl Encodable for Bios {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        // We don't store the full BIOS image in the savestate, mainly
        // because I want to be able to share and distribute
        // savestates without having to worry about legal
        // implications. Let's just serialize the checksum to make
        // sure we use the correct BIOS when loading the savestate.

        let sha256 = &self.metadata.sha256;

        s.emit_seq(sha256.len(), |s| {
            for (i, b) in sha256.iter().enumerate() {
                try!(s.emit_seq_elt(i, |s| b.encode(s)));
            }
            Ok(())
        })
    }
}

impl Decodable for Bios {
    fn decode<D: Decoder>(d: &mut D) -> Result<Bios, D::Error> {
        d.read_seq(|d, len| {
            let mut sha256 = [0; 32];

            if len != sha256.len() {
                return Err(d.error("wrong BIOM checksum length"));
            }

            for (i, b) in sha256.iter_mut().enumerate() {
                *b = try!(d.read_seq_elt(i, |d| Decodable::decode(d)))
            }

            let meta =
                match db::lookup_sha256(&sha256) {
                    Some(m) => m,
                    None => return Err(d.error("unknown BIOS checksum")),
                };

            // Create an "empty" BIOS instance, only referencing the
            // metadata. It's up to the caller to fill the blanks.
            let mut bios =
                Bios {
                    data: box_array![0; BIOS_SIZE],
                    metadata: meta,
                };


            // Store `0x7badb105` (an invalid instruction) in the BIOS
            // for troubleshooting.
            for (i, b) in bios.data.iter_mut().enumerate() {
                *b = (0x7badb105 >> ((i % 4) * 2)) as u8;
            }

            Ok(bios)
        })
    }
}

/// BIOS images are always 512KB in length
pub const BIOS_SIZE: usize = 512 * 1024;

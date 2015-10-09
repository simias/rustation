use std::path::Path;
use std::fs::File;
use std::io::{Result, Error, ErrorKind, Read};

use super::Addressable;

/// BIOS image
pub struct Bios {
    /// BIOS memory. Boxed in order not to overflow the stack at the
    /// construction site. Might change once "placement new" is
    /// available.
    data: Box<[u8; BIOS_SIZE]>,
}

impl Bios {

    /// Load a BIOS image from the file located at `path`
    pub fn new(path: &Path) -> Result<Bios> {

        let mut file = try!(File::open(path));

        // Load the BIOS
        let mut data = Box::new([0; BIOS_SIZE]);
        let mut nread = 0;

        while nread < BIOS_SIZE {
            nread +=
                match try!(file.read(&mut data[nread..])) {
                    0 => return Err(Error::new(ErrorKind::InvalidInput,
                                               "BIOS file is too small")),
                    n => n,
                };
        }

        // Make sure the BIOS file is not too big, it's probably not a
        // good dump otherwise.
        if try!(file.read(&mut [0; 1])) != 0 {
            return Err(Error::new(ErrorKind::InvalidInput,
                                  "BIOS file is too big"));
        }

        Ok(Bios { data: data })
    }

    /// Fetch the little endian value at `offset`
    pub fn load<T: Addressable>(&self, offset: u32) -> T {
        let offset = offset as usize;

        let mut r = 0;

        for i in 0..T::width() as usize {
            r |= (self.data[offset + i] as u32) << (8 * i)
        }

        Addressable::from_u32(r)
    }
}

/// BIOS images are always 512KB in length
const BIOS_SIZE: usize = 512 * 1024;

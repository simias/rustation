use cdimage::{Image, CdError};
use cdimage::sector::Sector;
use cdimage::msf::Msf;
use cdimage::bcd::Bcd;

/// Structure representing an ISO9660 directory
pub struct Directory<'a> {
    /// Reference to the underlying disc image
    image: &'a Image,
    /// Contents of the directory
    extent: Vec<u8>,
}

impl<'a> Directory<'a> {
    pub fn ls(&self) {
        let mut entries = self.extent.as_slice();

        while entries.len() > 0 {
            let dir_len = entries[0] as usize;
            println!("{} {}", entries.len(), dir_len);


            assert!(dir_len >= 34);

            let name_len = entries[32];

            let name = String::from_utf8_lossy(&entries[33..(33 + name_len as usize)]);

            println!("{}", name);

            entries = &entries[dir_len..];
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// Cdimage access error
    CdError(CdError),
    /// Couldn't find the ISO9660 magic "CD0001"
    BadMagic,
    /// Couldn't find the Primary Volume Descriptor
    MissingPrimaryVolumeDescriptor,
    /// Unexpected Volume Descriptor version
    BadVolumDescriptorVersion,
    /// Encountered an invalid extent location
    BadExtent(u32),
    /// Miscellaneous ISO9660 format error containing a description of
    /// the problem
    BadFormat(String)
}

impl From<CdError> for Error {
    fn from(e: CdError) -> Error {
        Error::CdError(e)
    }
}

pub fn open_image(image: &mut Image) -> Result<Directory, Error> {
    // The first 16 sectors are the "system area" which is ignored by
    // the ISO filesystem. The Volume Descriptor Set should start at
    // 00:00:16 in track 01
    let mut msf = try!(image.track_msf(Bcd::one(),
                                       Msf::from_bcd(0, 0, 0x16).unwrap()));

    let mut sector = Sector::empty();

    // Look for the primary volume descriptor
    loop {
        try!(image.read_sector(&mut sector, msf));

        let volume_descriptor = try!(sector.mode2_xa_payload());

        // Check volume descriptor "standard identifier"
        if &volume_descriptor[1..6] != b"CD001" {
            return Err(Error::BadMagic);
        }

        // Byte 0 contains the "volume descriptor type".
        match volume_descriptor[0] {
            // Primary Volume Descriptor
            0x01 => break,
            // Volume Descriptor Set Terminator
            0xff => return Err(Error::MissingPrimaryVolumeDescriptor),
            // Unhandled volume descriptor type, ignore
            _ => (),
        }

        // Not the primary volume descriptor, move on to the next
        // sector
        msf = msf.next().unwrap();
    }

    let volume_descriptor = try!(sector.mode2_xa_payload());

    // Volume Descriptor Version
    if volume_descriptor[6] != 0x01 {
        return Err(Error::BadVolumDescriptorVersion);
    }

    // We can now open the root directory descriptor
    let root_dir_descriptor = &volume_descriptor[156..190];

    let flags = root_dir_descriptor[25];

    let is_dir = (flags & 0x2) != 0 ;

    if !is_dir {
        return Err(Error::BadFormat("Root directory is not a directory"
                                    .into()));
    }

    let extent_location = read_u32(&root_dir_descriptor[2..10]);
    let extent_len = read_u32(&root_dir_descriptor[10..18]);

    let extent =
        try!(read_extent(image, extent_location, extent_len));

    Ok(Directory {
        image: image,
        extent: extent,
    })
}

/// Try to read an extent from a contiguous series of sectors starting
/// at `extent_location`
fn read_extent(image: &mut Image,
               extent_location: u32,
               extent_len: u32) -> Result<Vec<u8>, Error> {

    let mut extent_len = extent_len as usize;

    let track_msf =
        match Msf::from_sector_index(extent_location) {
            Some(m) => m,
            None => return Err(Error::BadExtent(extent_location)),
        };

    let mut msf = try!(image.track_msf(Bcd::one(), track_msf));

    let mut extent = Vec::with_capacity(extent_len);

    let mut sector = Sector::empty();

    while extent_len > 0 {
        try!(image.read_sector(&mut sector, msf));

        let data = try!(sector.mode2_xa_payload());

        let len =
            if extent_len > 2048 {
                2048
            } else {
                extent_len
            };

        for i in 0..len {
            extent.push(data[i]);
        }

        extent_len -= len;
        msf = msf.next().unwrap();
    }

    Ok(extent)
}

// fn open_dir_file(_image: &mut Image,
//                  dir_desc: &[u8],
//                  _path: &str) -> Result<(), Error> {

//     if dir_desc[27] != 0 {
//         panic!("Unsupported interleaved file in ISO9660 filesystem");
//     }

//     let flags = dir_desc[25];

//     let is_dir = flags & 0x2;

//     let len_dr = dir_desc[0];

    

//     let data_len = read_u32(&dir_desc[10..18]);

//     let identifier_len = dir_desc[32];

//     let file_identifier = String::from_utf8_lossy(&dir_desc[33..(33 + identifier_len as usize)]);

//     let mut sector = Sector::empty();

//     let msf = Msf::from_sector_index(extent_location);

//     println!("file_identifier: {}", file_identifier);

//     panic!("{} {} {} {} {}", len_dr, extent_location, data_len, flags, is_dir);
// }

/// Read a 32bit number stored in "both byte order" format
fn read_u32(v: &[u8]) -> u32 {
    // Only use the little endian representation. Should we bother
    // validating that the BE version is coherent?
    v[0] as u32 |
    ((v[1] as u32) << 8) |
    ((v[2] as u32) << 16) |
    ((v[3] as u32) << 24)
}

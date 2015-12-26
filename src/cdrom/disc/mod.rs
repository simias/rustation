use std::path::Path;
use std::fs::File;
use std::io;
use std::io::{Seek, Read};

use self::crc::crc32;
use self::msf::Msf;

pub mod msf;
mod crc;

/// PlayStation disc.
///
/// XXX: add support for CD-DA? Not really useful but shouldn't
/// be very hard either. We need to support audio tracks anyway...
pub struct Disc {
    /// BIN file
    file: File,
    /// Disc region
    region: Region,
}

/// Disc region coding
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Japan (NTSC): SCEI
    Japan,
    /// North America (NTSC): SCEA
    NorthAmerica,
    /// Europe (PAL): SCEE
    Europe,
}

impl Disc {
    /// Reify a disc from file at `path` and attempt to identify it.
    pub fn from_path(path: &Path) -> io::Result<Disc> {
        let file = try!(File::open(path));

        let disc = Disc {
            file: file,
            // Use a dummy id for now.
            region: Region::Japan,
        };

        disc.extract_region()
    }

    pub fn region(&self) -> Region {
        self.region
    }

    /// Attempt to discover the region of the disc. This way we know
    /// which string to return in the CD-ROM drive's "get id" command
    /// and we can also decide which BIOS and output video standard to
    /// use based on the game disc.
    fn extract_region(mut self) -> io::Result<Disc> {
        // In order to identify the type of disc we're going to use
        // sector 00:02:04 which should contain the "Licensed by..."
        // string.
        let msf = Msf::from_bcd(0x00, 0x02, 0x04);

        let sector = try!(self.read_data_sector(msf));

        // On the discs I've tried we always have an ASCII license
        // string in the first 76 data bytes (after the header). I
        // hope it's always like that...
        let license_blob = &sector.data_bytes()[24..100];

        // There are spaces everywhere in the string (including in the
        // middle of some words), let's clean it up and convert to a
        // string
        let license: String = license_blob.iter()
            .filter_map(|&b| {
                match b {
                    b'A'...b'z' => Some(b as char),
                    _ => None,
                }
            })
            .collect();

        self.region =
            match license.as_ref() {
                "LicensedbySonyComputerEntertainmentInc"
                    => Region::Japan,
                "LicensedbySonyComputerEntertainmentAmerica"
                    => Region::NorthAmerica,
                "LicensedbySonyComputerEntertainmentofAmerica"
                    => Region::NorthAmerica,
                "LicensedbySonyComputerEntertainmentEurope"
                    => Region::Europe,
                _ => {
                    let msg =
                        format!("couldn't identify disc region string: {}",
                                license);
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                                              msg));
                }
            };

        Ok(self)
    }

    /// Read a Mode 1 or 2 CD-ROM XA sector and validate it. Will
    /// return an error if used on a CD-DA raw audio sector.
    pub fn read_data_sector(&mut self, msf: Msf) -> io::Result<XaSector> {
        let sector = try!(self.read_sector(msf));

        sector.validate_mode_1_2(msf)
    }

    /// Read a raw CD sector without any validation. For Mode 1 and 2
    /// sectors XaSector::validate_mode_1_2 should then be called to
    /// make sure the sector is valid.
    fn read_sector(&mut self, msf: Msf) -> io::Result<XaSector> {
        // XXX for now I assume that the track 01 pregap is 2 seconds
        // (150 sectors), needs to parse cuesheet.
        let index = msf.sector_index() - 150;

        // Convert in a byte offset in the bin file
        let pos = index as u64 * SECTOR_SIZE as u64;

        try!(self.file.seek(io::SeekFrom::Start(pos)));

        let mut sector = XaSector::new();
        let mut nread = 0;

        while nread < SECTOR_SIZE {
            nread +=
                match try!(self.file.read(&mut sector.raw[nread..])) {
                    0 => return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                   "short sector read")),
                    n => n,
                };
        }

        Ok(sector)
    }
}

/// Structure representing a single CD-ROM XA sector
pub struct XaSector {
    /// The raw array of 2352 bytes contained in the sector
    raw: [u8; SECTOR_SIZE],
}

impl XaSector {
    pub fn new() -> XaSector {
        XaSector {
            raw: [0; SECTOR_SIZE],
        }
    }

    /// Return payload data byte at `index`
    pub fn data_byte(&self, index: u16) -> u8 {
        let index = index as usize;

        self.raw[index]
    }

    /// Return the sector data as a byte slice
    fn data_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Validate CD-ROM XA Mode 1 or 2 sector
    fn validate_mode_1_2(self, msf: Msf) -> io::Result<XaSector> {
        let error = |what| {
            Err(io::Error::new(io::ErrorKind::InvalidData, what))
        };

        // Check sync pattern
        if self.raw[0..12] != SECTOR_SYNC_PATTERN {
            return error(format!("invalid sector sync at {}", msf));
        }

        // Check that the expected MSF maches the one we have in the
        // header
        if self.msf() != msf {
            return error(format!("unexpected sector MSF: expected {} got {}",
                                 msf, self.msf()));
        }

        let mode = self.raw[15];

        match mode {
            // XXX handle Mode 1
            1 => panic!("Unhandled Mode 1 sector at {}", msf),
            2 => self.validate_mode2(),
            _ => error(format!("unhandled sector mode {} at {}",
                               mode, msf)),
        }
    }

    /// Parse and validate CD-ROM XA mode 2 sector.
    ///
    /// Regular CD-ROM defines mode2 as just containing 0x920 bytes of
    /// "raw" data after the 16byte sector header. However the CD-ROM
    /// XA spec defines two possible "forms" for this mode 2 data,
    /// there's an 8 byte sub-header at the beginning of the data that
    /// will tell us how to interpret it.
    fn validate_mode2(self) -> io::Result<XaSector> {
        // Mode 2 XA sub-header (from the CDi "green book"):
        //
        //   byte 16: File number
        //   byte 17: Channel number
        //   byte 18: Submode
        //   byte 19: Coding information
        //   byte 20: File number
        //   byte 21: Channel number
        //   byte 22: Submode
        //   byte 23: Coding information

        // Make sure the two copies of the subcode are the same,
        // otherwise the sector is probably corrupted or in the wrong
        // format.
        let submode = self.raw[18];
        let submode_copy = self.raw[22];

        if submode != submode_copy {
            let msg =
                format!("Sector {}: mode 2 submode missmatch: {:02x}, {:02x}",
                        self.msf(), submode, submode_copy);

            return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
        }

        // Look for form in submode bit 5
        match submode & 0x20 != 0 {
            false => self.validate_mode2_form1(),
            true  => self.validate_mode2_form2(),
        }
    }

    /// CD-ROM XA Mode 2 Form 1: 0x800 bytes of data protected by a
    /// 32bit CRC for error detection and 276 bytes of error
    /// correction codes.
    fn validate_mode2_form1(self) -> io::Result<XaSector> {
        // Validate CRC
        let crc = crc32(&self.raw[16..2072]);

        let sector_crc = self.raw[2072] as u32
            | ((self.raw[2073] as u32) << 8)
            | ((self.raw[2074] as u32) << 16)
            | ((self.raw[2075] as u32) << 24);

        if crc != sector_crc {
            // Sector appears corrupted. Should we attempt to correct
            // it with the ECC data? For now I assume it means it's a
            // bad dump, tell the user to fix it...
            let msg =
                format!("Sector {}: Mode 2 Form 1 CRC missmatch", self.msf());

            return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
        }

        Ok(self)
    }

    /// CD-ROM XA Mode 2 Form 2: 0x914 bytes of data without ECC or
    /// EDC.
    ///
    /// Last 4 bytes are "reserved for quality control" but the CDi
    /// spec doesn't mandate what goes in it exactly, only that "[i]t
    /// is recommended that the same EDC algorithm should be used here
    /// as is used for Form 1 sectors. If this algorithm is not used,
    /// then the reserved bytes are set to 0."
    ///
    /// We'll have to see what's put in it in the wild, or simply
    /// ignore it like the CDi does (or maybe CD-ROM XA doesn't follow
    /// the CDi spec exactly here? Who knows.)
    fn validate_mode2_form2(self) -> io::Result<XaSector> {
        Ok(self)
    }

    /// Return the MSF in the sector's header
    fn msf(&self) -> Msf {
        // The MSF is recorded just after the sync pattern
        Msf::from_bcd(self.raw[12],
                      self.raw[13],
                      self.raw[14])
    }
}

/// Size of a CD sector in bytes
const SECTOR_SIZE: usize = 2352;

/// CD-ROM sector sync pattern: 10 0xff surrounded by two 0x00. Not
/// used in CD-DA audio tracks.
const SECTOR_SYNC_PATTERN: [u8; 12] = [0x00,
                                       0xff, 0xff, 0xff, 0xff, 0xff,
                                       0xff, 0xff, 0xff, 0xff, 0xff,
                                       0x00];

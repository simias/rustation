use std::fmt;

use cdimage::{Image, CdError};
use cdimage::msf::Msf;
use cdimage::bcd::Bcd;
use cdimage::sector::Sector;

use rustc_serialize::{Decodable, Encodable, Decoder, Encoder};

use super::iso9660;

/// PlayStation disc.
///
/// XXX: add support for CD-DA? Not really useful but shouldn't
/// be very hard either. We need to support audio tracks anyway...
pub struct Disc {
    /// Image file
    image: Box<Image>,
    /// Disc serial number
    serial: SerialNumber,
}

impl Disc {
    /// Reify a disc using `image` as a backend.
    pub fn new(mut image: Box<Image>) -> Result<Disc, String> {
        let serial =
            match extract_serial_number(&mut *image) {
                Some(s) => s,
                None => {
                    return Err("Couldn't find disc serial number".into());
                }
            };

        let disc = Disc {
            image: image,
            serial: serial,
        };

        Ok(disc)
    }

    pub fn region(&self) -> Region {
        // For now I prefer to panic to catch potential issues with
        // the serial number handling code, alternatively we could
        // fallback on `extract_system_region`
        match self.serial.region() {
            Some(r) => r,
            None => panic!("Can't establish the region of {}", self.serial),
        }
    }

    pub fn serial_number(&self) -> SerialNumber {
        self.serial
    }

    pub fn image(&mut self) -> &mut Image {
        &mut*self.image
    }
}

impl Encodable for Disc {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        // Only encode the serial number
        self.serial.encode(s)
    }
}

impl Decodable for Disc {
    fn decode<D: Decoder>(d: &mut D) -> Result<Disc, D::Error> {
        let serial = try!(SerialNumber::decode(d));

        // Placeholder disc image
        Ok(Disc {
            image: Box::new(MissingImage),
            serial: serial,
        })
    }
}

/// Dummy Image implemementation used when deserializing a Disc. Since
/// we don't want to store the entire disc in the image it will be
/// missing after a load, it's up to the frontend to make sure to
/// reload the image.
struct MissingImage;

impl Image for MissingImage {
    fn image_format(&self) -> String {
        panic!("Missing CD image!");
    }

    fn read_sector(&mut self, _: &mut Sector, _: Msf) -> Result<(), CdError> {
        panic!("Missing CD image!");
    }

    fn track_msf(&self, _: Bcd, _: Msf) -> Result<Msf, CdError> {
        panic!("Missing CD image!");
    }
}

/// Disc region
#[derive(Clone, Copy, Debug, PartialEq, Eq, RustcDecodable, RustcEncodable)]
pub enum Region {
    /// Japan (NTSC): SCEI
    Japan,
    /// North America (NTSC): SCEA
    NorthAmerica,
    /// Europe (PAL): SCEE
    Europe,
}

/// Disc serial number
#[derive(Copy, Clone, PartialEq, Eq, RustcDecodable, RustcEncodable)]
pub struct SerialNumber([u8; 10]);

impl SerialNumber {
    /// Create a dummy serial number: UNKN-00000. Used when no serial
    /// number can be found.
    pub fn dummy() -> SerialNumber {
        SerialNumber(*b"UNKN-00000")
    }

    /// Extract a serial number from a standard PlayStation binary
    /// name of the form "aaaa_ddd.dd"
    fn from_bin_name(bin: &[u8]) -> Option<SerialNumber> {
        if bin.len() != 11 {
            return None;
        }

        if bin[4] != b'_' {
            // This will fail for the few "lightspan educational"
            // discs since they have a serial number looking like
            // "LSP-123456". Those games are fairly obscure and
            // some of them seem to have weird and nonstandards
            // SYSTEM.CNF anyway.
            return None;
        }

        let mut serial = [0u8; 10];

        let to_upper = |b| {
            if b >= b'a' && b <= b'z' {
                b - b'a' + b'A'
            } else {
                b
            }
        };

        serial[0] = to_upper(bin[0]);
        serial[1] = to_upper(bin[1]);
        serial[2] = to_upper(bin[2]);
        serial[3] = to_upper(bin[3]);
        serial[4] = b'-';
        serial[5] = bin[5];
        serial[6] = bin[6];
        serial[7] = bin[7];
        serial[8] = bin[9];
        serial[9] = bin[10];

        Some(SerialNumber(serial))
    }

    pub fn region(&self) -> Option<Region> {
        match &self.0[0..4] {
            b"SCPS" | b"SLPS" | b"SLPM" | b"PAPX" => Some(Region::Japan),
            b"SCUS" | b"SLUS" | b"LSP-" => Some(Region::NorthAmerica),
            b"SCES" | b"SCED" | b"SLES" | b"SLED" => Some(Region::Europe),
            _ => None,
        }
    }
}

impl fmt::Display for SerialNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

/// Attempt to discover the region of the disc using the license
/// string stored in the system area of the official PlayStation
/// ISO filesystem.
pub fn extract_system_region(image: &mut Image) -> Result<Region, CdError> {
    // In order to identify the type of disc we're going to use
    // sector 00:00:04 from Track01 which should contain the
    // "Licensed by..."  string.
    let msf = try!(image.track_msf(Bcd::one(),
                                   Msf::from_bcd(0, 0, 4).unwrap()));

    let mut sector = Sector::empty();

    try!(image.read_sector(&mut sector, msf));

    // On the discs I've tried we always have an ASCII license
    // string in the first 76 data bytes. We'll see if it holds
    // true for all the discs out there...
    let license_blob = &try!(sector.mode2_xa_payload())[0..76];

    // There are spaces everywhere in the license string
    // (including in the middle of some words), let's clean it up
    // and convert to a canonical string
    let license: String = license_blob.iter()
        .filter_map(|&b| {
            match b {
                b'A'...b'z' => Some(b as char),
                _ => None,
            }
        })
        .collect();

    let region =
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
                warn!("Couldn't identify disc region string: {}", license);
                return Err(CdError::BadFormat);
            }
        };

    Ok(region)
}

/// Attempt to extract the serial number of the disc. All officially
/// licensed PlayStation game should have a serial number.
fn extract_serial_number(image: &mut Image) -> Option<SerialNumber> {

    let system_cnf =
        match read_system_cnf(image) {
            Ok(c) => c,
            Err(e) => {
                warn!("Couldn't read SYSTEM.CNF: {:?}", e);
                return None;
            }
        };

    // Now we need to parse the SYSTEM.CNF file to get the content of
    // the "BOOT" line
    let mut boot_path = None;

    for line in system_cnf.split(|&b| b == b'\n') {
        let words: Vec<_> = line
            .split(|&b| b == b' ' || b == b'\t' || b == b'=')
            .filter(|w| !w.is_empty())
            .collect();

        if words.len() == 2 {
            if words[0] == b"BOOT" {
                boot_path = Some(words[1]);
                break;
            }
        }
    }

    let boot_path =
        match boot_path {
            Some(b) => b,
            None => {
                warn!("Couldn't find BOOT line in SYSTEM.CNF");
                return None;
            }
        };

    // boot_path should look like "cdrom:\FOO\BAR\...\aaaa_ddd.dd;1"
    let path: Vec<_> = boot_path
        .split(|&b| b == b':' || b == b';' || b == b'\\')
        .collect();

    if path.len() < 2 {
        warn!("Unexpected boot path: {}", String::from_utf8_lossy(boot_path));
        return None;
    }

    let bin_name = path[path.len() - 2];

    let serial = SerialNumber::from_bin_name(&bin_name);

    if serial.is_none() {
        warn!("Unexpected bin name: {}", String::from_utf8_lossy(bin_name));
    }

    serial
}

fn read_system_cnf(image: &mut Image) -> Result<Vec<u8>, iso9660::Error> {
    let dir = try!(iso9660::open_image(image));

    let system_cnf = try!(dir.entry_by_name(b"SYSTEM.CNF;1"));

    // SYSTEM.CNF should be a small text file, 1MB should bb way more
    // than necessary
    let len = system_cnf.extent_len();

    if len > 1024 * 1024 {
        let desc = format!("SYSTEM.CNF is too big: {}B", len);
        return Err(iso9660::Error::BadFormat(desc));
    }

    system_cnf.read_file(image)
}

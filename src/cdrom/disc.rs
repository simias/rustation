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
    /// Disc region
    region: Region,
}

impl Disc {
    /// Reify a disc using `image` as a backend.
    pub fn new(mut image: Box<Image>) -> Result<Disc, CdError> {
        let region = 
            try!(extract_region(&mut *image));

        try!(extract_serial_number(&mut *image));

        let disc = Disc {
            image: image,
            region: region,
        };

        Ok(disc)
    }

    pub fn region(&self) -> Region {
        self.region
    }

    pub fn image(&mut self) -> &mut Image {
        &mut*self.image
    }
}

impl Encodable for Disc {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        // XXX We could maybe store something to make sure we're
        // loading the right disc. A checksum might be a bit overkill,
        // maybe just the game's serial number or something?
        s.emit_nil()
    }
}

impl Decodable for Disc {
    fn decode<D: Decoder>(d: &mut D) -> Result<Disc, D::Error> {
        try!(d.read_nil());

        // Placeholder disc image
        Ok(Disc {
            image: Box::new(MissingImage),
            region: Region::Japan,
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

/// Disc region coding
#[derive(Clone, Copy, Debug, PartialEq, Eq, RustcDecodable, RustcEncodable)]
pub enum Region {
    /// Japan (NTSC): SCEI
    Japan,
    /// North America (NTSC): SCEA
    NorthAmerica,
    /// Europe (PAL): SCEE
    Europe,
}

/// Attempt to discover the region of the disc. This way we know
/// which string to return in the CD-ROM drive's "get id" command
/// and we can also decide which BIOS and output video standard to
/// use based on the game disc.
fn extract_region(image: &mut Image) -> Result<Region, CdError> {
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
fn extract_serial_number(image: &mut Image) -> Result<(), CdError> {
    let dir = iso9660::open_image(image).unwrap();

    dir.ls();

    Ok(())
}

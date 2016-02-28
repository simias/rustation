use cdimage::{Image, CdError};
use cdimage::msf::Msf;
use cdimage::sector::Sector;

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
    pub fn new(image: Box<Image>) -> Result<Disc, CdError> {
        let disc = Disc {
            image: image,
            // Use a dummy id for now.
            region: Region::Japan,
        };

        disc.extract_region()
    }

    pub fn region(&self) -> Region {
        self.region
    }

    pub fn image(&mut self) -> &mut Image {
        &mut*self.image
    }

    /// Attempt to discover the region of the disc. This way we know
    /// which string to return in the CD-ROM drive's "get id" command
    /// and we can also decide which BIOS and output video standard to
    /// use based on the game disc.
    fn extract_region(mut self) -> Result<Disc, CdError> {
        // In order to identify the type of disc we're going to use
        // sector 00:02:04 which should contain the "Licensed by..."
        // string.
        let msf = Msf::from_bcd(0x00, 0x02, 0x04).unwrap();

        let mut sector = Sector::empty();

        try!(self.image.read_sector(&mut sector, msf));

        // On the discs I've tried we always have an ASCII license
        // string in the first 76 data bytes. We'll see if it holds
        // true for all the discs out there...
        let license_blob = &try!(sector.mode2_xa_form1_payload())[0..76];

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
                    warn!("Couldn't identify disc region string: {}", license);
                    return Err(CdError::BadFormat);
                }
            };

        Ok(self)
    }
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

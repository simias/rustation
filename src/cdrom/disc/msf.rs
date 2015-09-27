use std::fmt;
use std::cmp;

/// CD "minute:second:frame" timestamp, given as 3 pairs of *BCD*
/// encoded bytes (4bits per digit). In this context "frame" is
/// synonymous with "sector".
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Msf(u8, u8, u8);

impl Msf {
    /// Create a 00:00:00 MSF timestamp
    pub fn zero() -> Msf {
        Msf(0, 0, 0)
    }

    pub fn from_bcd(m: u8, s: u8, f: u8) -> Msf {
        let msf = Msf(m, s, f);

        // Make sure we have valid BCD data
        for &b in &[m, s, f] {
            if b > 0x99 || (b & 0xf) > 0x9 {
                panic!("Invalid MSF: {}", msf);
            }
        }

        // Make sure the frame and seconds makes sense (there are only
        // 75 frames per second and obviously 60 seconds per minute)
        if s >= 0x60 || f >= 0x75 {
            panic!("Invalid MSF: {}", msf);
        }

        msf
    }

    /// Convert an MSF "coordinate" into a sector index. In this
    /// convention sector 0 is 00:00:00 (i.e. before track 01's
    /// pregap).
    pub fn sector_index(self) -> u32 {
        let from_bcd = |b| -> u8 {
            (b >> 4) * 10u8 + (b & 0xf)
        };

        let Msf(m, s, f) = self;

        let m = from_bcd(m) as u32;
        let s = from_bcd(s) as u32;
        let f = from_bcd(f) as u32;

        // 60 seconds in a minute, 75 sectors(frames) in a second
        (60 * 75 * m) + (75 * s) + f
    }

    /// Return the MSF timestamp of the next sector
    pub fn next(self) -> Msf {
        let Msf(m, s, f) = self;

        let bcd_inc = |b| {
            if b & 0xf < 9 {
                b + 1
            } else {
                (b & 0xf0) + 0x10
            }
        };

        if f < 0x74 {
            return Msf(m, s, bcd_inc(f))
        }

        if s < 0x59 {
            return Msf(m, bcd_inc(s), 0)
        }

        if m < 0x99 {
            return Msf(bcd_inc(m), 0, 0)
        }

        panic!("MSF overflow");
    }

    /// Pack the Msf in a single u32, makes it easier to do
    /// comparisons.
    fn as_u32_bcd(self) -> u32 {
        let Msf(m, s, f) = self;

        ((m as u32) << 16) | ((s as u32) << 8) | (f as u32)
    }
}

impl fmt::Display for Msf {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let Msf(m, s, f) = *self;

        write!(fmt, "{:02x}:{:02x}:{:02x}", m, s, f)
    }
}

impl cmp::PartialOrd for Msf {
    fn partial_cmp(&self, other: &Msf) -> Option<cmp::Ordering> {
        let a = self.as_u32_bcd();
        let b = other.as_u32_bcd();

        a.partial_cmp(&b)
    }
}

impl cmp::Ord for Msf {
    fn cmp(&self, other: &Msf) -> cmp::Ordering {
        let a = self.as_u32_bcd();
        let b = other.as_u32_bcd();

        a.cmp(&b)
    }
}

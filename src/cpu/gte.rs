//! Geometry Transform Engine (Coprocessor 2) emulation

pub struct Gte {
    /// Background color red component: signed 20.12
    rbk: i32,
    /// Background color green component: signed 20.12
    gbk: i32,
    /// Background color blue component: signed 20.12
    bbk: i32,
    /// Far color red component: signed 28.4
    rfc: i32,
    /// Far color green component: signed 28.4
    gfc: i32,
    /// Far color blue component: signed 28.4
    bfc: i32,
    /// Screen offset X: signed 16.16
    ofx: i32,
    /// Screen offset Y: signed 16.16
    ofy: i32,
    /// Projection plane distance (XXX reads back as a signed value
    /// even though unsigned)
    h: u16,
    /// Depth queing coeffient: signed 8.8
    dqa: i16,
    /// Depth queing offset: signed 8.24
    dqb: i32,
    /// Scale factor when computing the average of 3 Z values
    /// (triangle): signed 4.12.
    zsf3: i16,
    /// Scale factor when computing the average of 4 Z values
    /// (quad): signed 4.12
    zsf4: i16,
}

impl Gte {
    pub fn new() -> Gte {
        // It seems that none of the registers are reset even when I
        // reboot the whole system. So the initial register values
        // probably don't matter much.
        Gte {
            rbk: 0,
            gbk: 0,
            bbk: 0,
            rfc: 0,
            gfc: 0,
            bfc: 0,
            ofx: 0,
            ofy: 0,
            h: 0,
            dqa: 0,
            dqb: 0,
            zsf3: 0,
            zsf4: 0,
        }
    }

    /// Store value in one of the "control" registers. Used by the
    /// CTC2 opcode.
    pub fn set_control(&mut self, reg: u32, val: u32) {
        // XXX: on the real hardware it seems that there's a "store
        // delay" when setting a register in the GTE. See
        // https://github.com/simias/psx-rs/issues/5

        println!("Set GTE control {}: {:x}", reg, val);

        match reg {
            13 => self.rbk = val as i32,
            14 => self.gbk = val as i32,
            15 => self.bbk = val as i32,
            21 => self.rfc = val as i32,
            22 => self.gfc = val as i32,
            23 => self.bfc = val as i32,
            24 => self.ofx = val as i32,
            25 => self.ofy = val as i32,
            26 => self.h = val as u16,
            27 => self.dqa = val as i16,
            28 => self.dqb = val as i32,
            29 => self.zsf3 = val as i16,
            30 => self.zsf4 = val as i16,
            _  => panic!("Unhandled GTE control register {}", reg),
        }
    }
}

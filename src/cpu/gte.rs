//! Geometry Transform Engine (Coprocessor 2) emulation

pub struct Gte {
    // Control registers

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
    /// Translation vector: 3x signed word
    tr: [i32; 3],
    /// Light source matrix: 3x3 signed 4.12
    lsm: [[i16; 3]; 3],
    /// Light color matrix: 3x3 signed 4.12
    lcm: [[i16; 3]; 3],
    /// Rotation matrix: 3x3 signed 4.12
    rm: [[i16; 3]; 3],

    // Data registers

    /// Vector 0: 3x signed 4.12
    v0: [i16; 3],
    /// Vector 1: 3x signed 4.12
    v1: [i16; 3],
    /// Vector 2: 3x signed 4.12
    v2: [i16; 3],
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
            tr: [0; 3],
            rm: [[0; 3]; 3],
            lsm: [[0; 3]; 3],
            lcm: [[0; 3]; 3],
            v0: [0; 3],
            v1: [0; 3],
            v2: [0; 3],
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
            0 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.rm[0][0] = v0;
                self.rm[0][1] = v1;
            }
            1 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.rm[0][2] = v0;
                self.rm[1][0] = v1;
            }
            2 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.rm[1][1] = v0;
                self.rm[1][2] = v1;
            }
            3 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.rm[2][0] = v0;
                self.rm[2][1] = v1;
            }
            4 => self.rm[2][2] = val as i16,
            5 => self.tr[0] = val as i32,
            6 => self.tr[1] = val as i32,
            7 => self.tr[2] = val as i32,
            8 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lsm[0][0] = v0;
                self.lsm[0][1] = v1;
            }
            9 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lsm[0][2] = v0;
                self.lsm[1][0] = v1;
            }
            10 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lsm[1][1] = v0;
                self.lsm[1][2] = v1;
            }
            11 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lsm[2][0] = v0;
                self.lsm[2][1] = v1;
            }
            12 => self.lsm[2][2] = val as i16,
            13 => self.rbk = val as i32,
            14 => self.gbk = val as i32,
            15 => self.bbk = val as i32,
            16 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lcm[0][0] = v0;
                self.lcm[0][1] = v1;
            }
            17 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lcm[0][2] = v0;
                self.lcm[1][0] = v1;
            }
            18 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lcm[1][1] = v0;
                self.lcm[1][2] = v1;
            }
            19 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.lcm[2][0] = v0;
                self.lcm[2][1] = v1;
            }
            20 => self.lcm[2][2] = val as i16,
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
            _  => panic!("Unhandled GTE control register {} {:x}", reg, val),
        }
    }

    /// Store value in one of the "data" registers. Used by the
    /// MTC2 and LWC2 opcodes
    pub fn set_data(&mut self, reg: u32, val: u32) {

        println!("Set GTE data {}: {:x}", reg, val);

        match reg {
            0 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.v0[0] = v0;
                self.v0[1] = v1;
            }
            1 => self.v0[2] = val as i16,
            2 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.v1[0] = v0;
                self.v1[1] = v1;
            }
            3 => self.v1[2] = val as i16,
            4 => {
                let v0 = val as i16;
                let v1 = (val >> 16) as i16;

                self.v2[0] = v0;
                self.v2[1] = v1;
            }
            5 => self.v2[2] = val as i16,
            _  => panic!("Unhandled GTE data register {} {:x}", reg, val),
        }
    }

    /// Execute GTE command
    pub fn command(&mut self, command: u32) {
        panic!("Unhandled GTE command {:x}", command);
    }
}

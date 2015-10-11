use memory::{Addressable, AccessWidth};

/// Sound Processing Unit
pub struct Spu {
    /// Most of the SPU registers are not updated by the hardware,
    /// their value is just moved to the internal registers when
    /// needed. Therefore we can emulate those registers like a RAM of
    /// sorts.
    shadow_registers: [u16; 0x100],

    /// SPU RAM: 256k 16bit samples
    ram: [u16; 256 * 1024],
    /// Write pointer in the SPU RAM
    ram_index: u32,
}

impl Spu {
    pub fn new() -> Spu {
        Spu {
            shadow_registers: [0; 0x100],
            ram: [0xbad; 256 * 1024],
            ram_index: 0,
        }
    }

    pub fn store<T: Addressable>(&mut self, offset: u32, val: T) {
        if T::width() != AccessWidth::HalfWord {
            panic!("Unhandled {:?} SPU store", T::width());
        }

        let val = val.as_u16();

        // Convert into a halfword index
        let index = (offset >> 1) as usize;

        if index < 0xc0 {
            match index & 7 {
                regmap::voice::VOLUME_LEFT => (),
                regmap::voice::VOLUME_RIGHT => (),
                regmap::voice::ADPCM_SAMPLE_RATE => (),
                regmap::voice::ADPCM_START_INDEX => (),
                regmap::voice::ADPCM_ADSR_LOW => (),
                regmap::voice::ADPCM_ADSR_HIGH => (),
                // XXX change current volume?
                regmap::voice::CURRENT_ADSR_VOLUME => (),
                regmap::voice::ADPCM_REPEAT_INDEX => (),
                _ => unreachable!(),
            }
        } else {
            match index {
                regmap::MAIN_VOLUME_LEFT => (),
                regmap::MAIN_VOLUME_RIGHT => (),
                regmap::REVERB_VOLUME_LEFT => (),
                regmap::REVERB_VOLUME_RIGHT => (),
                regmap::VOICE_ON_LOW =>
                    self.shadow_registers[regmap::VOICE_STATUS_LOW] |= val,
                regmap::VOICE_ON_HIGH =>
                    self.shadow_registers[regmap::VOICE_STATUS_HIGH] |= val,
                regmap::VOICE_OFF_LOW =>
                    self.shadow_registers[regmap::VOICE_STATUS_LOW] &= !val,
                regmap::VOICE_OFF_HIGH =>
                    self.shadow_registers[regmap::VOICE_STATUS_HIGH] &= !val,
                regmap::VOICE_PITCH_MOD_EN_LOW => (),
                regmap::VOICE_PITCH_MOD_EN_HIGH => (),
                regmap::VOICE_NOISE_EN_LOW => (),
                regmap::VOICE_NOISE_EN_HIGH => (),
                regmap::VOICE_REVERB_EN_LOW => (),
                regmap::VOICE_REVERB_EN_HIGH => (),
                regmap::VOICE_STATUS_LOW => (),
                regmap::VOICE_STATUS_HIGH => (),
                regmap::REVERB_BASE => (),
                regmap::TRANSFER_START_INDEX =>
                    self.ram_index = (val as u32) << 2,
                regmap::TRANSFER_FIFO =>
                    self.fifo_write(val),
                regmap::CONTROL =>
                    self.set_control(val),
                regmap::TRANSFER_CONTROL =>
                    self.set_transfer_control(val),
                regmap::CD_VOLUME_LEFT => (),
                regmap::CD_VOLUME_RIGHT => (),
                regmap::EXT_VOLUME_LEFT => (),
                regmap::EXT_VOLUME_RIGHT => (),
                regmap::REVERB_APF_OFFSET1 => (),
                regmap::REVERB_APF_OFFSET2 => (),
                regmap::REVERB_REFLECT_VOLUME1 => (),
                regmap::REVERB_COMB_VOLUME1 => (),
                regmap::REVERB_COMB_VOLUME2 => (),
                regmap::REVERB_COMB_VOLUME3 => (),
                regmap::REVERB_COMB_VOLUME4 => (),
                regmap::REVERB_REFLECT_VOLUME2 => (),
                regmap::REVERB_APF_VOLUME1 => (),
                regmap::REVERB_APF_VOLUME2 => (),
                regmap::REVERB_REFLECT_SAME_LEFT1 => (),
                regmap::REVERB_REFLECT_SAME_RIGHT1 => (),
                regmap::REVERB_COMB_LEFT1 => (),
                regmap::REVERB_COMB_RIGHT1 => (),
                regmap::REVERB_COMB_LEFT2 => (),
                regmap::REVERB_COMB_RIGHT2 => (),
                regmap::REVERB_REFLECT_SAME_LEFT2 => (),
                regmap::REVERB_REFLECT_SAME_RIGHT2 => (),
                regmap::REVERB_REFLECT_DIFF_LEFT1 => (),
                regmap::REVERB_REFLECT_DIFF_RIGHT1 => (),
                regmap::REVERB_COMB_LEFT3 => (),
                regmap::REVERB_COMB_RIGHT3 => (),
                regmap::REVERB_COMB_LEFT4 => (),
                regmap::REVERB_COMB_RIGHT4 => (),
                regmap::REVERB_REFLECT_DIFF_LEFT2 => (),
                regmap::REVERB_REFLECT_DIFF_RIGHT2 => (),
                regmap::REVERB_APF_LEFT1 => (),
                regmap::REVERB_APF_RIGHT1 => (),
                regmap::REVERB_APF_LEFT2 => (),
                regmap::REVERB_APF_RIGHT2 => (),
                regmap::REVERB_INPUT_VOLUME_LEFT => (),
                regmap::REVERB_INPUT_VOLUME_RIGHT => (),
                _ => panic!("Unhandled SPU store {:x} {:04x}", offset, val),
            }
        }

        if index < 0x100 {
            self.shadow_registers[index] = val;
        }
    }

    pub fn load<T: Addressable>(&mut self, offset: u32) -> T {
        if T::width() != AccessWidth::HalfWord {
            panic!("Unhandled {:?} SPU load", T::width());
        }

        let index = (offset >> 1) as usize;

        let shadow = self.shadow_registers[index];

        // XXX This is a bit ugly but I use the match to "whitelist"
        // shadow registers as I encounter them. Once all registers
        // are correctly implemented we can default to the shadow.
        let r =
            if index < 0xc0 {
                match index & 7 {
                    regmap::voice::CURRENT_ADSR_VOLUME =>
                        // XXX return current volume
                        shadow,
                    regmap::voice::ADPCM_REPEAT_INDEX =>
                        // XXX return current repeat index
                        shadow,
                    _ => shadow,
                }
            } else {
                match (offset >> 1) as usize {
                    regmap::VOICE_ON_LOW => shadow,
                    regmap::VOICE_ON_HIGH => shadow,
                    regmap::VOICE_OFF_LOW => shadow,
                    regmap::VOICE_OFF_HIGH => shadow,
                    regmap::VOICE_REVERB_EN_LOW => shadow,
                    regmap::VOICE_REVERB_EN_HIGH => shadow,
                    regmap::VOICE_STATUS_LOW => shadow,
                    regmap::VOICE_STATUS_HIGH => shadow,
                    regmap::TRANSFER_START_INDEX => shadow,
                    regmap::CONTROL => shadow,
                    regmap::TRANSFER_CONTROL => shadow,
                    regmap::STATUS => self.status(),
                    regmap::CURRENT_VOLUME_LEFT =>
                        // XXX return current value
                        shadow,
                    regmap::CURRENT_VOLUME_RIGHT =>
                        // XXX return current value
                        shadow,
                    _ => panic!("Unhandled SPU load {:x}", offset),
                }
            };

        Addressable::from_u32(r as u32)
    }

    fn control(&self) -> u16 {
        self.shadow_registers[regmap::CONTROL]
    }

    fn set_control(&mut self, ctrl: u16) {
        if ctrl & 0x3f4a != 0 {
            panic!("Unhandled SPU control {:04x}", ctrl);
        }
    }

    fn status(&self) -> u16 {
        self.control() & 0x3f
    }

    /// Set the SPU RAM access pattern
    fn set_transfer_control(&self, val: u16) {
        // For now only support "normal" (i.e. sequential) access
        if val != 0x4 {
            panic!("Unhandled SPU RAM access pattern {:x}", val);
        }
    }

    fn fifo_write(&mut self, val: u16) {
        // XXX handle FIFO overflow?
        let index = self.ram_index;

        println!("SPU RAM store {:05x}: {:04x}", index, val);

        self.ram[index as usize] = val;
        self.ram_index = (index + 1) & 0x3ffff;
    }
}

mod regmap {
    //! SPU register map: offset from the base in number of
    //! *halfwords*

    pub mod voice {
        //! Per-voice regmap, repeated 24 times
        pub const VOLUME_LEFT:            usize = 0x0;
        pub const VOLUME_RIGHT:           usize = 0x1;
        pub const ADPCM_SAMPLE_RATE:      usize = 0x2;
        pub const ADPCM_START_INDEX:      usize = 0x3;
        pub const ADPCM_ADSR_LOW:         usize = 0x4;
        pub const ADPCM_ADSR_HIGH:        usize = 0x5;
        pub const CURRENT_ADSR_VOLUME:    usize = 0x6;
        pub const ADPCM_REPEAT_INDEX:     usize = 0x7;
    }

    pub const MAIN_VOLUME_LEFT:           usize = 0xc0;
    pub const MAIN_VOLUME_RIGHT:          usize = 0xc1;
    pub const REVERB_VOLUME_LEFT:         usize = 0xc2;
    pub const REVERB_VOLUME_RIGHT:        usize = 0xc3;
    pub const VOICE_ON_LOW:               usize = 0xc4;
    pub const VOICE_ON_HIGH:              usize = 0xc5;
    pub const VOICE_OFF_LOW:              usize = 0xc6;
    pub const VOICE_OFF_HIGH:             usize = 0xc7;
    pub const VOICE_PITCH_MOD_EN_LOW:     usize = 0xc8;
    pub const VOICE_PITCH_MOD_EN_HIGH:    usize = 0xc9;
    pub const VOICE_NOISE_EN_LOW:         usize = 0xca;
    pub const VOICE_NOISE_EN_HIGH:        usize = 0xcb;
    pub const VOICE_REVERB_EN_LOW:        usize = 0xcc;
    pub const VOICE_REVERB_EN_HIGH:       usize = 0xcd;
    pub const VOICE_STATUS_LOW:           usize = 0xce;
    pub const VOICE_STATUS_HIGH:          usize = 0xcf;

    pub const REVERB_BASE:                usize = 0xd1;
    pub const TRANSFER_START_INDEX:       usize = 0xd3;
    pub const TRANSFER_FIFO:              usize = 0xd4;
    pub const CONTROL:                    usize = 0xd5;
    pub const TRANSFER_CONTROL:           usize = 0xd6;
    pub const STATUS:                     usize = 0xd7;
    pub const CD_VOLUME_LEFT:             usize = 0xd8;
    pub const CD_VOLUME_RIGHT:            usize = 0xd9;
    pub const EXT_VOLUME_LEFT:            usize = 0xda;
    pub const EXT_VOLUME_RIGHT:           usize = 0xdb;
    pub const CURRENT_VOLUME_LEFT:        usize = 0xdc;
    pub const CURRENT_VOLUME_RIGHT:       usize = 0xdd;

    pub const REVERB_APF_OFFSET1:         usize = 0xe0;
    pub const REVERB_APF_OFFSET2:         usize = 0xe1;
    pub const REVERB_REFLECT_VOLUME1:     usize = 0xe2;
    pub const REVERB_COMB_VOLUME1:        usize = 0xe3;
    pub const REVERB_COMB_VOLUME2:        usize = 0xe4;
    pub const REVERB_COMB_VOLUME3:        usize = 0xe5;
    pub const REVERB_COMB_VOLUME4:        usize = 0xe6;
    pub const REVERB_REFLECT_VOLUME2:     usize = 0xe7;
    pub const REVERB_APF_VOLUME1:         usize = 0xe8;
    pub const REVERB_APF_VOLUME2:         usize = 0xe9;
    pub const REVERB_REFLECT_SAME_LEFT1:  usize = 0xea;
    pub const REVERB_REFLECT_SAME_RIGHT1: usize = 0xeb;
    pub const REVERB_COMB_LEFT1:          usize = 0xec;
    pub const REVERB_COMB_RIGHT1:         usize = 0xed;
    pub const REVERB_COMB_LEFT2:          usize = 0xee;
    pub const REVERB_COMB_RIGHT2:         usize = 0xef;
    pub const REVERB_REFLECT_SAME_LEFT2:  usize = 0xf0;
    pub const REVERB_REFLECT_SAME_RIGHT2: usize = 0xf1;
    pub const REVERB_REFLECT_DIFF_LEFT1:  usize = 0xf2;
    pub const REVERB_REFLECT_DIFF_RIGHT1: usize = 0xf3;
    pub const REVERB_COMB_LEFT3:          usize = 0xf4;
    pub const REVERB_COMB_RIGHT3:         usize = 0xf5;
    pub const REVERB_COMB_LEFT4:          usize = 0xf6;
    pub const REVERB_COMB_RIGHT4:         usize = 0xf7;
    pub const REVERB_REFLECT_DIFF_LEFT2:  usize = 0xf8;
    pub const REVERB_REFLECT_DIFF_RIGHT2: usize = 0xf9;
    pub const REVERB_APF_LEFT1:           usize = 0xfa;
    pub const REVERB_APF_RIGHT1:          usize = 0xfb;
    pub const REVERB_APF_LEFT2:           usize = 0xfc;
    pub const REVERB_APF_RIGHT2:          usize = 0xfd;
    pub const REVERB_INPUT_VOLUME_LEFT:   usize = 0xfe;
    pub const REVERB_INPUT_VOLUME_RIGHT:  usize = 0xff;
}

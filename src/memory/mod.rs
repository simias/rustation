pub mod bios;
mod ram;

use self::bios::Bios;
use self::ram::Ram;

/// Global interconnect
pub struct Interconnect {
    /// Basic Input/Output memory
    bios: Bios,
    /// Main RAM
    ram: Ram,
}

impl Interconnect {
    pub fn new(bios: Bios) -> Interconnect {
        Interconnect {
            bios: bios,
            ram:  Ram::new(),
        }
    }

    /// Load 32bit word at `addr`
    pub fn load32(&self, addr: u32) -> u32 {

        if addr % 4 != 0 {
            panic!("Unaligned load32 address: {:08x}", addr);
        }

        let abs_addr = map::mask_region(addr);

        if let Some(offset) = map::BIOS.contains(abs_addr) {
            return self.bios.load32(offset);
        }

        if let Some(offset) = map::RAM.contains(abs_addr) {
            return self.ram.load32(offset);
        }

        panic!("unhandled load32 at address {:08x}", addr);
    }

    /// Store 32bit word `val` into `addr`
    pub fn store32(&mut self, addr: u32, val: u32) {

        if addr % 4 != 0 {
            panic!("Unaligned store32 address: {:08x}", addr);
        }

        let abs_addr = map::mask_region(addr);

        if let Some(offset) = map::RAM.contains(abs_addr) {
            return self.ram.store32(offset, val);
        }

        if let Some(_) = map::CACHE_CONTROL.contains(abs_addr) {
            println!("Unhandled write to CACHE_CONTROL: {:08x}", val);
            return;
        }

        if let Some(offset) = map::MEM_CONTROL.contains(abs_addr) {
            match offset {
                0 => // Expansion 1 base address
                    if val != 0x1f000000 {
                        panic!("Bad expansion 1 base address: 0x{:08x}", val);
                    },
                4 => // Expansion 2 base address
                    if val != 0x1f802000 {
                        panic!("Bad expansion 2 base address: 0x{:08x}", val);
                    },
                _ =>
                    println!("Unhandled write to MEM_CONTROL register {:x}: \
                              0x{:08x}",
                             offset, val),
            }
            return;
        }

        if let Some(_) = map::RAM_SIZE.contains(abs_addr) {
            // We ignore writes at this address
            return;
        }

        panic!("unhandled store32 into address {:08x}", addr);
    }

    /// Store 16bit halfword `val` into `addr`
    pub fn store16(&mut self, addr: u32, _: u16) {

        if addr % 2 != 0 {
            panic!("Unaligned store16 address: {:08x}", addr);
        }

        let abs_addr = map::mask_region(addr);

        if let Some(offset) = map::SPU.contains(abs_addr) {
            println!("Unhandled write to SPU register {:x}", offset);
            return;
        }

        panic!("unhandled store16 into address {:08x}", addr);
    }

    /// Store byte `val` into `addr`
    pub fn store8(&mut self, addr: u32, _: u8) {
        let abs_addr = map::mask_region(addr);

        if let Some(offset) = map::EXPANSION_2.contains(abs_addr) {
            println!("Unhandled write to expansion 2 register {:x}", offset);
            return;
        }

        panic!("unhandled store8 into address {:08x}", addr);
    }
}

mod map {
    pub struct Range(u32, u32);

    impl Range {
        /// Return `Some(offset)` if addr is contained in `self`
        pub fn contains(self, addr: u32) -> Option<u32> {
            let Range(start, length) = self;

            if addr >= start && addr < start + length {
                Some(addr - start)
            } else {
                None
            }
        }
    }

    /// Mask array used to strip the region bits of the address. The
    /// mask is selected using the 3 MSBs of the address so each entry
    /// effectively matches 512kB of the address space. KSEG2 is not
    /// touched since it doesn't share anything with the other
    /// regions.
    const REGION_MASK: [u32; 8] = [
        0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, // KUSEG: 2048MB
        0x7fffffff,                                     // KSEG0:  512MB
        0x1fffffff,                                     // KSEG1:  512MB
        0xffffffff, 0xffffffff,                         // KSEG2: 1024MB
        ];

    /// Mask a CPU address to remove the region bits.
    pub fn mask_region(addr: u32) -> u32 {
        // Index address space in 512MB chunks
        let index = (addr >> 29) as usize;

        addr & REGION_MASK[index]
    }

    pub const RAM: Range = Range(0x00000000, 2 * 1024 * 1024);

    pub const BIOS: Range = Range(0x1fc00000, 512 * 1024);

    /// Memory latency and expansion mapping
    pub const MEM_CONTROL: Range = Range(0x1f801000, 36);

    /// Register that has something to do with RAM configuration,
    /// configured by the BIOS
    pub const RAM_SIZE: Range = Range(0x1f801060, 4);

    /// SPU registers
    pub const SPU: Range = Range(0x1f801c00, 640);

    /// Expansion region 2
    pub const EXPANSION_2: Range = Range(0x1f802000, 66);

    /// Cache control register. Full address since it's in KSEG2
    pub const CACHE_CONTROL: Range = Range(0xfffe0130, 4);
}

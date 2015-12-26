pub mod timers;
mod ram;
mod dma;

use self::ram::{Ram, ScratchPad};
use self::dma::{Dma, Port, Direction, Step, Sync};
use self::timers::Timers;

use shared::SharedState;
use bios::Bios;
use timekeeper::Peripheral;
use gpu::Gpu;
use gpu::renderer::Renderer;
use spu::Spu;
use cdrom::CdRom;
use cdrom::disc::Disc;
use padmemcard::PadMemCard;
use padmemcard::gamepad;

/// Global interconnect
pub struct Interconnect {
    /// Basic Input/Output memory
    bios: Bios,
    /// Main RAM
    ram: Ram,
    /// ScratchPad
    scratch_pad: ScratchPad,
    /// DMA registers
    dma: Dma,
    /// Graphics Processor Unit
    gpu: Gpu,
    /// Sound Processing Unit
    spu: Spu,
    /// System timers
    timers: Timers,
    /// Cache Control register
    cache_control: CacheControl,
    /// CDROM controller
    cdrom: CdRom,
    /// Gamepad and memory card controller
    pad_memcard: PadMemCard,
    /// Contents of the RAM_SIZE register which is probably a
    /// configuration register for the memory controller.
    ram_size: u32,
    /// Memory control registers
    mem_control: [u32; 9],
}

impl Interconnect {
    pub fn new(bios: Bios, gpu: Gpu, disc: Option<Disc>) -> Interconnect {
        Interconnect {
            bios: bios,
            ram: Ram::new(),
            scratch_pad: ScratchPad::new(),
            dma: Dma::new(),
            gpu: gpu,
            spu: Spu::new(),
            timers: Timers::new(),
            cache_control: CacheControl(0),
            cdrom: CdRom::new(disc),
            pad_memcard: PadMemCard::new(),
            ram_size: 0,
            mem_control: [0; 9],
        }
    }

    pub fn sync(&mut self, shared: &mut SharedState) {
        if shared.tk().needs_sync(Peripheral::Gpu) {
            self.gpu.sync(shared);
        }

        if shared.tk().needs_sync(Peripheral::PadMemCard) {
            self.pad_memcard.sync(shared);
        }

        self.timers.sync(shared);

        if shared.tk().needs_sync(Peripheral::CdRom) {
            self.cdrom.sync(shared);
        }
    }

    pub fn cache_control(&self) -> CacheControl {
        self.cache_control
    }

    pub fn pad_profiles(&mut self) -> [&mut gamepad::Profile; 2] {
        self.pad_memcard.pad_profiles()
    }

    /// Interconnect: load instruction at `PC`. Only the RAM and BIOS
    /// are supported, would it make sense to fetch instructions from
    /// anything else?
    pub fn load_instruction(&self, pc: u32) -> u32 {
        let abs_addr = map::mask_region(pc);

        if let Some(offset) = map::RAM.contains(abs_addr) {
            return self.ram.load::<Word>(offset);
        }

        if let Some(offset) = map::BIOS.contains(abs_addr) {
            return self.bios.load::<Word>(offset);
        }

        panic!("unhandled instruction load at address {:08x}", pc);
    }

    /// Interconnect: load value at `addr`
    pub fn load<T: Addressable>(&mut self,
                                shared: &mut SharedState,
                                addr: u32) -> u32 {
        // XXX Average RAM load delay, needs to do per-device tests
        // XXX This does not take the CPU pipelining into account so
        // it might be a little too slow in some cases actually.
        shared.tk().tick(5);

        let abs_addr = map::mask_region(addr);

        if let Some(offset) = map::RAM.contains(abs_addr) {
            return self.ram.load::<T>(offset);
        }

        if let Some(offset) = map::SCRATCH_PAD.contains(abs_addr) {
            if addr > 0xa0000000 {
                panic!("ScratchPad access through uncached memory");
            }

            return self.scratch_pad.load::<T>(offset);
        }

        if let Some(offset) = map::BIOS.contains(abs_addr) {
            return self.bios.load::<T>(offset);
        }

        if let Some(offset) = map::IRQ_CONTROL.contains(abs_addr) {
            return
                match offset {
                    0 => shared.irq_state().status() as u32,
                    4 => shared.irq_state().mask() as u32,
                    _ => panic!("Unhandled IRQ load at address {:08x}", addr),
                };
        }

        if let Some(offset) = map::DMA.contains(abs_addr) {
            return self.dma_reg::<T>(offset);
        }

        if let Some(offset) = map::GPU.contains(abs_addr) {
            return self.gpu.load::<T>(shared, offset);
        }

        if let Some(offset) = map::TIMERS.contains(abs_addr) {
            return self.timers.load::<T>(shared, offset);
        }

        if let Some(offset) = map::CDROM.contains(abs_addr) {
            return self.cdrom.load::<T>(shared, offset);
        }

        if let Some(offset) = map::MDEC.contains(abs_addr) {
            warn!("Unhandled load from MDEC register {:x}", offset);
            return 0;
        }

        if let Some(offset) = map::SPU.contains(abs_addr) {
            return self.spu.load::<T>(offset);
        }

        if let Some(offset) = map::PAD_MEMCARD.contains(abs_addr) {
            return self.pad_memcard.load::<T>(shared, offset);
        }

        if let Some(_) = map::EXPANSION_1.contains(abs_addr) {
            // No expansion implemented. Returns full ones when no
            // expansion is present
            return !0;
        }

        if let Some(_) = map::RAM_SIZE.contains(abs_addr) {
            return self.ram_size;
        }

        if let Some(offset) = map::MEM_CONTROL.contains(abs_addr) {

            if T::size() != 4 {
                panic!("Unhandled MEM_CONTROL access ({})", T::size());
            }

            let index = (offset >> 2) as usize;

            return self.mem_control[index];
        }

        panic!("unhandled load at address {:08x}", addr);
    }

    /// Interconnect: store `val` into `addr`
    pub fn store<T: Addressable>(&mut self,
                                 shared: &mut SharedState,
                                 renderer: &mut Renderer,
                                 addr: u32,
                                 val: u32) {

        let abs_addr = map::mask_region(addr);

        if let Some(offset) = map::RAM.contains(abs_addr) {
            self.ram.store::<T>(offset, val);
            return;
        }

        if let Some(offset) = map::SCRATCH_PAD.contains(abs_addr) {
            if addr > 0xa0000000 {
                panic!("ScratchPad access through uncached memory");
            }

            return self.scratch_pad.store::<T>(offset, val);
        }

        if let Some(offset) = map::IRQ_CONTROL.contains(abs_addr) {
            match offset {
                0 => shared.irq_state().ack(val as u16),
                4 => shared.irq_state().set_mask(val as u16),
                _ => panic!("Unhandled IRQ store at address {:08x}"),
            }
            return;
        }

        if let Some(offset) = map::DMA.contains(abs_addr) {
            self.set_dma_reg::<T>(shared, renderer, offset, val);
            return;
        }

        if let Some(offset) = map::GPU.contains(abs_addr) {
            self.gpu.store::<T>(shared,
                                renderer,
                                &mut self.timers,
                                offset,
                                val);
            return;
        }

        if let Some(offset) = map::TIMERS.contains(abs_addr) {
            self.timers.store::<T>(shared,
                                   &mut self.gpu,
                                   offset,
                                   val);
            return;
        }

        if let Some(offset) = map::CDROM.contains(abs_addr) {
            return self.cdrom.store::<T>(shared, offset, val);
        }

        if let Some(offset) = map::MDEC.contains(abs_addr) {
            warn!("Unhandled write to MDEC register {:x}", offset);
            return;
        }

        if let Some(offset) = map::SPU.contains(abs_addr) {
            self.spu.store::<T>(offset, val);
            return;
        }

        if let Some(offset) = map::PAD_MEMCARD.contains(abs_addr) {
            self.pad_memcard.store::<T>(shared, offset, val);
            return;
        }

        if let Some(_) = map::CACHE_CONTROL.contains(abs_addr) {
            if T::size() != 4 {
                panic!("Unhandled cache control access");
            }

            self.cache_control = CacheControl(val);

            return;
        }

        if let Some(offset) = map::MEM_CONTROL.contains(abs_addr) {

            if T::size() != 4 {
                panic!("Unhandled MEM_CONTROL access ({})", T::size());
            }

            let val = val;

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
                    warn!("Unhandled write to MEM_CONTROL register {:x}: \
                           0x{:08x}",
                          offset, val),
            }

            let index = (offset >> 2) as usize;

            self.mem_control[index] = val;

            return;
        }

        if let Some(_) = map::RAM_SIZE.contains(abs_addr) {

            if T::size() != 4 {
                panic!("Unhandled RAM_SIZE access");
            }

            self.ram_size = val;
            return;
        }

        if let Some(offset) = map::EXPANSION_2.contains(abs_addr) {
            warn!("Unhandled write to expansion 2 register {:x}", offset);
            return;
        }

        panic!("unhandled store into address {:08x}: {:08x}",
               addr, val);
    }

    /// DMA register read
    fn dma_reg<T: Addressable>(&self, offset: u32) -> u32 {

        // The DMA uses 32bit registers
        let align = offset & 3;
        let offset = offset & !3;

        let major = (offset & 0x70) >> 4;
        let minor = offset & 0xf;

        let res =
            match major {
                // Per-channel registers
                0...6 => {
                    let channel = self.dma.channel(Port::from_index(major));

                    match minor {
                        0 => channel.base(),
                        4 => channel.block_control(),
                        8 => channel.control(),
                        _ => panic!("Unhandled DMA read at {:x}", offset)
                    }
                },
                // Common DMA registers
                7 => match minor {
                    0 => self.dma.control(),
                    4 => self.dma.interrupt(),
                    _ => panic!("Unhandled DMA read at {:x}", offset)
                },
                _ => panic!("Unhandled DMA read at {:x}", offset)
            };

        // Byte and halfword reads fetch only a portion of the register
        res >> (align * 8)
    }

    /// DMA register write
    fn set_dma_reg<T: Addressable>(&mut self,
                                   shared: &mut SharedState,
                                   renderer: &mut Renderer,
                                   offset: u32,
                                   val: u32) {
        // Byte and Halfword writes are treated like word writes with
        // the *entire* Word value shifted by the alignment.
        let align = offset & 3;
        let val = val << (align * 8);
        let offset = offset & !3;

        let major = (offset & 0x70) >> 4;
        let minor = offset & 0xf;

        let active_port =
            match major {
                // Per-channel registers
                0...6 => {
                    let port = Port::from_index(major);
                    let channel = self.dma.channel_mut(port);

                    match minor {
                        0 => channel.set_base(val),
                        4 => channel.set_block_control(val),
                        8 => channel.set_control(val),
                        _ => panic!("Unhandled DMA write {:x}: {:08x}",
                                    offset, val)
                    }

                    if channel.active() {
                        Some(port)
                    } else {
                        None
                    }
                },
                // Common DMA registers
                7 => {
                    match minor {
                        0 => self.dma.set_control(val),
                        4 => self.dma.set_interrupt(shared, val),
                        _ => panic!("Unhandled DMA write {:x}: {:08x}",
                                    offset, val),
                    }

                    None
                }
                _ => panic!("Unhandled DMA write {:x}: {:08x}",
                            offset, val),
            };

        if let Some(port) = active_port {
            self.do_dma(shared, renderer, port);
        }
    }

    /// Execute DMA transfer for a port
    fn do_dma(&mut self,
              shared: &mut SharedState,
              renderer: &mut Renderer,
              port: Port) {
        // DMA transfer has been started, for now let's
        // process everything in one pass (i.e. no
        // chopping or priority handling)

        match self.dma.channel(port).sync() {
                Sync::LinkedList => self.do_dma_linked_list(renderer, port),
                _                => self.do_dma_block(renderer, port),
        }

        self.dma.done(shared, port);
    }

    /// Emulate DMA transfer for linked list synchronization mode.
    fn do_dma_linked_list(&mut self, renderer: &mut Renderer, port: Port) {
        let channel = self.dma.channel_mut(port);

        let mut addr = channel.base() & 0x1ffffc;

        if channel.direction() == Direction::ToRam {
            panic!("Invalid DMA direction for linked list mode");
        }

        // I don't know if the DMA even supports linked list mode for
        // anything besides the GPU
        if port != Port::Gpu {
            panic!("Attempted linked list DMA on port {:?}", port);
        }

        loop {
            // In linked list mode, each entry starts with a "header"
            // word. The high byte contains the number of words in the
            // "packet" (not counting the header word)
            let header = self.ram.load::<Word>(addr);

            let mut remsz = header >> 24;

            while remsz > 0 {
                addr = (addr + 4) & 0x1ffffc;

                let command = self.ram.load::<Word>(addr);

                // Send command to the GPU
                self.gpu.gp0(renderer, command);

                remsz -= 1;
            }

            // The end-of-table marker is usually 0xffffff but
            // mednafen only checks for the MSB so maybe that's what
            // the hardware does? Since this bit is not part of any
            // valid address it makes some sense. I'll have to test
            // that at some point...
            if header & 0x800000 != 0 {
                break;
            }

            addr = header & 0x1ffffc;
        }
    }

    /// Emulate DMA transfer for Manual and Request synchronization
    /// modes.
    fn do_dma_block(&mut self, renderer: &mut Renderer, port: Port) {
        let channel = self.dma.channel_mut(port);

        let increment = match channel.step() {
            Step::Increment =>  4,
            Step::Decrement => -4i32 as u32,
        };

        let mut addr = channel.base();

        // Transfer size in words
        let mut remsz = match channel.transfer_size() {
            Some(n) => n,
            // Shouldn't happen since we shouldn't be reaching this code
            // in linked list mode
            None    => panic!("Couldn't figure out DMA block transfer size"),
        };

        while remsz > 0 {
            // Not sure what happens if address is
            // bogus... Mednafen just masks addr this way, maybe
            // that's how the hardware behaves (i.e. the RAM
            // address wraps and the two LSB are ignored, seems
            // reasonable enough
            let cur_addr = addr & 0x1ffffc;

            match channel.direction() {
                Direction::FromRam => {
                    let src_word = self.ram.load::<Word>(cur_addr);

                    match port {
                        Port::Gpu => self.gpu.gp0(renderer, src_word),
                        // XXX ignore transfers to the MDEC for now
                        Port::MDecIn => (),
                        // XXX ignre transfers to the SPU for now
                        Port::Spu => (),
                        _ => panic!("Unhandled DMA destination port {:?}",
                                    port),
                    }
                }
                Direction::ToRam => {
                    let src_word = match port {
                        // Clear ordering table
                        Port::Otc => match remsz {
                            // Last entry contains the end
                            // of table marker
                            1 => 0xffffff,
                            // Pointer to the previous entry
                            _ => addr.wrapping_sub(4) & 0x1fffff,
                        },
                        // XXX to be implemented
                        Port::MDecOut => 0,
                        Port::Gpu => {
                            // XXX to be implemented
                            debug!("DMA GPU READ");
                            0
                        }
                        Port::CdRom => self.cdrom.dma_read_word(),
                        _ => panic!("Unhandled DMA source port {:?}", port),
                    };

                    self.ram.store::<Word>(cur_addr, src_word);
                }
            }

            addr = addr.wrapping_add(increment);
            remsz -= 1;
        }
    }
}

#[derive(Clone,Copy)]
pub struct CacheControl(u32);

impl CacheControl {

    /// Return true if the instruction cache is enabled
    pub fn icache_enabled(self) -> bool {
        self.0 & 0x800 != 0
    }

    pub fn tag_test_mode(self) -> bool {
        self.0 & 4 != 0
    }
}

/// Trait representing the attributes of a memory access
pub trait Addressable {
    /// Retreive the size of the access in bytes
    fn size() -> u8;
}

/// Marker for Byte (8bit) access
pub struct Byte;

impl Addressable for Byte {
    fn size() -> u8 {
        1
    }
}

/// Marker for Halfword (16bit) access
pub struct HalfWord;

impl Addressable for HalfWord {
    fn size() -> u8 {
        2
    }
}

/// Marker for Word (32bit) access
pub struct Word;

impl Addressable for Word {
    fn size() -> u8 {
        4
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

    /// Main RAM: 2MB mirrored four times over the first 8MB (probably
    /// in case they decided to use a bigger RAM later on?)
    pub const RAM: Range = Range(0x00000000, 8 * 1024 * 1024);

    /// Expansion region 1
    pub const EXPANSION_1: Range = Range(0x1f000000, 512 * 1024);

    pub const BIOS: Range = Range(0x1fc00000, 512 * 1024);

    /// ScratchPad: data cache used as a fast 1kB RAM
    pub const SCRATCH_PAD: Range = Range(0x1f800000, 1024);

    /// Memory latency and expansion mapping
    pub const MEM_CONTROL: Range = Range(0x1f801000, 36);

    /// Gamepad and memory card controller
    pub const PAD_MEMCARD: Range = Range(0x1f801040, 32);

    /// Register that has something to do with RAM configuration,
    /// configured by the BIOS
    pub const RAM_SIZE: Range = Range(0x1f801060, 4);

    /// Interrupt Control registers (status and mask)
    pub const IRQ_CONTROL: Range = Range(0x1f801070, 8);

    /// Direct Memory Access registers
    pub const DMA: Range = Range(0x1f801080, 0x80);

    pub const TIMERS: Range = Range(0x1f801100, 0x30);

    /// CDROM controller
    pub const CDROM: Range = Range(0x1f801800, 0x4);

    pub const GPU: Range = Range(0x1f801810, 8);

    pub const MDEC: Range = Range(0x1f801820, 8);

    /// SPU registers
    pub const SPU: Range = Range(0x1f801c00, 640);

    /// Expansion region 2
    pub const EXPANSION_2: Range = Range(0x1f802000, 66);

    /// Cache control register. Full address since it's in KSEG2
    pub const CACHE_CONTROL: Range = Range(0xfffe0130, 4);
}

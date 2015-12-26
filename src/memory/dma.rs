use shared::SharedState;
use interrupt::Interrupt;

/// Direct Memory Access
pub struct Dma {
    /// DMA control register
    control: u32,
    /// master IRQ enable
    irq_en: bool,
    /// IRQ enable for individual channels
    channel_irq_en: u8,
    /// IRQ flags for individual channels
    channel_irq_flags: u8,
    /// When set the interrupt is active unconditionally (even if
    /// `irq_en` is false)
    force_irq: bool,
    /// Bits [0:5] of the interrupt registers are RW but I don't know
    /// what they're supposed to do so I just store them and send them
    /// back untouched on reads
    irq_dummy: u8,
    /// The 7 channel instances
    channels: [Channel; 7],
}

impl Dma {
    pub fn new() -> Dma {
        Dma {
            // Reset value taken from the Nocash PSX spec
            control: 0x07654321,
            irq_en: false,
            channel_irq_en: 0,
            channel_irq_flags: 0,
            force_irq: false,
            irq_dummy: 0,
            channels: [ Channel::new(); 7 ],
        }
    }

    /// Return the status of the DMA interrupt
    fn irq(&self) -> bool {
        let channel_irq = self.channel_irq_flags & self.channel_irq_en;

        self.force_irq || (self.irq_en && channel_irq != 0)
    }

    /// Retrieve the value of the control register
    pub fn control(&self) -> u32 {
        self.control
    }

    /// Set the value of the control register
    pub fn set_control(&mut self, val: u32) {
        self.control = val
    }

    /// Retrieve the value of the interrupt register
    pub fn interrupt(&self) -> u32 {
        let mut r = 0;

        r |= self.irq_dummy as u32;
        r |= (self.force_irq as u32) << 15;
        r |= (self.channel_irq_en as u32) << 16;
        r |= (self.irq_en as u32) << 23;
        r |= (self.channel_irq_flags as u32) << 24;
        r |= (self.irq() as u32) << 31;

        r
    }

    /// Set the value of the interrupt register
    pub fn set_interrupt(&mut self,
                         shared: &mut SharedState,
                         val: u32) {
        let prev_irq = self.irq();

        // Unknown what bits [5:0] do
        self.irq_dummy = (val & 0x3f) as u8;

        self.force_irq = (val >> 15) & 1 != 0;

        // XXX I don't think disabling the channel IRQ clears the
        // interrupt in channel_irq_flags but I should check that.
        self.channel_irq_en = ((val >> 16) & 0x7f) as u8;

        self.irq_en = (val >> 23) & 1 != 0;

        // Writing 1 to a flag resets it
        let ack = ((val >> 24) & 0x3f) as u8;
        self.channel_irq_flags &= !ack;

        if !prev_irq && self.irq() {
            // Rising edge of the done interrupt
            shared.irq_state().assert(Interrupt::Dma);
        }
    }

    /// Return a reference to a channel by port number.
    pub fn channel(&self, port: Port) -> &Channel {
        &self.channels[port as usize]
    }

    /// Return a mutable reference to a channel by port number.
    pub fn channel_mut(&mut self, port: Port) -> &mut Channel {
        &mut self.channels[port as usize]
    }

    pub fn done(&mut self,
                shared: &mut SharedState,
                port: Port) {

        self.channel_mut(port).done();

        let prev_irq = self.irq();

        // Set interrupt flag if the channel's interrupt is enabled
        let it_en = self.channel_irq_en & (1 << (port as usize));

        self.channel_irq_flags |= it_en;

        if !prev_irq && self.irq() {
            // Rising edge of the done interrupt
            shared.irq_state().assert(Interrupt::Dma);
        }
    }
}

/// Per-channel data
#[derive(Clone,Copy)]
struct Channel {
    /// If true the channel is enabled and the copy can take place
    /// depending on the condition mandated by the `sync` mode.
    enable: bool,
    /// Copy direction: from RAM or from device
    direction: Direction,
    /// DMA can either increment or decrement the RAM pointer after
    /// each copy
    step: Step,
    /// Synchronization mode
    sync: Sync,
    /// Used to start the DMA transfer when `sync` is `Manual`
    trigger: bool,
    /// If true the DMA "chops" the transfer and lets the CPU run in
    /// the gaps.
    chop: bool,
    /// Chopping DMA window size (log2 number of words)
    chop_dma_sz: u8,
    /// Chopping CPU window size (log2 number of cycles)
    chop_cpu_sz: u8,
    /// DMA start address
    base: u32,
    /// Size of a block in words
    block_size: u16,
    /// Block count, only used when `sync` is `Request`
    block_count: u16,
    /// Unkown 2 RW bits in configuration register
    dummy: u8,
}

impl Channel {
    fn new() -> Channel {
        Channel {
            enable: false,
            direction: Direction::ToRam,
            step: Step::Increment,
            sync: Sync::Manual,
            trigger: false,
            chop: false,
            chop_dma_sz: 0,
            chop_cpu_sz: 0,
            base: 0,
            block_size: 0,
            block_count: 0,
            dummy: 0,
        }
    }

    /// Retrieve the channel's base address
    pub fn base(&self) -> u32 {
        self.base
    }

    /// Set channel base address. Only bits [0:23] are significant so
    /// only 16MB are addressable by the DMA
    pub fn set_base(&mut self, val: u32) {
        self.base = val & 0xffffff;
    }

    /// Retrieve the value of the control register
    pub fn control(&self) -> u32 {
        let mut r = 0;

        r |= (self.direction as u32) << 0;
        r |= (self.step as u32) << 1;
        r |= (self.chop as u32) << 8;
        r |= (self.sync as u32) << 9;
        r |= (self.chop_dma_sz as u32) << 16;
        r |= (self.chop_cpu_sz as u32) << 20;
        r |= (self.enable as u32) << 24;
        r |= (self.trigger as u32) << 28;
        r |= (self.dummy as u32) << 29;

        r
    }

    /// Set the value of the control register
    pub fn set_control(&mut self, val: u32) {

        self.direction = match val & 1 != 0 {
            true  => Direction::FromRam,
            false => Direction::ToRam,
        };

        self.step = match (val >> 1) & 1 != 0 {
            true  => Step::Decrement,
            false => Step::Increment,
        };

        self.chop = (val >> 8) & 1 != 0;

        self.sync = match (val >> 9) & 3 {
            0 => Sync::Manual,
            1 => Sync::Request,
            2 => Sync::LinkedList,
            n => panic!("Unknown DMA sync mode {}", n),
        };

        self.chop_dma_sz = ((val >> 16) & 7) as u8;
        self.chop_cpu_sz = ((val >> 20) & 7) as u8;

        self.enable  = (val >> 24) & 1 != 0;
        self.trigger = (val >> 28) & 1 != 0;

        self.dummy = ((val >> 29) & 3) as u8;
    }

    /// Retrieve value of the Block Control register
    pub fn block_control(&self) -> u32 {
        let bs = self.block_size as u32;
        let bc = self.block_count as u32;

        (bc << 16) | bs
    }

    /// Set value of the Block Control register
    pub fn set_block_control(&mut self, val: u32) {
        self.block_size = val as u16;
        self.block_count = (val >> 16) as u16;
    }

    /// Return true if the channel has been started
    pub fn active(&self) -> bool {
        // In manual sync mode the CPU must set the "trigger" bit to
        // start the transfer.
        let trigger = match self.sync {
            Sync::Manual => self.trigger,
            _            => true,
        };

        self.enable && trigger
    }

    /// Set the channel status to "completed" state
    fn done(&mut self) {
        self.enable = false;
        self.trigger = false;
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn step(&self) -> Step {
        self.step
    }

    pub fn sync(&self) -> Sync {
        self.sync
    }

    /// Return the DMA transfer size in bytes or None for linked list
    /// mode.
    pub fn transfer_size(&self) -> Option<u32> {
        let bs = self.block_size as u32;
        let bc = self.block_count as u32;

        match self.sync {
            // For manual mode only the block size is used
            Sync::Manual => Some(bs),
            // In DMA request mode we must transfer `bc` blocks
            Sync::Request => Some(bc * bs),
            // In linked list mode the size is not known ahead of
            // time: we stop when we encounter the "end of list"
            // marker (0xffffff)
            Sync::LinkedList => None,
        }
    }
}

/// DMA transfer direction
#[derive(Clone,Copy,PartialEq,Eq)]
pub enum Direction {
    ToRam   = 0,
    FromRam = 1,
}

/// DMA transfer step
#[derive(Clone,Copy)]
pub enum Step {
    Increment = 0,
    Decrement = 1,
}

/// DMA transfer synchronization mode
#[derive(Clone,Copy)]
pub enum Sync {
    /// Transfer starts when the CPU writes to the Trigger bit and
    /// transfers everything at once
    Manual = 0,
    /// Sync blocks to DMA requests
    Request = 1,
    /// Used to transfer GPU command lists
    LinkedList = 2,
}

/// The 7 DMA ports
#[derive(Clone,Copy,PartialEq,Eq,Debug)]
pub enum Port {
    /// Macroblock decoder input
    MDecIn = 0,
    /// Macroblock decoder output
    MDecOut = 1,
    /// Graphics Processing Unit
    Gpu = 2,
    /// CD-ROM drive
    CdRom = 3,
    /// Sound Processing Unit
    Spu = 4,
    /// Extension port
    Pio = 5,
    /// Used to clear the ordering table
    Otc = 6,
}

impl Port {
    pub fn from_index(index: u32) -> Port {
        match index {
            0 => Port::MDecIn,
            1 => Port::MDecOut,
            2 => Port::Gpu,
            3 => Port::CdRom,
            4 => Port::Spu,
            5 => Port::Pio,
            6 => Port::Otc,
            n => panic!("Invalid port {}", n),
        }
    }
}

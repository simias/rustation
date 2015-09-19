/// The PlayStation supports 10 interrupts
#[derive(Clone, Copy, Debug)]
pub enum Interrupt {
    /// Display in vertical blanking
    VBlank = 0,
    /// CDROM controller
    CdRom = 2,
    /// DMA transfer done
    Dma = 3,
    /// Timer0 interrupt
    Timer0 = 4,
    /// Timer1 interrupt
    Timer1 = 5,
    /// Timer2 interrupt
    Timer2 = 6,
    /// Gamepad and Memory Card controller interrupt
    PadMemCard = 7,
}

#[derive(Clone,Copy)]
pub struct InterruptState {
    /// Interrupt status
    status: u16,
    /// Interrupt mask
    mask: u16,
}

impl InterruptState {

    pub fn new() -> InterruptState {
        InterruptState {
            status: 0,
            mask:   0,
        }
    }

    /// Return true if at least one interrupt is asserted and not
    /// masked
    pub fn active(self) -> bool {
        (self.status & self.mask) != 0
    }

    pub fn status(self) -> u16 {
         self.status
    }

    /// Acknowledge interrupts by writing 0 to the corresponding bit
    pub fn ack(&mut self, ack: u16) {
         self.status &= ack;
    }

    pub fn mask(self) -> u16 {
        self.mask
    }

    pub fn set_mask(&mut self, mask: u16) {
        // Temporary hack: trigger an error if a non-implemented
        // interrupt is requested
        let supported = [ Interrupt::VBlank,
                          Interrupt::CdRom,
                          Interrupt::Dma,
                          Interrupt::Timer0,
                          Interrupt::Timer1,
                          Interrupt::Timer2,
                          Interrupt::PadMemCard];

        let rem = supported.iter().fold(mask,
                                        |mask, &it| mask & !(1 << it as u16));

        if rem != 0 {
            panic!("Unsupported interrupt: {:04x}", rem);
        }

        self.mask = mask;
    }

    /// Trigger the interrupt `which`, must be called on the rising
    /// edge of the interrupt signal.
    pub fn assert(&mut self, which: Interrupt) {
        self.status |= 1 << (which as usize);
    }
}

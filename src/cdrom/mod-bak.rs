//! CD-ROM interface
//!
//! The PlayStation uses an external controller for decoding and
//! correcting CD sectors. This controller is similar to the CXD1199AQ
//! whose datasheet is available online. I try to use the symbolic
//! names defined in this datasheet where it makes sense.
//!
//! This controller communicates asynchronously with a microcontroller
//! handling actual CD-ROM drive (called the "sub-CPU" in the
//! CXD1199AQ datasheet).
//!
//! Since you can't access the sub-CPU directly from the main CPU it's
//! pretty difficult to reverse-engineer what's going on exactly
//! without using an oscilloscope. As a result this implementation is
//! based on No$'s specs, mednafen's source code and some educated
//! guesses.

use memory::Addressable;
use timekeeper::{Peripheral, Cycles};
use interrupt::Interrupt;
use shared::SharedState;
use arrayvec::ArrayVec;
use cdimage::sector::Sector;
use cdimage::msf::Msf;

use self::disc::{Disc, Region};

pub mod disc;

/// CDROM Controller
pub struct CdRom {
    /// Command state machine
    command_state: CommandState,
    /// Data read state machine
    read_state: ReadState,
    /// Some of the memory mapped registers change meaning depending
    /// on the value of the index.
    index: u8,
    /// Command arguments FIFO
    params: Fifo,
    /// Command response FIFO
    response: Fifo,
    /// Interrupt mask (5 bits)
    irq_mask: u8,
    /// Interrupt flag (5 bits). The low 3bits are set by the sub-CPU
    /// (see IrqCode for their meaning). The two other bits are used
    /// by the decoder.
    irq_flags: u8,
    /// Commands/response are generally stalled as long as the
    /// interrupt is active
    on_ack: fn (&mut CdRom) -> CommandState,
    /// Currently loaded disc or None if no disc is present
    disc: Option<Disc>,
    /// Target of the next seek command
    seek_target: Msf,
    /// True if `seek_target` has been set but no seek took place
    seek_target_pending: bool,
    /// Current read position
    position: Msf,
    /// If true the drive is in double speed mode (2x, 150 sectors per
    /// second), otherwise we're in the default 1x (75 sectors per
    /// second).
    double_speed: bool,
    /// If true Send ADPCM samples to the SPU
    xa_adpcm_to_spu: bool,
    /// Data RX buffer
    rx_buffer: [u8; 2352],
    /// Raw sector read from the disc image
    sector: Sector,
    /// When this bit is set the data RX buffer is active, otherwise
    /// it's reset. The software is supposed to reset it between
    /// sectors.
    rx_active: bool,
    /// Index of the next byte to be read in the RX sector
    rx_index: u16,
    /// Index of the last byte to be read in the RX sector
    rx_len: u16,
    /// If true we read the whole sector except for the sync bytes
    /// (0x924 bytes), otherwise it only reads 0x800 bytes.
    read_whole_sector: bool,
    /// Not sure what this does exactly, apparently it overrides the
    /// normal sector size. Needs to run more tests to see what it
    /// does exactly.
    sector_size_override: bool,
    /// Enable CD-DA mode to play Redbook Audio tracks
    cdda_mode: bool,
    /// If true automatically pause at the end of the track
    autopause: bool,
    /// CDROM audio mixer connected to the SPU
    mixer: Mixer,
    /// True if the ADPCM filter is enabled
    filter_enabled: bool,
    /// If ADPCM filtering is enabled only sectors with this file
    /// number are processed
    filter_file: u8,
    /// If ADPCM filtering is enabled only sectors with this channel
    /// number are processed
    filter_channel: u8,
    /// Buffer holding an asynchronous event while we're waiting for
    /// the interrupt to be acknowledged
    pending_async_event: Option<(IrqCode, Fifo)>,
    /// XXX Not sure what this does exactly, No$ says "Enable
    /// Report-Interrupts for Audio Play"
    report_interrupts: bool,
}

impl CdRom {
    pub fn new(disc: Option<Disc>) -> CdRom {
        CdRom {
            command_state: CommandState::Idle,
            read_state: ReadState::Idle,
            index: 0,
            params: Fifo::new(),
            response: Fifo::new(),
            irq_mask: 0,
            irq_flags: 0,
            on_ack: CdRom::ack_idle,
            disc: disc,
            seek_target: Msf::zero(),
            seek_target_pending: false,
            position: Msf::zero(),
            double_speed: false,
            xa_adpcm_to_spu: false,
            sector: Sector::empty(),
            rx_buffer: [0; 2352],
            rx_active: false,
            rx_index: 0,
            rx_len: 0,
            read_whole_sector: true,
            sector_size_override: false,
            cdda_mode: false,
            autopause: false,
            mixer: Mixer::new(),
            filter_enabled: false,
            filter_file: 0,
            filter_channel: 0,
            pending_async_event: None,
            report_interrupts: true,
        }
    }

    pub fn load<T: Addressable>(&mut self,
                                shared: &mut SharedState,
                                offset: u32) -> u32 {
        self.sync(shared);

        if T::size() != 1 {
            panic!("Unhandled CDROM load ({})", T::size());
        }

        let index = self.index;

        let unimplemented = || {
            panic!("read CDROM register {}.{}",
                   offset,
                   index)
        };

        // CXD1199AQ Datasheet section 3 documents the host interface
        let val =
            match offset {
                0 => self.host_status(),
                1 => {
                    // RESULT register. The CXD1199AQ datasheet says
                    // that the response FIFO is 8-byte long, however
                    // the PSX seems to be 16bytes (at least it seems
                    // to wrap around at the 16th read). May be a
                    // small upgrade to the IP in order to support
                    // commands like GetQ which return more than 8
                    // response bytes.

                    if self.response.empty() {
                        warn!("CDROM response FIFO underflow");
                    }

                    self.response.pop()
                }
                3 =>
                    match index {
                        // IRQ mask/flags have the 3 MSB set when
                        // read.
                        0 => self.irq_mask | 0xe0,
                        1 => self.irq_flags | 0xe0,
                        _ => unimplemented(),
                    },
                _ => unimplemented(),
            };

        val as u32
    }

    pub fn store<T: Addressable>(&mut self,
                                 shared: &mut SharedState,
                                 offset: u32,
                                 val: u32) {

        self.sync(shared);

        if T::size() != 1 {
            panic!("Unhandled CDROM store ({})", T::size());
        }

        // All writeable registers are 8bit wide
        let val = val as u8;

        let index = self.index;

        let unimplemented = || {
            panic!("write CDROM register {}.{} {:x}",
                   offset,
                   index,
                   val)
        };

        // CXD1199AQ Datasheet section 3 documents the host interface
        match offset {
            0 => self.set_address(val),
            1 =>
                match index {
                    0 => self.command(shared, val),
                    // ATV2 register
                    3 => self.mixer.cd_right_to_spu_right = val,
                    _ => unimplemented(),
                },
            2 =>
                match index {
                    0 => self.push_param(val),
                    1 => self.set_host_interrupt_mask(val),
                    // ATV0 register
                    2 => self.mixer.cd_left_to_spu_left = val,
                    // ATV3 register
                    3 => self.mixer.cd_right_to_spu_left = val,
                    _ => unimplemented(),
                },
            3 =>
                match index {
                    0 => self.set_host_chip_control(val),
                    1 => {
                        // HCLRCTL (host clear control) register
                        self.irq_ack(val & 0x1f);

                        if val & 0x40 != 0 {
                            self.params.clear();
                        }

                        if val & 0xa0 != 0 {
                            panic!("Unhandled CDROM 3.1: {:02x}", val);
                        }
                    }
                    // ATV1 register
                    2 => self.mixer.cd_left_to_spu_right = val,
                    // ADPCTL register
                    3 => debug!("CDROM Mixer apply {:02x}", val),
                    _ => unimplemented(),
                },
            _ => unimplemented(),
        }

        self.check_async_event(shared);
    }

    pub fn sync(&mut self, shared: &mut SharedState) {

        let delta = shared.tk().sync(Peripheral::CdRom);

        // Command processing is stalled if an interrupt is active
        // XXX mednafen's code also adds a delay *after* the interrupt
        // is acknowledged before the processing restarts
        if self.irq_flags == 0 {
            self.sync_commands(shared, delta);
        }

        // See if have a read pending
        if let ReadState::Reading(delay) = self.read_state {
            let next_sync =
                if delay as Cycles > delta {
                    // Not yet there
                    delay - delta as u32
                } else {
                    debug!("[{}] CDROM read sector {}",
                           shared.tk(),
                           self.position);

                    // A sector has been read from the disc
                    self.sector_read(shared);

                    // Prepare for the next one
                    self.cycles_per_sector()
                };

            self.read_state = ReadState::Reading(next_sync);

            shared.tk().set_next_sync_delta_if_sooner(Peripheral::CdRom,
                                                      next_sync as Cycles);
        }
    }

    /// Synchronize the command processing state machine
    fn sync_commands(&mut self,
                     shared: &mut SharedState,
                     delta: Cycles) {

        let new_command_state =
            match self.command_state {
                CommandState::Idle => {
                    shared.tk().no_sync_needed(Peripheral::CdRom);

                    CommandState::Idle
                }
                CommandState::RxPending(rx_delay,
                                        irq_delay,
                                        irq_code,
                                        response) => {
                    if rx_delay as Cycles > delta {
                        // Nothing new, update the counters
                        let delta = delta as u32;

                        let rx_delay = rx_delay - delta;
                        let irq_delay = irq_delay - delta;

                        shared.tk().set_next_sync_delta(Peripheral::CdRom,
                                                        rx_delay as Cycles);

                        CommandState::RxPending(rx_delay,
                                                irq_delay,
                                                irq_code,
                                                response)
                    } else {
                        // We reached the end of the transfer
                        self.response = response;

                        if irq_delay as Cycles > delta {
                            // Schedule the interrupt
                            let irq_delay = irq_delay - delta as u32;

                            shared.tk().set_next_sync_delta(Peripheral::CdRom,
                                                            irq_delay as Cycles);

                            CommandState::IrqPending(irq_delay, irq_code)
                        } else {
                            // IRQ reached
                            self.trigger_irq(shared, irq_code);

                            shared.tk().no_sync_needed(Peripheral::CdRom);

                            CommandState::Idle
                        }
                    }
                }
                CommandState::IrqPending(irq_delay, irq_code) => {
                    if irq_delay as Cycles > delta {
                        // Not reached the interrupt yet
                        let irq_delay = irq_delay - delta as u32;

                        shared.tk().set_next_sync_delta(Peripheral::CdRom,
                                                        irq_delay as Cycles);

                        CommandState::IrqPending(irq_delay, irq_code)
                    } else {
                        // IRQ reached
                        self.trigger_irq(shared, irq_code);

                        shared.tk().no_sync_needed(Peripheral::CdRom);

                        CommandState::Idle
                    }
                }
            };

        self.command_state = new_command_state;
    }

    /// Retreive a single byte from the RX buffer
    fn read_byte(&mut self) -> u8 {

        let b = self.rx_buffer[self.rx_index as usize];

        if self.rx_active {
            self.rx_index += 1;

            if self.rx_index == self.rx_len {
                // rx_active clears automatically at the end of the
                // transfer
                self.rx_active = false;
            }
        } else {
            panic!("read byte while !rx_active");
        }

        b
    }

    /// The DMA can read the RX buffer one word at a time
    pub fn dma_read_word(&mut self) -> u32 {
        let b0 = self.read_byte() as u32;
        let b1 = self.read_byte() as u32;
        let b2 = self.read_byte() as u32;
        let b3 = self.read_byte() as u32;

        // Pack in a little endian word
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }

    fn do_seek(&mut self) {
        // Make sure we don't end up in track1's pregap, I don't know
        // if it's ever useful? Needs special handling at least...
        if self.seek_target < Msf::from_bcd(0x00, 0x02, 0x00).unwrap() {
            panic!("Seek to track. 1 pregap: {}", self.seek_target);
        }

        self.position = self.seek_target;
        self.seek_target_pending = false;
    }

    /// Called when a new sector must be read
    fn read_sector(&mut self, shared: &mut SharedState) {
        if self.pending_async_event.is_some() {
            // XXX I think it should replace the current pending event
            panic!("Sector read while an async event is pending");
        }

        let position = self.position;

        // Read the sector at `position`
        match self.disc {
            Some(ref mut d) =>
                if let Err(e) = d.image().read_sector(&mut self.sector,
                                                      position) {
                    panic!("Couldn't read sector: {}", e);
                },
            None => panic!("Sector read without a disc"),
        }

        {
            // Extract the data we need from the sector.
            let data =
                if self.read_whole_sector {
                    // Read the entire sector except for the 12bits sync pattern

                    let data =
                        match self.sector.data_2352() {
                            Ok(d) => d,
                            Err(e) =>
                                panic!("Failed to read whole sector {}: {}",
                                       position, e),
                        };

                    // Skip the sync pattern
                    &data[12..]
                } else {
                    // Read 2048 bytes after the Mode2 XA sub-header
                    let data =
                        match self.sector.mode2_xa_payload() {
                            Ok(d) => d as &[u8],
                            Err(e) =>
                                panic!("Failed to read sector {}: {}", position, e),
                        };

                    if data.len() > 2048 {
                        // This is a Mode 2 Form 2 sector, it has more
                        // data and no error correction. It probably
                        // shouldn't be read without
                        // `read_whole_sector` being set.
                        warn!("Form 2 sector partial read");
                    }

                    &data[0..2048]
                };

            // Copy data into the RX buffer
            for (i, &b) in data.iter().enumerate() {
                self.rx_buffer[i] = b;
            }

            self.rx_len = data.len() as u16;
        }

        // XXX in reality the interrupt should happen roughly 1969
        // cycles later
        self.pending_async_event =
            Some((IrqCode::SectorReady,
                  Fifo::from_bytes(&[self.drive_status()])));

        self.check_async_event(shared);

        // Move on to the next segment.
        // XXX what happens when we're at the last one?
        self.position =
            match self.position.next() {
                Some(m) => m,
                None => panic!("MSF overflow!"),
            };
    }

    fn check_async_event(&mut self, shared: &mut SharedState) {

        if let Some((code, response)) = self.pending_async_event {
            if self.irq_flags == 0 {
                // Trigger async interrupt
                self.response = response;
                self.trigger_irq(shared, code);

                self.pending_async_event = None;
            }
        }
    }

    fn host_status(&mut self) -> u8 {
        let mut r = self.index;

        // TODO: ADPCM busy (ADPBUSY)
        r |= 0 << 2;
        // Parameter empty (PRMEMPT)
        r |= (self.params.empty() as u8) << 3;
        // Parameter write ready (PRMWRDY)
        r |= (!self.params.full() as u8) << 4;
        // Result read ready (RSLRRDY)
        r |= (!self.response.empty() as u8) << 5;

        // Data request status (DRQSTS)
        let data_available = self.rx_index < self.rx_len;

        r |= (data_available as u8) << 6;

        // "Busy" flag (BUSYSTS).
        //
        // The CXD1199AQ datasheet says it's set high when we write to
        // the command register and it's cleared when the sub-CPU
        // asserts the CLRBUSY signal.
        match self.command_state {
            CommandState::RxPending(..) => r |= 1 << 7,
            _ => (),
        }

        r
    }


    fn irq(&self) -> bool {
        self.irq_flags & self.irq_mask != 0
    }

    fn trigger_irq(&mut self, shared: &mut SharedState, irq: IrqCode) {
        if self.irq_flags != 0 {
            // XXX No$ says that the interrupts are stacked, i.e. the
            // next interrupt will only become active once the
            // previous one is acked. How deep is the stack? Can it be
            // cleared?
            panic!("Unsupported nested CDROM interrupt");
        }

        let prev_irq = self.irq();

        self.irq_flags = irq as u8;

        if !prev_irq && self.irq() {
            // Interrupt rising edge
            shared.irq_state().assert(Interrupt::CdRom);
        }
    }

    /// ADDRESS register
    fn set_address(&mut self, index: u8) {
        self.index = index & 3;
    }

    fn irq_ack(&mut self, v: u8) {
        self.irq_flags &= !v;

        if self.irq_flags == 0 {

            // Certain commands have a 2nd phase after the first
            // interrupt is acknowledged
            let on_ack = self.on_ack;

            self.on_ack = CdRom::ack_idle;

            self.command_state = on_ack(self);
        }
    }

    /// HCHPCTL register
    fn set_host_chip_control(&mut self, ctrl: u8) {
        let prev_active = self.rx_active;

        self.rx_active = ctrl & 0x80 != 0;

        if self.rx_active {
            if !prev_active {
                // Reset the index to the beginning of the RX buffer
                self.rx_index = 0;
            }
        } else {
            // It seems that on the real hardware when one attempts to
            // read the RX data register while the rx_active bit is
            // low it returns always the same bytes which seems to be
            // located at the *closest* multiple of 8 bytes. I think
            // there's an 8byte buffer behind this register somewhere.
            //
            // I also observe that if I wait too long and a new sector
            // gets read while I'm in the middle of the previous one I
            // can still read the previous sector data up to the next
            // 8byte boundary (need to make more intensive
            // checks). Not that it should matter anyway, it's still
            // garbage as far as the software is concerned.

            // Align to the next multiple of 8bytes
            let i = self.rx_index;

            let adjust = (i & 4) << 1;

            self.rx_index = (i & !7) + adjust
        }

        if ctrl & 0x7f != 0 {
            panic!("CDROM: unhandled HCHPCTL {:02x}", ctrl);
        }
    }

    /// HINTMSK register
    fn set_host_interrupt_mask(&mut self, val: u8) {
        if val & 0x18 != 0 {
            warn!("CDROM: unhandled IRQ mask: {:02x}", val);
        }

        self.irq_mask = val & 0x1f;
    }

    fn push_param(&mut self, param: u8) {
        if self.params.full() {
            warn!("CDROM parameter FIFO overflow");
        }

        self.params.push(param);
    }

    /// Return the number of CPU cycles needed to read a single sector
    /// depending on the current drive speed. The PSX drive can read
    /// 75 sectors per second at 1x or 150sectors per second at 2x.
    fn cycles_per_sector(&self) -> u32 {
        // 1x speed: 75 sectors per second
        let cycles_1x = ::cpu::CPU_FREQ_HZ / 75;

        cycles_1x >> (self.double_speed as u32)
    }

    fn command(&mut self,
               shared: &mut SharedState,
               cmd: u8) {
        if !self.command_state.is_idle() {
            panic!("CDROM command while controller is busy");
        }

        // TODO: is this really accurate? Need to run more tests.
        self.response.clear();

        let handler: fn (&mut CdRom) -> CommandState =
            match cmd {
                0x01 => CdRom::cmd_get_stat,
                0x02 => CdRom::cmd_set_loc,
                // ReadN
                0x06 => CdRom::cmd_read,
                0x09 => CdRom::cmd_pause,
                0x0a => CdRom::cmd_init,
                0x0b => CdRom::cmd_mute,
                0x0c => CdRom::cmd_demute,
                0x0d => CdRom::cmd_set_filter,
                0x0e => CdRom::cmd_set_mode,
                0x0f => CdRom::cmd_get_param,
                0x11 => CdRom::cmd_get_loc_p,
                0x13 => CdRom::cmd_get_tn,
                0x15 => CdRom::cmd_seek_l,
                0x19 => CdRom::cmd_test,
                0x1a => CdRom::cmd_get_id,
                // ReadS
                0x1b => CdRom::cmd_read,
                0x1e => CdRom::cmd_read_toc,
                _    => panic!("Unhandled CDROM command 0x{:02x} {:?}",
                               cmd, self.params),
            };

        if self.irq_flags == 0 {
            // If the previous command (if any) has been acknowledged
            // we can directly start the new one
            self.command_state = handler(self);

            // Schedule the interrupt if needed
            if let CommandState::RxPending(_, irq_delay, _, _)
                = self.command_state {
                shared.tk().set_next_sync_delta(Peripheral::CdRom,
                                                irq_delay as Cycles);
            }
        } else {
            // Schedule the command to be executed once the current
            // one is ack'ed
            self.on_ack = handler;
        }

        if let ReadState::Reading(delay) = self.read_state {
            shared.tk().set_next_sync_delta_if_sooner(Peripheral::CdRom,
                                                      delay as Cycles);
        }

        // It seems that the parameters get cleared in all cases (even
        // if an error occurs). I should run more tests to make sure...
        self.params.clear();
    }

    /// Assembles the first status byte returned by many commands
    fn drive_status(&self) -> u8 {
        match self.disc {
            // XXX on the real hardware bit 4 is always set the first time
            // this command is called even if the console is booted with
            // the tray closed. Using the "get_stat" command command
            // clears it however.
            Some(_) => {
                let mut r = 0;

                let reading = !self.read_state.is_idle();

                // Motor on
                r |= 1 << 1;
                r |= (reading as u8) << 5;

                r
            }
            // No disc, pretend that the shell is open (bit 4)
            None => 0x10,
        }
    }

    /// Read the drive's status byte
    fn cmd_get_stat(&mut self) -> CommandState {
        if !self.params.empty() {
            // If this occurs on real hardware it should set bit 1 of
            // the result byte and then put a 2nd byte "0x20" to
            // signal the wrong number of params. It should also
            // trigger IRQ 5 instead of 3.
            //
            // For now I'm going to assume that if this occurs it
            // means that the emulator is buggy rather than the game.
            panic!("Unexected parameters for CDROM GetStat command");
        }

        let mut response = Fifo::new();

        response.push(self.drive_status());

        // XXX Apparently we should also clear bit 4 of status if the
        // tray is closed (or all the time? not sure what triggers
        // that bit)

        // The response comes earlier when there's no disc
        let rx_delay =
            match self.disc {
                /* Average measured delay with game disc */
                Some(_) => 24_000,
                /* Average measured delay with shell open */
                None => 17_000,
            };

        CommandState::RxPending(rx_delay,
                                rx_delay + 5401,
                                IrqCode::Ok,
                                response)
    }

    /// Tell the CDROM controller where the next seek should take us
    /// (but do not physically perform the seek yet)
    fn cmd_set_loc(&mut self) -> CommandState {
        if self.params.len() != 3 {
            // XXX: should trigger IRQ 5 with response 0x13, 0x20
            panic!("CDROM: bad number of parameters for SetLoc: {:?}",
                   self.params);
        }

        // Parameters are in BCD.
        // XXX: what happens if invalid BCD is used?
        let m = self.params.pop();
        let s = self.params.pop();
        let f = self.params.pop();

        self.seek_target =
            match Msf::from_bcd(m, s, f) {
                Some(m) => m,
                None => panic!("Invalid MSF in set loc: {:02x}:{:02x}:{:02x}",
                               m, s, f),
            };

        self.seek_target_pending = true;

        match self.disc {
            Some(_) =>
                CommandState::RxPending(35_000,
                                        35_000 + 5399,
                                        IrqCode::Ok,
                                        Fifo::from_bytes(&[
                                            self.drive_status()])),
            None =>
                CommandState::RxPending(25_000,
                                        25_000 + 6763,
                                        IrqCode::Error,
                                        Fifo::from_bytes(&[0x11, 0x80])),
        }
    }

    /// Start data read sequence. This is the implementation for both
    /// ReadN and ReadS, apparently the only difference between the
    /// two is that ReadN will retry in case of an error while ReadS
    /// will continue to the next sector (useful for streaming
    /// audio/movies). In our emulator we'll just pretend no error
    /// ever occurs.
    fn cmd_read(&mut self) -> CommandState {
        if !self.read_state.is_idle() {
            panic!("CDROM \"read n\" while we're already reading");
        }

        if self.seek_target_pending {
            // XXX That should take some time...
            self.do_seek();
        }

        let read_delay = self.cycles_per_sector();

        self.read_state = ReadState::Reading(read_delay);

        CommandState::RxPending(28_000,
                                28_000 + 5401,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Stop reading sectors but remain at the same position on the
    /// disc
    fn cmd_pause(&mut self) -> CommandState {
        if self.read_state.is_idle() {
            warn!("Pause when we're not reading");
        }

        self.on_ack = CdRom::ack_pause;

        CommandState::RxPending(25_000,
                                25_000 + 5393,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Reinitialize the CD ROM controller
    fn cmd_init(&mut self) -> CommandState {
        self.on_ack = CdRom::ack_init;

        CommandState::RxPending(58_000,
                                58_000 + 5401,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Mute CDROM audio playback
    fn cmd_mute(&mut self) -> CommandState {
        CommandState::RxPending(23_000,
                                23_000 + 5401,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Demute CDROM audio playback
    fn cmd_demute(&mut self) -> CommandState {
        CommandState::RxPending(32_000,
                                32_000 + 5401,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Filter for ADPCM sectors
    fn cmd_set_filter(&mut self) -> CommandState {
        if self.params.len() != 2 {
            // XXX: should trigger IRQ 5 with response 0x13, 0x20
            panic!("CDROM: bad number of parameters for SetFilter: {:?}",
                   self.params);
        }

        self.filter_file = self.params.pop();
        self.filter_channel = self.params.pop();

        CommandState::RxPending(34_000,
                                34_000 + 5408,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Configure the behaviour of the CDROM drive
    fn cmd_set_mode(&mut self) -> CommandState {
        if self.params.len() != 1 {
            // XXX: should trigger IRQ 5 with response 0x13, 0x20
            panic!("CDROM: bad number of parameters for SetMode: {:?}",
                   self.params);
        }

        let mode = self.params.pop();

        self.double_speed = (mode >> 7) & 1 != 0;
        self.xa_adpcm_to_spu = (mode >> 6) & 1 != 0;
        self.read_whole_sector = (mode >> 5) & 1 != 0;
        self.sector_size_override = (mode >> 4) & 1 != 0;
        self.filter_enabled = (mode >> 3) & 1 != 0;
        self.report_interrupts = (mode >> 2) & 1 != 0;
        self.autopause = (mode >> 1) & 1 != 0;
        self.cdda_mode = (mode >> 0) & 1 != 0;

        if self.cdda_mode ||
           self.autopause ||
           self.report_interrupts ||
           self.sector_size_override {
            panic!("CDROM: unhandled mode: {:02x}", mode);
        }

        CommandState::RxPending(22_000,
                                22_000 + 5391,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Return various parameters of the CDROM controller
    fn cmd_get_param(&mut self) -> CommandState {
        let mut mode = 0u8;

        mode |= (self.double_speed as u8) << 7;
        mode |= (self.xa_adpcm_to_spu as u8) << 6;
        mode |= (self.read_whole_sector as u8) << 5;
        mode |= (self.sector_size_override as u8) << 4;
        mode |= (self.filter_enabled as u8) << 3;
        mode |= (self.report_interrupts as u8) << 2;
        mode |= (self.autopause as u8) << 1;
        mode |= (self.cdda_mode as u8) << 0;

        let response =
            Fifo::from_bytes(&[self.drive_status(),
                               mode,
                               0, // Apparently always 0
                               self.filter_file,
                               self.filter_channel]);

        CommandState::RxPending(26_000,
                                26_000 + 11_980,
                                IrqCode::Ok,
                                response)
    }

    /// Get the current position of the drive head by returning the
    /// contents of the Q subchannel
    fn cmd_get_loc_p(&mut self) -> CommandState {
        if self.position < Msf::from_bcd(0x00, 0x02, 0x00).unwrap() {
            // The values returned in the track 01 pregap are strange,
            // The absolute MSF seems correct but the track MSF looks
            // like garbage.
            //
            // For instance after seeking at 00:01:25 the track MSF
            // returned by GetLocP is 00:00:49 with my PAL Spyro disc.
            panic!("GetLocP while in track1 pregap");
        }

        // Fixme: All this data should be extracted from the
        // subchannel Q (when available in cdimage).

        let metadata = self.sector.metadata();

        // The position returned by get_loc_p seems to be ahead of the
        // currently read sector *sometimes*. Probably because of the
        // way the subchannel data is buffered? Let's not worry about
        // it for now.
        let abs_msf = metadata.msf;

        // Position within the current track
        let track_msf = metadata.track_msf;

        let track = metadata.track;
        let index = metadata.index;

        let (track_m, track_s, track_f) = track_msf.into_bcd();

        let (abs_m, abs_s, abs_f) = abs_msf.into_bcd();

        let response_bcd: ArrayVec<[_; 8]> = [track, index,
                                              track_m, track_s, track_f,
                                              abs_m, abs_s, abs_f]
            .iter()
            .map(|v| v.bcd())
            .collect();

        let response = Fifo::from_bytes(&response_bcd);

        CommandState::RxPending(32_000,
                                32_000 + 16816,
                                IrqCode::Ok,
                                response)
    }

    /// Get the first and last track number for the current session
    fn cmd_get_tn(&mut self) -> CommandState {
        // XXX For now only one track is supported. Values are BCD!
        let first_bcd = 0x01;
        let last_bcd = 0x01;

        let response = Fifo::from_bytes(
            &[self.drive_status(), first_bcd, last_bcd]);

        CommandState::RxPending(40_000,
                                40_000 + 8289,
                                IrqCode::Ok,
                                response)
    }


    /// Execute seek. Target is given by previous "set loc" command.
    fn cmd_seek_l(&mut self) -> CommandState {
        self.do_seek();

        self.on_ack = CdRom::ack_seek_l;

        CommandState::RxPending(35_000,
                                35_000 + 5401,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }


    /// Read the CD-ROM's identification string. This is how the BIOS
    /// checks that the disc is an official PlayStation disc (and not
    /// a copy) and handles region locking.
    fn cmd_get_id(&mut self) -> CommandState {
        match self.disc {
            Some(_) => {
                // When a disc is present we have two responses: first
                // we answer with the status byte and when it's acked
                // we send the actual disc identification sequence
                self.on_ack = CdRom::ack_get_id;

                // First response: status byte
                CommandState::RxPending(26_000,
                                        26_000 + 5401,
                                        IrqCode::Ok,
                                        Fifo::from_bytes(&[
                                            self.drive_status()]))
            }
            None => {
                // Pretend the shell is open
                CommandState::RxPending(20_000,
                                        20_000 + 6776,
                                        IrqCode::Error,
                                        Fifo::from_bytes(&[0x11, 0x80]))
            }
        }
    }

    /// Instruct the CD drive to read the table of contents
    fn cmd_read_toc(&mut self) -> CommandState {
        self.on_ack = CdRom::ack_read_toc;

        CommandState::RxPending(45_000,
                                45_000 + 5401,
                                IrqCode::Ok,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    fn cmd_test(&mut self) -> CommandState {
        if self.params.len() != 1 {
            panic!("Unexpected number of parameters for CDROM test command: {}",
                   self.params.len());
        }

        match self.params.pop() {
            0x20 => self.test_version(),
            n    => panic!("Unhandled CDROM test subcommand 0x{:02x}", n),
        }
    }

    fn test_version(&mut self) -> CommandState {
        // Values returned by my PAL SCPH-7502:
        let response = Fifo::from_bytes(&[
            // Year
            0x98,
            // Month
            0x06,
            // Day
            0x10,
            // Version
            0xc3]);

        let rx_delay =
            match self.disc {
                /* Average measured delay with game disc */
                Some(_) => 21_000,
                /* Average measured delay with shell open */
                None => 29_000,
            };

        CommandState::RxPending(rx_delay,
                                rx_delay + 9_711,
                                IrqCode::Ok,
                                response)
    }

    /// Placeholder function called when an interrupt is acknowledged
    /// and the command is completed
    fn ack_idle(&mut self) -> CommandState {
        CommandState::Idle
    }

    fn ack_seek_l(&mut self) -> CommandState {
        // The seek itself take a while to finish since the drive has
        // to physically move the head.
        //
        // XXX We should probably derive the length from the distance
        // of the seek. Also this timing is not actually tied to the
        // IRQ ack: it starts as soon as the command is sent, so
        // that's not accurate either
        CommandState::RxPending(1_000_000,
                                1_000_000 + 1859,
                                IrqCode::Done,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    /// Prepare the 2nd response of the "get ID" command.
    fn ack_get_id(&mut self) -> CommandState {
        match self.disc {
            Some(ref disc) => {
                let response = Fifo::from_bytes(&[
                    // Status + bit 3 if unlicensed/audio
                    self.drive_status(),
                    // Licensed, not audio, not missing
                    0x00,
                    // Disc type (mode2 apparently?)
                    0x20,
                    // Not sure what this one does. No$ says "8bit
                    // ATIP from Point=C0h, if session info exists",
                    // not sure what it means. Seems to be 0 for all
                    // CDs I've tested...
                    0x00,
                    // Region string: "SCEI" for japan, "SCEE" for
                    // Europe and "SCEA" for US.
                    b'S', b'C', b'E',
                    match disc.region() {
                        Region::Japan => b'I',
                        Region::NorthAmerica => b'A',
                        Region::Europe => b'E',
                    }]);

                CommandState::RxPending(7_336,
                                        7_336 + 12_376,
                                        IrqCode::Done,
                                        response)
            }
            // We shouldn't end up here if no disc is present.
            None => unreachable!(),
        }
    }

    fn ack_read_toc(&mut self) -> CommandState {
        let rx_delay =
            match self.disc {
                // XXX The read TOC command takes a while (almost a
                // second) to execute since the drive goes physically
                // reads the CD's table of contents. However it starts
                // the read sequence as soon as the first command is
                // issued, not when the first IRQ 3 is acknowledged,
                // therefore this state machine is inaccurate: if the
                // software takes a long time to issue the ACK the
                // results will be available faster. For now let's
                // pretend that the software acks very quickly and use
                // ~0.5s delay.
                Some(_) => 16_000_000,
                None => 11_000,
            };

        self.read_state = ReadState::Idle;

        CommandState::RxPending(rx_delay,
                                rx_delay + 1859,
                                IrqCode::Done,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    fn ack_pause(&mut self) -> CommandState {
        self.read_state = ReadState::Idle;

        CommandState::RxPending(2_000_000,
                                2_000_000 + 1858,
                                IrqCode::Done,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }

    fn ack_init(&mut self) -> CommandState {
        self.position = Msf::zero();
        self.seek_target = Msf::zero();
        self.read_state = ReadState::Idle;
        self.double_speed = false;
        self.xa_adpcm_to_spu = false;
        self.read_whole_sector = true;
        self.sector_size_override = false;
        self.filter_enabled = false;
        self.report_interrupts = false;
        self.autopause = false;
        self.cdda_mode = false;

        CommandState::RxPending(2_000_000,
                                2_000_000 + 1870,
                                IrqCode::Done,
                                Fifo::from_bytes(&[
                                    self.drive_status()]))
    }
}

/// Various IRQ codes used by the CDROM controller and their
/// signification.
#[derive(Clone,Copy,Debug)]
enum IrqCode {
    /// A CD sector has been read and is ready to be processed.
    SectorReady = 1,
    /// Command succesful, 2nd response.
    Done = 2,
    /// Command succesful, used for the 1st response.
    Ok = 3,
    /// Error: invalid command, disc command while do disc is present
    /// etc...
    Error = 5,
}

/// CDROM controller state machine
#[derive(Debug)]
enum CommandState {
    /// Controller is idle
    Idle,
    /// Controller is issuing a command or waits for the return
    /// value. We store the number of cycles until the response is
    /// received (RX delay) and the number of cycles until the IRQ is
    /// triggered (IRQ delay) as well as the IRQ code and response
    /// bytes in a FIFO.
    ///
    /// RX delay must *always* be less than or equal to IRQ delay.
    ///
    /// XXX The timings used are the average measured on the real
    /// hardware, however there's a huge standard deviation on the
    /// real hardware so that might require further tuning later on.
    ///
    /// It seems however that the time between the moment the response
    /// fifo receive the response and the moment the interrupt gets
    /// triggered is pretty constant for a given command.
    RxPending(u32, u32, IrqCode, Fifo),
    /// Transaction is done but we're still waiting for the interrupt
    /// (IRQ delay). For some reason it seems to occur a long time
    /// after the RX fifo is filled (thousands of CPU cycles, at least
    /// with some commands).
    IrqPending(u32, IrqCode)
}

impl CommandState {
    fn is_idle(&self) -> bool {
        match *self {
            CommandState::Idle => true,
            _ => false,
        }
    }
}

/// CDROM data read state machine
enum ReadState {
    Idle,
    /// We're expecting a sector
    Reading(u32)
}

impl ReadState {
    fn is_idle(&self) -> bool {
        match *self {
            ReadState::Idle => true,
            _ => false,
        }
    }
}

/// 16byte FIFO used to store command arguments and results
#[derive(Copy,Clone,Debug)]
struct Fifo {
    /// Data buffer
    buffer: [u8; 16],
    /// Write pointer (4bits + carry)
    write_idx: u8,
    /// Read pointer (4bits + carry)
    read_idx: u8,
}

impl Fifo {
    fn new() -> Fifo {
        Fifo {
            buffer: [0; 16],
            write_idx: 0,
            read_idx: 0,
        }
    }

    fn from_bytes(bytes: &[u8]) -> Fifo {
        let mut fifo = Fifo::new();

        for &b in bytes {
            fifo.push(b);
        }

        fifo
    }

    fn empty(&self) -> bool {
	// If both pointers point at the same cell and have the same
	// carry the FIFO is empty.
        self.write_idx == self.read_idx
    }

    fn full(&self) -> bool {
        // The FIFO is full if both indexes point to the same cell
        // while having a different carry.
        self.write_idx == self.read_idx ^ 0x10
    }

    fn clear(&mut self) {
        self.write_idx = 0;
        self.read_idx = 0;
        self.buffer = [0; 16];
    }

    // Retrieve the number of elements in the FIFO. This number is in
    // the range [0; 31] so it's potentially bogus if an overflow
    // occured. This does seem to match the behaviour of the actual
    // hardware though. For instance command 0x19 ("Test") takes a
    // single parameter. If you send 0 or more than one parameter you
    // get an error code back. However if you push 33 parameters in
    // the FIFO only the last one is actually used by the command and
    // it works as expected.
    fn len(&self) -> u8 {
        (self.write_idx.wrapping_sub(self.read_idx)) & 0x1f
    }

    fn push(&mut self, val: u8) {
        let idx = (self.write_idx & 0xf) as usize;

        self.buffer[idx] = val;

        self.write_idx = self.write_idx.wrapping_add(1) & 0x1f;
    }

    fn push_slice(&mut self, s: &[u8]) {
        for b in s {
            self.push(b);
        }
    }

    fn pop(&mut self) -> u8 {
        let idx = (self.read_idx & 0xf) as usize;

        self.read_idx = self.read_idx.wrapping_add(1) & 0x1f;

        self.buffer[idx]
    }
}

/// CD-DA Audio playback mixer. The CDROM's audio stereo output can be
/// mixed arbitrarily before reaching the SPU stereo input.
struct Mixer {
    cd_left_to_spu_left: u8,
    cd_left_to_spu_right: u8,
    cd_right_to_spu_left: u8,
    cd_right_to_spu_right: u8,
}

impl Mixer {
    fn new() -> Mixer {
        Mixer {
            // XXX are those the correct reset values? The registers
            // are write only so it's not straightforward to test.
            cd_left_to_spu_left: 0,
            cd_left_to_spu_right: 0,
            cd_right_to_spu_left: 0,
            cd_right_to_spu_right: 0,
        }
    }
}

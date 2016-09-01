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
use self::simple_rand::SimpleRand;

pub mod disc;
pub mod iso9660;

mod simple_rand;

/// CDROM drive, controller and decoder.
#[derive(RustcDecodable, RustcEncodable)]
pub struct CdRom {
    /// The CD-ROM interface has four memory-mapped registers. The
    /// first one contains an index which defines the meaning of the
    /// three others.
    index: u8,
    /// Command parameter FIFO
    host_params: Fifo,
    /// Command response FIFO
    host_response: Fifo,
    /// Pending command number, if any
    command: Option<u8>,
    /// Interrupt flag (5 bits). The low 3 bits are the sub-CPU
    /// interrupt code.
    irq_flags: u8,
    /// Interrupt mask (5 bits)
    irq_mask: u8,
    /// Data RX buffer
    rx_buffer: RxBuffer,
    /// Raw sector read from the disc image
    sector: Sector,
    /// This bit is set when the program wants to read sector
    /// data. It's automatically cleared when all the sector has been
    /// read but it can also be cleared by writing to the config
    /// register directly.
    rx_active: bool,
    /// Sub-CPU state
    sub_cpu: SubCpu,
    /// Index of the next byte to be read in the RX sector
    rx_index: u16,
    /// Index of the last valid byte in the RX sector
    rx_len: u16,
    /// Variable holding the CD read state
    read_state: ReadState,
    /// True if a sector has been read but not yet notified
    read_pending: bool,
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
    /// If true the controller will generate interrupts for each
    /// sector while playing CD-DA tracks. The response will contain
    /// the current location amongst other things.
    report_interrupts: bool,
    /// True if the ADPCM filter is enabled
    filter_enabled: bool,
    /// If ADPCM filtering is enabled only sectors with this file
    /// number are processed
    filter_file: u8,
    /// If ADPCM filtering is enabled only sectors with this channel
    /// number are processed
    filter_channel: u8,

    /// CDROM audio mixer connected to the SPU
    mixer: Mixer,
    /// PRNG to simulate the pseudo-random CD controller timings (from
    /// the host's perspective)
    rand: SimpleRand,
}

impl CdRom {
    pub fn new(disc: Option<Disc>) -> CdRom {
        CdRom {
            index: 0,
            host_params: Fifo::new(),
            host_response: Fifo::new(),
            command: None,
            irq_flags: 0,
            irq_mask: 0,
            rx_buffer: RxBuffer::new(),
            sector: Sector::empty(),
            rx_active: false,
            sub_cpu: SubCpu::new(),
            rx_index: 0,
            rx_len: 0,
            read_state: ReadState::Idle,
            read_pending: false,
            disc: disc,
            seek_target: Msf::zero(),
            seek_target_pending: false,
            position: Msf::zero(),
            double_speed: false,
            xa_adpcm_to_spu: false,
            read_whole_sector: true,
            sector_size_override: false,
            cdda_mode: false,
            autopause: false,
            report_interrupts: false,
            filter_enabled: false,
            filter_file: 0,
            filter_channel: 0,
            mixer: Mixer::new(),
            rand: SimpleRand::new(),
        }
    }

    pub fn sync(&mut self, shared: &mut SharedState) {
        let delta = shared.tk().sync(Peripheral::CdRom);

        let mut remaining_cycles = delta as u32;

        while remaining_cycles > 0 {
            let elapsed =
                if self.sub_cpu.in_command() {
                    if self.sub_cpu.timer > remaining_cycles {
                        self.sub_cpu.timer -= remaining_cycles;

                        remaining_cycles
                    } else {
                        let step_remaining = self.sub_cpu.timer;

                        // Time to advance to the next step in the
                        // sequence.
                        self.next_sub_cpu_step(shared);

                        step_remaining
                    }
                } else {
                    // No command pending, we can go through the
                    // entire delta at once
                    remaining_cycles
                };

            // We have to step the async events alongside the command
            // sequence since commands can spawn async responses.
            if let Some((delay, async)) = self.sub_cpu.async_response {
                if delay > elapsed {
                    self.sub_cpu.async_response = Some((delay - elapsed, async));
                } else {
                    // The async event is ready to be processed
                    self.sub_cpu.async_response = Some((0, async));
                    self.maybe_process_async_response(shared);
                }
            }

            // Check for sector reads
            if let ReadState::Reading(delay) = self.read_state {
                if delay > elapsed {
                    self.read_state = ReadState::Reading(delay - elapsed);
                } else {
                    let leftover = elapsed - delay;

                    // Read the current sector
                    self.read_sector();
                    self.maybe_notify_read(shared);

                    // Schedule the next sector read
                    let next = self.cycles_per_sector() - leftover;

                    self.read_state = ReadState::Reading(next);
                }
            }

            remaining_cycles -= elapsed;
        }

        self.predict_next_sync(shared);
    }

    // Remove the disc. Returns the disc instance, if any.
    pub fn remove_disc(&mut self) -> Option<Disc> {
        self.set_disc(None)
    }

    // Replace the disc, returns the old value. This is mostly meant
    // to replace the disc when loading savestates, not emulating a
    // real disc swap.
    pub fn set_disc(&mut self, mut disc: Option<Disc>) -> Option<Disc> {
        ::std::mem::swap(&mut self.disc, &mut disc);

        disc
    }

    fn predict_next_sync(&mut self, shared: &mut SharedState) {
        shared.tk().no_sync_needed(Peripheral::CdRom);

        if self.sub_cpu.in_command() {
            // Force a sync at the next step. If we wanted to optimize
            // that some more we could compute the delay till the next
            // IRQ instead since the rest doesn't need a hard sync.
            let delta = self.sub_cpu.timer as Cycles;

            shared.tk().set_next_sync_delta(Peripheral::CdRom, delta);
        } else if self.irq_flags == 0 {
            // If no command or interrupt is pending we'll want to
            // sync at the next async response

            if let Some((delay, _)) = self.sub_cpu.async_response {
                let delta = delay as Cycles;

                shared.tk().set_next_sync_delta(Peripheral::CdRom, delta);
            }
        }

        if let ReadState::Reading(delay) = self.read_state {
            shared.tk().maybe_set_next_sync_delta(Peripheral::CdRom,
                                                  delay as Cycles);
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

                    if self.host_response.is_empty() {
                        panic!("CDROM response FIFO underflow");
                    }

                    self.host_response.pop()
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

        match offset {
            // ADDRESS register
            0 => self.index = val & 3,
            1 =>
                match index {
                    0 => self.set_command(shared, val),
                    // ATV2 register
                    3 => self.mixer.cd_right_to_spu_right = val,
                    _ => unimplemented(),
                },
            2 =>
                match index {
                    0 => self.set_parameter(val),
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
                    // HCLRCTL (host clear control) register
                    1 => {
                        self.irq_ack(shared, val & 0x1f);

                        if val & 0x40 != 0 {
                            self.host_params.clear();
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

    /// HSTS register read
    fn host_status(&mut self) -> u8 {
        let mut r = self.index;

        // TODO: ADPCM busy (ADPBUSY)
        r |= 0 << 2;
        // Parameter empty (PRMEMPT)
        r |= (self.host_params.is_empty() as u8) << 3;
        // Parameter write ready (PRMWRDY)
        r |= (!self.host_params.is_full() as u8) << 4;
        // Result read ready (RSLRRDY)
        r |= (!self.host_response.is_empty() as u8) << 5;

        // Data request status (DRQSTS)
        let data_available = self.rx_index < self.rx_len;

        r |= (data_available as u8) << 6;

        // "Busy" flag (BUSYSTS).
        //
        // The CXD1199AQ datasheet says it's set high when we write to
        // the command register and it's cleared when the sub-CPU
        // asserts the CLRBUSY signal.
        r |= (self.sub_cpu.is_busy() as u8) << 7;

        r
    }

    /// COMMAND register write
    fn set_command(&mut self, shared: &mut SharedState, cmd: u8) {
        if let Some(c) = self.command {
            panic!("Nested CDC command! ({:02x} + {:02x})", c, cmd);
        }

        self.command = Some(cmd);

        self.maybe_start_command(shared);
    }

    /// PARAMETER register write
    fn set_parameter(&mut self, param: u8) {
        if let Some(c) = self.command {
            panic!("Parameter push during command {:02x})", c);
        }

        if self.host_params.is_full() {
            // Wraps around on real hardware
            panic!("CDROM parameter FIFO overflow");
        }

        self.host_params.push(param);
    }

    /// HINTMSK register write
    fn set_host_interrupt_mask(&mut self, val: u8) {
        // We only support the 3 bit sub-CPU interrupt code for now.
        if val & 0x18 != 0 {
            warn!("CDROM: unhandled IRQ mask: {:02x}", val);
        }

        self.irq_mask = val & 0x1f;
    }

    /// HCHPCTL register write
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

    fn irq_ack(&mut self, shared: &mut SharedState, v: u8) {
        self.irq_flags &= !v;

        // Check if a command/async/read event was waiting for the IRQ
        // ack to process.
        //
        // XXX I'm not sure which one would have the priority here,
        // assuming that they're all pending at the same time.
        self.maybe_start_command(shared);
        self.maybe_process_async_response(shared);
        self.maybe_notify_read(shared);
    }

    /// Start the command sequence if a command is pending and the
    /// preconditions are met
    fn maybe_start_command(&mut self, shared: &mut SharedState) {
        if self.command.is_some() {
            // We have to wait until all interrupts have been
            // acknowledged before starting a new command. We also
            // must make sure that the sub-CPU is ready to accept it.
            if self.irq_flags == 0 && !self.sub_cpu.in_command() {
                // We're good to go. Let's begin by computing the
                // seemingly pseudo-random command pending delay.

                let variation = self.rand.next() as u32
                    % timings::COMMAND_PENDING_VARIATION;

                let timer = timings::COMMAND_PENDING + variation;

                self.sub_cpu.start_command(timer);
                self.predict_next_sync(shared);
            }
        }
    }

    /// Start the async response sequence if an async response is
    /// pending and the preconditions are met
    fn maybe_process_async_response(&mut self, shared: &mut SharedState) {
        if let Some((0, async)) = self.sub_cpu.async_response {
            // The async response is ready, see if the sub-CPU is
            // ready to process it
            if self.irq_flags == 0 && !self.sub_cpu.in_command() {
                // We can run the response sequence
                self.sub_cpu.async_response = None;
                self.sub_cpu.response.clear();

                // Assume it's going to be successful, let the
                // handler override that if necessary.
                self.sub_cpu.irq_code = IrqCode::AsyncOk;

                let rx_delay = async(self);

                self.sub_cpu.sequence = SubCpuSequence::AsyncRxPush;
                self.sub_cpu.timer = rx_delay;

                self.predict_next_sync(shared);
            }
        }
    }

    /// Start the async read notification sequence if a sector read is
    /// pending and the preconditions are met
    fn maybe_notify_read(&mut self, shared: &mut SharedState) {
        if self.read_pending {
            if self.irq_flags == 0 && !self.sub_cpu.in_command() {
                self.sub_cpu.response.clear();

                self.sub_cpu.irq_code = IrqCode::SectorReady;

                let status = self.drive_status();

                self.sub_cpu.response.push(status);

                self.sub_cpu.sequence = SubCpuSequence::AsyncRxPush;
                self.sub_cpu.timer = timings::READ_RX_PUSH;

                self.read_pending = false;

                self.predict_next_sync(shared);
            }
        }
    }

    /// Called when it's time to advance in the sub-CPU execution
    /// sequence
    fn next_sub_cpu_step(&mut self, shared: &mut SharedState) {
        match self.sub_cpu.sequence {
            SubCpuSequence::Idle => unreachable!(),
            SubCpuSequence::CommandPending |
            SubCpuSequence::ParamPush => {
                if self.host_params.is_empty() {
                    // We have all the parameters (if any), we can run
                    // the actual command
                    self.execute_command();

                    self.sub_cpu.timer = timings::EXECUTION;
                    self.sub_cpu.sequence = SubCpuSequence::Execution;

                } else {
                    // Send parameter
                    let param = self.host_params.pop();
                    self.sub_cpu.params.push(param);

                    self.sub_cpu.timer = timings::PARAM_PUSH;
                    self.sub_cpu.sequence = SubCpuSequence::ParamPush;
                }
            },
            SubCpuSequence::Execution => {
                self.host_response.clear();

                self.sub_cpu.timer = timings::RX_FLUSH;
                self.sub_cpu.sequence = SubCpuSequence::RxFlush;
            }
            SubCpuSequence::RxFlush |
            SubCpuSequence::RxPush => {
                // We know that there is always at least one response
                // byte for any command so we can run this
                // unconditionally after `RxFlush`
                let b = self.sub_cpu.response.pop();
                self.host_response.push(b);

                if self.sub_cpu.response.is_empty() {
                    self.sub_cpu.timer = timings::BUSY_DELAY;
                    self.sub_cpu.sequence = SubCpuSequence::BusyDelay;
                } else {
                    self.sub_cpu.timer = timings::RX_PUSH;
                    self.sub_cpu.sequence = SubCpuSequence::RxPush;
                }
            }
            SubCpuSequence::BusyDelay => {
                self.sub_cpu.timer = timings::IRQ_DELAY;
                self.sub_cpu.sequence = SubCpuSequence::IrqDelay;
            }
            SubCpuSequence::IrqDelay => {
                self.command = None;

                let irq_code = self.sub_cpu.irq_code;

                self.trigger_irq(shared, irq_code);

                self.sub_cpu.sequence = SubCpuSequence::Idle;
            }
            SubCpuSequence::AsyncRxPush => {
                let b = self.sub_cpu.response.pop();

                self.host_response.push(b);

                if self.sub_cpu.response.is_empty() {
                    // No busy flag for async transfer, we move on
                    // directly to the IRQ delay
                    self.sub_cpu.timer = timings::IRQ_DELAY;
                    self.sub_cpu.sequence = SubCpuSequence::IrqDelay;
                } else {
                    self.sub_cpu.timer = timings::RX_PUSH;
                    self.sub_cpu.sequence = SubCpuSequence::AsyncRxPush;
                }
            }
        }
    }

    /// Trigger an interrupt and check if it must be sent to the main
    /// interrupt controller
    fn trigger_irq(&mut self, shared: &mut SharedState, irq: IrqCode) {
        assert!(self.irq_flags == 0);

        self.irq_flags = irq as u8;

        if self.irq() {
            // Interrupt rising edge
            shared.irq_state_mut().assert(Interrupt::CdRom);
        }
    }

    /// Return the state of the interrupt line
    fn irq(&self) -> bool {
        self.irq_flags & self.irq_mask != 0
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

    /// Return the number of CPU cycles needed to read a single sector
    /// depending on the current drive speed. The PSX drive can read
    /// 75 sectors per second at 1x or 150sectors per second at 2x.
    fn cycles_per_sector(&self) -> u32 {
        // 1x speed: 75 sectors per second
        let cycles_1x = ::cpu::CPU_FREQ_HZ / 75;

        cycles_1x >> (self.double_speed as u32)
    }

    /// Execute a pending seek (if any). On the real console that
    /// would mean physically moving the read head.
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
    fn read_sector(&mut self) {
        if self.read_pending {
            panic!("Sector read while previous one is still pending");
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
                                panic!("Failed to read sector {}: {}",
                                       position, e),
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

        // Move on to the next segment.
        // XXX what happens when we're at the last one?
        self.position =
            match self.position.next() {
                Some(m) => m,
                None => panic!("MSF overflow!"),
            };

        self.read_pending = true;
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

    /// Run the command designated by `self.command`. Panics if
    /// `self.command` is None.
    fn execute_command(&mut self) {

        let (min_param, max_param, handler): (u8, u8, fn(&mut CdRom)) =
            match self.command.unwrap() {
                0x01 => (0, 0, CdRom::cmd_get_stat),
                0x02 => (3, 3, CdRom::cmd_set_loc),
                // ReadN
                0x06 => (0, 0, CdRom::cmd_read),
                0x09 => (0, 0, CdRom::cmd_pause),
                0x0a => (0, 0, CdRom::cmd_init),
                0x0b => (0, 0, CdRom::cmd_mute),
                0x0c => (0, 0, CdRom::cmd_demute),
                0x0d => (2, 2, CdRom::cmd_set_filter),
                0x0e => (1, 1, CdRom::cmd_set_mode),
                0x0f => (0, 0, CdRom::cmd_get_param),
                0x11 => (0, 0, CdRom::cmd_get_loc_p),
                0x15 => (0, 0, CdRom::cmd_seek_l),
                0x19 => (1, 1, CdRom::cmd_test),
                0x1a => (0, 0, CdRom::cmd_get_id),
                // ReadS
                0x1b => (0, 0, CdRom::cmd_read),
                0x1e => (0, 0, CdRom::cmd_read_toc),
                c => panic!("Unhandled CDROM command 0x{:02x} {:?}",
                            c, self.sub_cpu.params),
            };

        let nparams = self.sub_cpu.params.len();

        if nparams < min_param || nparams > max_param {
            panic!("Wrong number of parameters for command {:02x} ({})",
                   self.command.unwrap(), nparams);
        }

        handler(self);
    }

    /// Read the drive's status byte
    fn cmd_get_stat(&mut self) {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Tell the CDROM controller where the next seek should take us
    /// (but do not physically perform the seek yet)
    fn cmd_set_loc(&mut self) {

        // Parameters are in BCD.
        let m = self.sub_cpu.params.pop();
        let s = self.sub_cpu.params.pop();
        let f = self.sub_cpu.params.pop();

        self.seek_target =
            match Msf::from_bcd(m, s, f) {
                Some(m) => m,
                // XXX: what happens if invalid BCD is used?
                None => panic!("Invalid MSF in set loc: {:02x}:{:02x}:{:02x}",
                               m, s, f),
            };

        self.seek_target_pending = true;

        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Start data read sequence. This is the implementation for both
    /// ReadN and ReadS, apparently the only difference between the
    /// two is that ReadN will retry in case of an error while ReadS
    /// will continue to the next sector (useful for streaming
    /// audio/movies). In our emulator we'll just pretend no error
    /// ever occurs.
    fn cmd_read(&mut self) {
        if !self.read_state.is_idle() {
            warn!("CDROM READ while we're already reading");
        }

        if self.seek_target_pending {
            // XXX That should take some time...
            self.do_seek();
        }

        let read_delay = self.cycles_per_sector();

        self.read_state = ReadState::Reading(read_delay);

        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Stop reading sectors but remain at the same position on the
    /// disc
    fn cmd_pause(&mut self) {

        let status = self.drive_status();

        self.sub_cpu.response.push(status);

        self.read_state = ReadState::Idle;

        let async_delay =
            if self.read_state.is_idle() {
                warn!("Pause when we're not reading");
                9000
            } else {
                // XXX Very very rough approximation, can change based
                // on many factors. Need to come up with a more
                // accurate heuristic
                1_000_000
            };

        self.sub_cpu.schedule_async_response(async_delay, CdRom::async_pause);
    }

    fn async_pause(&mut self) -> u32 {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);

        timings::PAUSE_RX_PUSH
    }

    /// Reinitialize the CD ROM controller
    fn cmd_init(&mut self) {
        let status = self.drive_status();
        self.sub_cpu.response.push(status);

        // XXX I think? Needs testing
        self.read_state = ReadState::Idle;
        self.read_pending = false;

        self.sub_cpu.schedule_async_response(900_000,
                                             CdRom::async_init);
    }

    fn async_init(&mut self) -> u32 {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);

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

        timings::INIT_RX_PUSH
    }

    /// Mute CDROM audio playback
    fn cmd_mute(&mut self) {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Demute CDROM audio playback
    fn cmd_demute(&mut self) {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Filter for ADPCM sectors
    fn cmd_set_filter(&mut self) {

        self.filter_file = self.sub_cpu.params.pop();
        self.filter_channel = self.sub_cpu.params.pop();

        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Configure the behaviour of the CDROM drive
    fn cmd_set_mode(&mut self) {

        let mode = self.sub_cpu.params.pop();

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

        let status = self.drive_status();

        self.sub_cpu.response.push(status);
    }

    /// Return various parameters of the CDROM controller
    fn cmd_get_param(&mut self) {
        let mut mode = 0u8;

        mode |= (self.double_speed as u8) << 7;
        mode |= (self.xa_adpcm_to_spu as u8) << 6;
        mode |= (self.read_whole_sector as u8) << 5;
        mode |= (self.sector_size_override as u8) << 4;
        mode |= (self.filter_enabled as u8) << 3;
        mode |= (self.report_interrupts as u8) << 2;
        mode |= (self.autopause as u8) << 1;
        mode |= (self.cdda_mode as u8) << 0;

        let response = [self.drive_status(),
                        mode,
                        0, // Apparently always 0
                        self.filter_file,
                        self.filter_channel];

        self.sub_cpu.response.push_slice(&response);
    }

    /// Get the current position of the drive head by returning the
    /// contents of the Q subchannel
    fn cmd_get_loc_p(&mut self) {
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

        self.sub_cpu.response.push_slice(&response_bcd);
    }

    /// Execute seek. Target is given by previous "set loc" command.
    fn cmd_seek_l(&mut self) {
        self.do_seek();

        let status = self.drive_status();

        self.sub_cpu.response.push(status);

        // XXX the delay for the async response is tied to the time it
        // takes for the reading head to physically seek on the
        // disc. We probably need a heuristic based on the current
        // head position, target position and probably a bunch of
        // other factors. For now hardcode a dumb value and hope for
        // the best.
        self.sub_cpu.schedule_async_response(1_000_000, CdRom::async_seek_l);
    }

    fn async_seek_l(&mut self) -> u32 {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);

        timings::SEEK_L_RX_PUSH
    }

    /// The test command can do a whole bunch of stuff, the first
    /// parameter says what
    fn cmd_test(&mut self) {
        if self.sub_cpu.params.len() != 1 {
            panic!("Unexpected number of parameters for CDROM test command: {}",
                   self.sub_cpu.params.len());
        }

        match self.sub_cpu.params.pop() {
             0x20 => self.test_version(),
             n    => panic!("Unhandled CDROM test subcommand 0x{:02x}", n),
        }
    }

    /// Instruct the CD drive to read the table of contents
    fn cmd_read_toc(&mut self) {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);

        // XXX should probably stop ReadN/S

        self.sub_cpu.schedule_async_response(timings::READ_TOC_ASYNC,
                                             CdRom::async_read_toc);
    }

    fn async_read_toc(&mut self) -> u32 {
        let status = self.drive_status();

        self.sub_cpu.response.push(status);

        timings::READ_TOC_RX_PUSH
    }

    /// Read the CD-ROM's identification string. This is how the BIOS
    /// checks that the disc is an official PlayStation disc (and not
    /// a copy) and handles region locking.
    fn cmd_get_id(&mut self) {

        match self.disc {
            Some(_) => {
                let status = self.drive_status();

                self.sub_cpu.response.push(status);

                self.sub_cpu.schedule_async_response(timings::GET_ID_ASYNC,
                                                     CdRom::async_get_id);
            }
            None => {
                // Pretend the shell is open
                self.sub_cpu.response.push_slice(&[0x11, 0x80]);

                self.sub_cpu.irq_code = IrqCode::Error;
            }
        }
    }

    fn async_get_id(&mut self) -> u32 {
        // If we're here we must have a disc
        let disc = self.disc.as_ref().unwrap();

        let response = [
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
            }
        ];

        self.sub_cpu.response.push_slice(&response);

        timings::GET_ID_RX_PUSH
    }

    /// Read the CD controller's internal version number
    fn test_version(&mut self) {
        // Values returned by my PAL SCPH-7502 console:
        self.sub_cpu.response.push(0x98); // Year
        self.sub_cpu.response.push(0x06); // Month
        self.sub_cpu.response.push(0x10); // Day
        self.sub_cpu.response.push(0xc3); // Version
    }
}

/// 16byte FIFO used to store command arguments and responses
#[derive(Copy, Clone, Debug, RustcDecodable, RustcEncodable)]
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

    fn is_empty(&self) -> bool {
	// If both pointers point at the same cell and have the same
	// carry the FIFO is empty.
        self.write_idx == self.read_idx
    }

    fn is_full(&self) -> bool {
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
        for &v in s {
            self.push(v)
        }
    }

    fn pop(&mut self) -> u8 {
        let idx = (self.read_idx & 0xf) as usize;

        self.read_idx = self.read_idx.wrapping_add(1) & 0x1f;

        self.buffer[idx]
    }
}

/// RX buffer serializable container
buffer!(struct RxBuffer([u8; 2352]));

/// CDROM disc state
#[derive(RustcDecodable, RustcEncodable)]
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

/// Description of the sub-CPU processing sequence
#[derive(PartialEq, Eq, Debug, Copy, Clone, RustcDecodable, RustcEncodable)]
enum SubCpuSequence {
    /// Sub-CPU waits for commands and async events
    Idle,
    /// Command pending, wait for the controller to start the
    /// execution.
    CommandPending,
    /// Parameter transfer
    ParamPush,
    /// Command is executed
    Execution,
    /// Response FIFO is cleared
    RxFlush,
    /// Response transfer
    RxPush,
    /// Busy flag goes down at the end of this step
    BusyDelay,
    /// IRQ is triggered at the end of this step
    IrqDelay,
    /// Async response transfer
    AsyncRxPush,
}

/// Sub-CPU state. This is an 8bit microcontroller in charge
#[derive(RustcDecodable, RustcEncodable)]
struct SubCpu {
    /// Current sub-CPU command state
    sequence: SubCpuSequence,
    /// Countdown until the next step in `sequence`
    timer: u32,
    /// Internal command parameter FIFO
    params: Fifo,
    /// Internal command response FIFO
    response: Fifo,
    /// Status for the current command
    irq_code: IrqCode,
    /// Async command response. The tuple contains a method pointer to
    /// the asynchronous command handler and the number of CPU cycles
    /// until the asynch handler must be run.
    async_response: Option<(u32, AsyncResponse)>,
}

impl SubCpu {
    fn new() -> SubCpu {
        SubCpu {
            sequence: SubCpuSequence::Idle,
            timer: 0,
            params: Fifo::new(),
            response: Fifo::new(),
            irq_code: IrqCode::Ok,
            async_response: None,
        }
    }

    fn start_command(&mut self, pending_delay: u32) {
        assert!(self.in_command() == false);

        if self.async_command_pending() {
            // Not sure what's supposed to happen here, might be
            // command dependant. Can't really see why anybody would
            // want to start a new command without waiting for the
            // response to the previous one though.
            panic!("New CD command while still waiting for an async response");
        }

        self.sequence = SubCpuSequence::CommandPending;
        self.timer = pending_delay;
        self.params.clear();
        self.response.clear();

        // Assume the command will be succesful, let the command
        // handler override that if something goes wrong.
        self.irq_code = IrqCode::Ok;
    }

    fn schedule_async_response(&mut self,
                               delay: u32,
                               handler: fn (&mut CdRom) -> u32) {
        assert!(self.async_response.is_none());

        self.async_response = Some((delay, AsyncResponse(handler)));
    }

    /// Return true if the sub-CPU is executing a command
    fn in_command(&self) -> bool {
        self.sequence != SubCpuSequence::Idle
    }

    /// Return true if an async command is pending
    fn async_command_pending(&self) -> bool {
        self.async_response.is_some()
    }

    /// Busy flag state. This is *not* equivalent to `in_command()`,
    /// the busy flag goes down roughly 2000 cyles before the IRQ
    /// triggers.
    fn is_busy(&self) -> bool {
        match self.sequence {
            SubCpuSequence::CommandPending |
            SubCpuSequence::ParamPush |
            SubCpuSequence::Execution |
            SubCpuSequence::RxFlush |
            SubCpuSequence::RxPush |
            SubCpuSequence::BusyDelay => true,
            _ => false
        }
    }
}

callback!(struct AsyncResponse(fn (&mut CdRom) -> u32) {
    CdRom::async_pause,
    CdRom::async_init,
    CdRom::async_seek_l,
    CdRom::async_read_toc,
    CdRom::async_get_id,
});

/// Various IRQ codes used by the sub-CPU
#[derive(Clone, Copy, Debug, RustcDecodable, RustcEncodable)]
enum IrqCode {
    /// A CD sector has been read and is ready to be processed.
    SectorReady = 1,
    /// Command succesful, 2nd response.
    AsyncOk = 2,
    /// Command succesful, used for the 1st response.
    Ok = 3,
    /// Error: invalid command, disc command while do disc is present
    /// etc...
    Error = 5,
}

/// CD-DA Audio playback mixer. The CDROM's audio stereo output can be
/// mixed arbitrarily before reaching the SPU stereo input.
#[derive(RustcDecodable, RustcEncodable)]
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

mod timings {
    //! CD controller timings, expressed in CPU clock cycles.
    //!
    //! Most of those timings are rough approximations, there can be
    //! massive variations in execution time on the real hardware,
    //! probably because the controller runs its own asynchronous main
    //! loop and has to handle all the events from the physical drive
    //! at the same time it processes the host commands.

    /// Delay between the moment a command starts being processed and
    /// the parameter transfer sequence. This delay is *extremely*
    /// variable on the real hardware (more so than other CD timings)
    /// so this is a low bound (it's possible to have even shorter
    /// times on the real hardware but they are quite uncommon)
    pub const COMMAND_PENDING: u32 = 9_400;

    /// Most command pending times are between COMMAND_PENDING and
    /// (COMMAND_PENDING + COMMAND_PENDING_VARIATION). Shorter and
    /// longer delays are possile on the real hardware but they're
    /// uncommon.
    pub const COMMAND_PENDING_VARIATION: u32 = 6_000;

    /// Approximate duration of a single parameter transfer
    pub const PARAM_PUSH: u32 = 1_800;

    /// Delay between the end of the last parameter push and the RX
    /// FIFO clear. I assume that's when the actual command execution
    /// takes place but I haven't tested it thouroughly. It shouldn't
    /// really matter anyway.
    pub const EXECUTION: u32 = 2_000;

    /// Delay between the RX FIFO clear and the first response byte being
    /// pushed onto the RX FIFO.
    pub const RX_FLUSH: u32 = 3_500;

    /// Time taken by each additional response bytes
    pub const RX_PUSH: u32 = 1_500;

    /// Delay between the moment the last response byte gets pushed
    /// and the moment the busy flag goes low.
    pub const BUSY_DELAY: u32 = 3_300;

    /// Delay between the moment the busy flag goes low and the moment
    /// the IRQ triggers OR in the case of async events the delay
    /// between the last response push and the IRQ
    pub const IRQ_DELAY: u32 = 2_000;

    /// Delay between GetId command execution and the asyncronous RX_CLEAR
    pub const GET_ID_ASYNC: u32 = 15_000;

    /// Delay between the asynchronous RX_CLEAR and first param push
    /// for the asynchronous GetId response
    pub const GET_ID_RX_PUSH: u32 = 3_100;

    /// Very rough estimate of the time taken to read the table of
    /// content. On my console it takes around 1 second (and probably
    /// varies depending on the disc. I'll just use ~0.5s for the
    /// emulator, I doubt it matters much in practice.
    pub const READ_TOC_ASYNC: u32 = 16_000_000;

    /// Delay between the asynchronous RX_CLEAR and first param push
    /// for the asynchronous ReadToc response
    pub const READ_TOC_RX_PUSH: u32 = 1_700;

    /// Delay between the asynchronous RX_CLEAR and first param push
    /// for the asynchronous SeekL response
    pub const SEEK_L_RX_PUSH: u32 = 1_700;

    /// Delay between the asynchronous RX_CLEAR and first param push
    /// for the asynchronous Read(S/N) response
    pub const READ_RX_PUSH: u32 = 1_800;

    /// Delay between the asynchronous RX_CLEAR and first param push
    /// for the asynchronous Pause response
    pub const PAUSE_RX_PUSH: u32 = 1_700;

    /// Delay between the asynchronous RX_CLEAR and first param push
    /// for the asynchronous Init response
    pub const INIT_RX_PUSH: u32 = 1_700;
}

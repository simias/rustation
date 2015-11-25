use self::opengl::{Renderer, Vertex};
use memory::Addressable;
use memory::interrupts::{Interrupt, InterruptState};
use memory::timers::Timers;
use timekeeper::{TimeKeeper, Peripheral, Cycles, FracCycles};
use HardwareType;

pub mod opengl;

pub struct Gpu {
    /// OpenGL renderer
    renderer: Renderer,
    /// Texture page base X coordinate (4 bits, 64 byte increment)
    page_base_x: u8,
    /// Texture page base Y coordinate (1bit, 256 line increment)
    page_base_y: u8,
    /// Mirror textured rectangles along the x axis
    rectangle_texture_x_flip: bool,
    /// Mirror textured rectangles along the y axis
    rectangle_texture_y_flip: bool,
    /// Semi-transparency. Not entirely sure how to handle that value
    /// yet, it seems to describe how to blend the source and
    /// destination colors.
    semi_transparency: u8,
    /// Texture page color depth
    texture_depth: TextureDepth,
    /// Texture window x mask (8 pixel steps)
    texture_window_x_mask: u8,
    /// Texture window y mask (8 pixel steps)
    texture_window_y_mask: u8,
    /// Texture window x offset (8 pixel steps)
    texture_window_x_offset: u8,
    /// Texture window y offset (8 pixel steps)
    texture_window_y_offset: u8,
    /// Enable dithering from 24 to 15bits RGB
    dithering: bool,
    /// Allow drawing to the display area
    draw_to_display: bool,
    /// Force "mask" bit of the pixel to 1 when writing to VRAM
    /// (otherwise don't modify it)
    force_set_mask_bit: bool,
    /// Don't draw to pixels which have the "mask" bit set
    preserve_masked_pixels: bool,
    /// Left-most column of drawing area
    drawing_area_left: u16,
    /// Top-most line of drawing area
    drawing_area_top: u16,
    /// Right-most column of drawing area
    drawing_area_right: u16,
    /// Bottom-most line of drawing area
    drawing_area_bottom: u16,
    /// Drawing offset in the framebuffer
    drawing_offset: (i16, i16),
    /// Currently displayed field. For progressive output this is
    /// always Top.
    field: Field,
    /// When true all textures are disabled
    texture_disable: bool,
    /// Video output horizontal resolution
    hres: HorizontalRes,
    /// Video output vertical resolution
    vres: VerticalRes,
    /// Video mode
    vmode: VMode,
    /// Display depth. The GPU itself always draws 15bit RGB, 24bit
    /// output must use external assets (pre-rendered textures, MDEC,
    /// etc...)
    display_depth: DisplayDepth,
    /// Output interlaced video signal instead of progressive
    interlaced: bool,
    /// Disable the display
    display_disabled: bool,
    /// First column of the display area in VRAM
    display_vram_x_start: u16,
    /// First line of the display area in VRAM
    display_vram_y_start: u16,
    /// Display output horizontal start relative to HSYNC
    display_horiz_start: u16,
    /// Display output horizontal end relative to HSYNC
    display_horiz_end: u16,
    /// Display output first line relative to VSYNC
    display_line_start: u16,
    /// Display output last line relative to VSYNC
    display_line_end: u16,
    /// DMA request direction
    dma_direction: DmaDirection,
    /// Handler function for GP0 writes
    gp0_handler: fn (&mut Gpu, val: u32),
    /// Buffer containing the current GP0 command
    gp0_command: CommandBuffer,
    /// Remaining number of words to fetch for the current GP0 command
    gp0_words_remaining: u32,
    /// Current GP0 command attributes
    gp0_attributes: Gp0Attributes,
    /// True when the GP0 interrupt has been requested
    gp0_interrupt: bool,
    /// True when the VBLANK interrupt is high
    vblank_interrupt: bool,
    /// Fractional GPU cycle remainder resulting from the CPU
    /// clock/GPU clock time conversion. Effectively the phase of the
    /// GPU clock relative to the CPU, expressed in CPU clock periods.
    gpu_clock_phase: u16,
    /// Currently displayed video output line
    display_line: u16,
    /// Current GPU clock tick for the current line
    display_line_tick: u16,
    /// Hardware type (PAL or NTSC)
    hardware: HardwareType,
    /// Next word returned by the GPUREAD command
    read_word: u32,
    /// When drawing polylines we must keep track of the previous
    /// vertex position and color
    polyline_prev: ([i16; 2], [u8; 3]),
}

impl Gpu {
    pub fn new(renderer: opengl::Renderer, hardware: HardwareType) -> Gpu {
        Gpu {
            renderer: renderer,
            page_base_x: 0,
            page_base_y: 0,
            rectangle_texture_x_flip: false,
            rectangle_texture_y_flip: false,
            semi_transparency: 0,
            texture_depth: TextureDepth::T4Bit,
            texture_window_x_mask: 0,
            texture_window_y_mask: 0,
            texture_window_x_offset: 0,
            texture_window_y_offset: 0,
            dithering: false,
            draw_to_display: false,
            force_set_mask_bit: false,
            preserve_masked_pixels: false,
            drawing_area_left: 0,
            drawing_area_top: 0,
            drawing_area_right: 0,
            drawing_area_bottom: 0,
            drawing_offset: (0, 0),
            field: Field::Top,
            texture_disable: false,
            hres: HorizontalRes::from_fields(0, 0),
            vres: VerticalRes::Y240Lines,
            vmode: VMode::Ntsc,
            display_depth: DisplayDepth::D15Bits,
            interlaced: false,
            display_disabled: true,
            display_vram_x_start: 0,
            display_vram_y_start: 0,
            display_horiz_start: 0x200,
            display_horiz_end: 0xc00,
            display_line_start: 0x10,
            display_line_end: 0x100,
            dma_direction: DmaDirection::Off,
            gp0_handler: Gpu::gp0_handle_command,
            gp0_command: CommandBuffer::new(),
            gp0_words_remaining: 0,
            gp0_attributes: Gp0Attributes::new(Gpu::gp0_nop),
            gp0_interrupt: false,
            vblank_interrupt: false,
            gpu_clock_phase: 0,
            display_line: 0,
            display_line_tick: 0,
            hardware: hardware,
            read_word: 0,
            polyline_prev: ([0; 2], [0; 3]),
        }
    }

    /// Return the number of GPU clock cycles in a line and number of
    /// lines in a frame (or field for interlaced output) depending on
    /// the configured video mode
    fn vmode_timings(&self) -> (u16, u16) {
        // The number of ticks per line is an estimate using the
        // average line length recorded by the timer1 using the
        // "hsync" clock source.
        match self.vmode {
            VMode::Ntsc => (3412, 263),
            VMode::Pal  => (3404, 314),
        }
    }

    /// Return the GPU to CPU clock ratio. The value is multiplied by
    /// CLOCK_RATIO_FRAC to get a precise fixed point value.
    fn gpu_to_cpu_clock_ratio(&self) -> FracCycles {
        // First we convert the delta into GPU clock periods.
        // GPU clock in Hz
        let gpu_clock =
            match self.hardware {
                HardwareType::Ntsc => 53_690_000.,
                HardwareType::Pal  => 53_200_000.,
            };

        // CPU clock in Hz
        let cpu_clock = ::cpu::CPU_FREQ_HZ as f32;

        // Clock ratio shifted 16bits to the left
        FracCycles::from_f32(gpu_clock / cpu_clock)
    }

    /// Return the period of the dotclock expressed in CPU clock
    /// periods
    pub fn dotclock_period(&self) -> FracCycles {
        let gpu_clock_period = self.gpu_to_cpu_clock_ratio();

        let dotclock_divider = self.hres.dotclock_divider();

        // Dividing the clock frequency means multiplying its period
        let period = gpu_clock_period.get_fp() * dotclock_divider as Cycles;

        FracCycles::from_fp(period)
    }

    /// Return the current phase of the GPU dotclock relative to the
    /// CPU clock
    pub fn dotclock_phase(&self) -> FracCycles {
        panic!("GPU dotclock phase not implemented");
    }

    /// Return the period of the HSync signal in CPU clock periods
    pub fn hsync_period(&self) -> FracCycles {
        let (ticks_per_line, _) = self.vmode_timings();

        let line_len = FracCycles::from_cycles(ticks_per_line as Cycles);

        // Convert from GPU cycles into CPU cycles
        line_len.divide(self.gpu_to_cpu_clock_ratio())
    }

    /// Return the phase of the hsync (position within the line) in
    /// CPU clock periods.
    pub fn hsync_phase(&self) -> FracCycles {
        let phase = FracCycles::from_cycles(self.display_line_tick as Cycles);

        let clock_phase = FracCycles::from_fp(self.gpu_clock_phase as Cycles);

        let phase = phase.add(clock_phase);

        // Convert phase from GPU clock cycles into CPU clock cycles
        phase.multiply(self.gpu_to_cpu_clock_ratio())
    }

    /// Update the GPU state to its current status
    pub fn sync(&mut self,
                tk: &mut TimeKeeper,
                irq_state: &mut InterruptState) {

        let delta = tk.sync(Peripheral::Gpu);

        // Convert delta in GPU time, adding the leftover from the
        // last time
        let delta = self.gpu_clock_phase as Cycles +
                    delta * self.gpu_to_cpu_clock_ratio().get_fp();

        // The 16 low bits are the new fractional part
        self.gpu_clock_phase = delta as u16;

        // Conwert delta back to integer
        let delta = delta >> 16;

        // Compute the current line and position within the line.

        let (ticks_per_line, lines_per_frame) = self.vmode_timings();

        let ticks_per_line = ticks_per_line as Cycles;
        let lines_per_frame = lines_per_frame as Cycles;

        let line_tick = self.display_line_tick as Cycles + delta;
        let line      = self.display_line as Cycles +
                        line_tick / ticks_per_line;

        self.display_line_tick = (line_tick % ticks_per_line) as u16;

        if line > lines_per_frame {
            // New frame

            if self.interlaced {
                // Update the field
                let nframes = line / lines_per_frame;

                self.field =
                    match (nframes + self.field as Cycles) & 1 != 0 {
                        true  => Field::Top,
                        false => Field::Bottom,
                    }
            }

            self.display_line = (line % lines_per_frame) as u16;
        } else {
            self.display_line = line as u16;
        }

        let vblank_interrupt = self.in_vblank();

        if !self.vblank_interrupt && vblank_interrupt {
            // Rising edge of the vblank interrupt
            irq_state.assert(Interrupt::VBlank);
        }

        if self.vblank_interrupt && !vblank_interrupt {
            // End of vertical blanking, probably as a good place as
            // any to update the display
            self.renderer.display();
        }

        self.vblank_interrupt = vblank_interrupt;

        self.predict_next_sync(tk);
    }

    /// Predict when the next "forced" sync should take place
    pub fn predict_next_sync(&self, tk: &mut TimeKeeper) {
        let (ticks_per_line, lines_per_frame) = self.vmode_timings();

        let ticks_per_line = ticks_per_line as Cycles;
        let lines_per_frame = lines_per_frame as Cycles;

        let mut delta = 0;

        let cur_line = self.display_line as Cycles;

        let display_line_start = self.display_line_start as Cycles;
        let display_line_end   = self.display_line_end   as Cycles;

        // Number of ticks to get to the start of the next line
        delta += ticks_per_line - self.display_line_tick as Cycles;

        // The various -1 in the next formulas are because we start
        // counting at line 0. Without them we'd go one line too far.
        if cur_line >= display_line_end {
            // We're in the vertical blanking at the end of the
            // frame. We want to synchronize at the end of the
            // blanking at the beginning of the next frame.

            // Number of ticks to get to the end of the frame
            delta += (lines_per_frame - cur_line) * ticks_per_line;

            // Numbef of ticks to get to the end of vblank in the next
            // frame
            delta += (display_line_start - 1) * ticks_per_line;

        } else if cur_line < display_line_start {
            // We're in the vertical blanking at the beginning of the
            // frame. We want to synchronize at the end of the
            // blanking for the current rame

            delta += (display_line_start - 1 - cur_line) * ticks_per_line;
        } else {
            // We're in the active video, we want to synchronize at
            // the beginning of the vertical blanking period
            delta += (display_line_end - 1 - cur_line) * ticks_per_line;
        }

        // Convert delta in CPU clock periods.
        delta <<= FracCycles::frac_bits();
        // Remove the current fractional cycle to be more accurate
        delta -= self.gpu_clock_phase as Cycles;

        // Divide by the ratio while always rounding up to make sure
        // we're never triggered too early
        let ratio = self.gpu_to_cpu_clock_ratio().get_fp();
        delta = (delta + ratio - 1) / ratio;

        tk.set_next_sync_delta(Peripheral::Gpu, delta);
    }

    /// Return true if we're currently in the video blanking period
    fn in_vblank(&self) -> bool {
        self.display_line < self.display_line_start ||
        self.display_line >= self.display_line_end
    }

    /// Return the index of the currently displayed VRAM line
    fn displayed_vram_line(&self) -> u16 {
        let offset =
            match self.interlaced {
                true  => self.display_line * 2 + self.field as u16,
                false => self.display_line,
            };

        // The VRAM "wraps around" so we in case of an overflow we
        // simply truncate to 9bits
        (self.display_vram_y_start + offset) & 0x1ff
    }

    pub fn load<T: Addressable>(&mut self,
                                tk: &mut TimeKeeper,
                                irq_state: &mut InterruptState,
                                offset: u32) -> u32 {

        if T::size() != 4 {
            panic!("Unhandled GPU load ({})", T::size());
        }

        self.sync(tk, irq_state);

        let r =
            match offset {
                0 => self.read(),
                4 => self.status(),
                _ => unreachable!(),
            };

        r
    }

    pub fn store<T: Addressable>(&mut self,
                                 tk: &mut TimeKeeper,
                                 timers: &mut Timers,
                                 irq_state: &mut InterruptState,
                                 offset: u32,
                                 val: u32) {

        if T::size() != 4 {
            panic!("Unhandled GPU load ({})", T::size());
        }

        self.sync(tk, irq_state);

        match offset {
            0 => self.gp0(val),
            4 => self.gp1(val, tk, timers, irq_state),
            _ => unreachable!(),
        }
    }

    /// Dispatch to the current GP0 handler method
    pub fn gp0(&mut self, val: u32) {
        (self.gp0_handler)(self, val);
    }

    /// Retrieve value of the status register
    fn status(&self) -> u32 {
        let mut r = 0u32;

        r |= (self.page_base_x as u32) << 0;
        r |= (self.page_base_y as u32) << 4;
        r |= (self.semi_transparency as u32) << 5;
        r |= (self.texture_depth as u32) << 7;
        r |= (self.dithering as u32) << 9;
        r |= (self.draw_to_display as u32) << 10;
        r |= (self.force_set_mask_bit as u32) << 11;
        r |= (self.preserve_masked_pixels as u32) << 12;
        r |= (self.field as u32) << 13;
        // Bit 14: not supported
        r |= (self.texture_disable as u32) << 15;
        r |= self.hres.into_status();
        r |= (self.vres as u32) << 19;
        r |= (self.vmode as u32) << 20;
        r |= (self.display_depth as u32) << 21;
        r |= (self.interlaced as u32) << 22;
        r |= (self.display_disabled as u32) << 23;
        r |= (self.gp0_interrupt as u32) << 24;

        // For now we pretend that the GPU is always ready:
        // Ready to receive command
        r |= 1 << 26;
        // Ready to send VRAM to CPU
        r |= 1 << 27;
        // Ready to receive DMA block
        r |= 1 << 28;

        r |= (self.dma_direction as u32) << 29;

        // Bit 31 is 1 if the currently displayed VRAM line is odd, 0
        // if it's even or if we're in the vertical blanking.
        if !self.in_vblank() {
            r |= ((self.displayed_vram_line() & 1) as u32) << 31
        }

        // Not sure about that, I'm guessing that it's the signal
        // checked by the DMA in when sending data in Request
        // synchronization mode. For now I blindly follow the Nocash
        // spec.
        let dma_request =
            match self.dma_direction {
                // Always 0
                DmaDirection::Off => 0,
                // Should be 0 if FIFO is full, 1 otherwise
                DmaDirection::Fifo => 1,
                // Should be the same as status bit 28
                DmaDirection::CpuToGp0 => (r >> 28) & 1,
                // Should be the same as status bit 27
                DmaDirection::VRamToCpu => (r >> 27) & 1,
            };

        r |= dma_request << 25;

        r
    }

    /// Retrieve value of the "read" register
    fn read(&self) -> u32 {
        println!("GPUREAD");
        // XXX framebuffer read not supported
        self.read_word
    }

    /// GP0 handler method: handle a command word
    fn gp0_handle_command(&mut self, val: u32) {
        let (len, attributes) = self.gp0_parse_command(val);

        self.gp0_words_remaining = len;
        self.gp0_attributes = attributes;
        self.gp0_command.clear();

        self.gp0_handler = Gpu::gp0_handle_parameter;

        // Call the parameter handling function for the current word
        self.gp0_handle_parameter(val);
    }

    /// GP0 handler method: handle a command parameter
    fn gp0_handle_parameter(&mut self, val: u32) {
        self.gp0_command.push_word(val);
        self.gp0_words_remaining -= 1;

        if self.gp0_words_remaining == 0 {
            // Command is complete and ready to run
            //
            // XXX certain commands (like quad drawing) actually start
            // drawing the first triangle before the last vertex is
            // received by the GPU

            // Reset GP0 handler. Can be overriden by the callback in
            // certain cases, for instance for image load commands.
            self.gp0_handler = Gpu::gp0_handle_command;
            (self.gp0_attributes.callback)(self);
        }
    }

    /// GP0 handler method: handle image load
    fn gp0_handle_image_load(&mut self, _: u32) {
        // XXX Should copy pixel data to VRAM

        self.gp0_words_remaining -= 1;

        if self.gp0_words_remaining == 0 {
            // We're done, wait for the next command
            self.gp0_handler = Gpu::gp0_handle_command;
        }
    }

    /// GP0 handler method: handle shaded polyline color word
    fn gp0_handle_shaded_polyline_color(&mut self, val: u32) {
        self.gp0_handler =
            if is_polyline_end_marker(val) {
                // We found the end-of-polyline marker, we're done.
                Gpu::gp0_handle_command
            } else {
                // Store the color and wait for the position in the
                // next word
                self.gp0_command.clear();
                self.gp0_command.push_word(val);
                Gpu::gp0_handle_shaded_polyline_vertex
            };
    }

    /// GP0 handler method: handle shaded polyline vertex word
    fn gp0_handle_shaded_polyline_vertex(&mut self, val: u32) {
        // We don't test for the end-of-polyline marker here because
        // it only works in color words for shaded polylines.

        // The line starts at the end of the previous segment
        let (start_pos, start_color) = self.polyline_prev;

        // Retrieve color stored in `gp0_handle_shaded_polyline_color`
        let end_color = gp0_color(self.gp0_command[0]);
        let end_pos = gp0_position(val);

        let vertices = [
            self.gp0_attributes.vertex(start_pos, start_color),
            self.gp0_attributes.vertex(end_pos, end_color),
            ];

        self.renderer.push_line(&vertices);

        // Store the new ending position for the next segment (if any)
        self.polyline_prev = (end_pos, end_color);

        // We expect the color of the next segment
        self.gp0_handler = Gpu::gp0_handle_shaded_polyline_color;
    }

    /// GP0 handler method: handle monochrome polyline position word
    fn gp0_handle_monochrome_polyline_vertex(&mut self, val: u32) {
        if is_polyline_end_marker(val) {
            // We found the end-of-polyline marker, we're done.
            self.gp0_handler = Gpu::gp0_handle_command;
            return;
        }

        // The line starts at the end of the previous segment
        let (start_pos, color) = self.polyline_prev;

        let end_pos = gp0_position(val);

        let vertices = [
            self.gp0_attributes.vertex(start_pos, color),
            self.gp0_attributes.vertex(end_pos, color),
            ];

        self.renderer.push_line(&vertices);

        // Store the new ending position for the next segment (if any)
        self.polyline_prev = (end_pos, color);
    }


    /// Parse GP0 command and return its length in words and attributes
    fn gp0_parse_command(&self, gp0: u32) -> (u32, Gp0Attributes) {
        let opcode = gp0 >> 24;

        match opcode {
            0x00 =>
                (1, Gp0Attributes::new(Gpu::gp0_nop)),
            0x01 =>
                (1, Gp0Attributes::new(Gpu::gp0_clear_cache)),
            0x02 =>
                (3, Gp0Attributes::new(Gpu::gp0_fill_rect)),
            0x20 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_triangle,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x22 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_triangle,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x24 =>
                (7, Gp0Attributes {
                    callback: Gpu::gp0_textured_triangle,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x25 =>
                (7, Gp0Attributes {
                    callback: Gpu::gp0_textured_triangle,
                    semi_transparent: false,
                    texture: TextureMethod::Raw,
                }),
            0x26 =>
                (7, Gp0Attributes {
                    callback: Gpu::gp0_textured_triangle,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x27 =>
                (7, Gp0Attributes {
                    callback: Gpu::gp0_textured_triangle,
                    semi_transparent: true,
                    texture: TextureMethod::Raw,
                }),
            0x28 =>
                (5, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_quad,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x2a =>
                (5, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_quad,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x2c =>
                (9, Gp0Attributes {
                    callback: Gpu::gp0_textured_quad,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x2d =>
                (9, Gp0Attributes {
                    callback: Gpu::gp0_textured_quad,
                    semi_transparent: false,
                    texture: TextureMethod::Raw,
                }),
            0x2e =>
                (9, Gp0Attributes {
                    callback: Gpu::gp0_textured_quad,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x2f =>
                (9, Gp0Attributes {
                    callback: Gpu::gp0_textured_quad,
                    semi_transparent: true,
                    texture: TextureMethod::Raw,
                }),
            0x30 =>
                (6, Gp0Attributes {
                    callback: Gpu::gp0_shaded_triangle,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x32 =>
                (6, Gp0Attributes {
                    callback: Gpu::gp0_shaded_triangle,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x34 =>
                (9, Gp0Attributes {
                    callback: Gpu::gp0_textured_shaded_triangle,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x36 =>
                (9, Gp0Attributes {
                    callback: Gpu::gp0_textured_shaded_triangle,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x38 =>
                (8, Gp0Attributes {
                    callback: Gpu::gp0_shaded_quad,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x3a =>
                (8, Gp0Attributes {
                    callback: Gpu::gp0_shaded_quad,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x3c =>
                (12, Gp0Attributes {
                    callback: Gpu::gp0_textured_shaded_quad,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x3e =>
                (12, Gp0Attributes {
                    callback: Gpu::gp0_textured_shaded_quad,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x40 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_line,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x42 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_line,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x48 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_polyline,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x4a =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_polyline,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x50 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_shaded_line,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x52 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_shaded_line,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x58 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_shaded_polyline,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x5a =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_shaded_polyline,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x60 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_rect,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x62 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_rect,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x64 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x65 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect,
                    semi_transparent: false,
                    texture: TextureMethod::Raw,
                }),
            0x66 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x67 =>
                (4, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect,
                    semi_transparent: true,
                    texture: TextureMethod::Raw,
                }),
            0x68 =>
                (2, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_rect_1x1,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x6a =>
                (2, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_rect_1x1,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x74 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_8x8,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x75 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_8x8,
                    semi_transparent: false,
                    texture: TextureMethod::Raw,
                }),
            0x76 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_8x8,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x77 =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_8x8,
                    semi_transparent: true,
                    texture: TextureMethod::Raw,
                }),
            0x78 =>
                (2, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_rect_16x16,
                    semi_transparent: false,
                    texture: TextureMethod::None,
                }),
            0x7a =>
                (2, Gp0Attributes {
                    callback: Gpu::gp0_monochrome_rect_16x16,
                    semi_transparent: true,
                    texture: TextureMethod::None,
                }),
            0x7c =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_16x16,
                    semi_transparent: false,
                    texture: TextureMethod::Blended,
                }),
            0x7d =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_16x16,
                    semi_transparent: false,
                    texture: TextureMethod::Raw,
                }),
            0x7e =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_16x16,
                    semi_transparent: true,
                    texture: TextureMethod::Blended,
                }),
            0x7f =>
                (3, Gp0Attributes {
                    callback: Gpu::gp0_textured_rect_16x16,
                    semi_transparent: true,
                    texture: TextureMethod::Raw,
                }),
            0x80 =>
                (4, Gp0Attributes::new(Gpu::gp0_copy_rect)),
            0xa0 =>
                (3, Gp0Attributes::new(Gpu::gp0_image_load)),
            0xc0 =>
                (3, Gp0Attributes::new(Gpu::gp0_image_store)),
            0xe1 =>
                (1, Gp0Attributes::new(Gpu::gp0_draw_mode)),
            0xe2 =>
                (1, Gp0Attributes::new(Gpu::gp0_texture_window)),
            0xe3 =>
                (1, Gp0Attributes::new(Gpu::gp0_drawing_area_top_left)),
            0xe4 =>
                (1, Gp0Attributes::new(Gpu::gp0_drawing_area_bottom_right)),
            0xe5 =>
                (1, Gp0Attributes::new(Gpu::gp0_drawing_offset)),
            0xe6 =>
                (1, Gp0Attributes::new(Gpu::gp0_mask_bit_setting)),
            _    => panic!("Unhandled GP0 command {:08x}", gp0),
        }
    }

    /// GP0(0x00): No operation
    fn gp0_nop(&mut self) {
        // NOP
    }

    /// GP0(0x01): Clear cache
    fn gp0_clear_cache(&mut self) {
        // XXX Not implemented
    }

    /// GP0(0x02): Fill rectangle
    /// *Not* affected by mask setting unlike other rect commands
    fn gp0_fill_rect(&mut self) {
        let top_left = gp0_position(self.gp0_command[1]);
        let size = gp0_position(self.gp0_command[2]);

        let color = gp0_color(self.gp0_command[0]);

        // Alignment constraints
        let left = (top_left[0] & 0x3f0) as u16;
        let top = (top_left[1] & 0x1ff) as u16;

        // width = 0x400 (ignoring higher bits) is the same as width =
        // 0, however width = 0x3ff is rounded up to 0x400 (without
        // wrapping to 0) so we need to special case 0x400
        let width =
            match size[0] & 0x7ff {
                0x400 => 0,
                // round up to the next multiple of 0x10
                n => ((n + 0xf) & 0x3f0) as u16,
            };

        let height = (size[1] & 0x1ff) as u16;

        let mut bottom = top + height;
        let mut right = left + width;

        // XXX Normally the fill rect wraps around: if the x or y
        // coordinates overflow we should wrap around the image.
        if right > 0x400 {
            println!("Fill rect X overflow: {}", right);
            right = 0x400;
        }

        if bottom > 0x200 {
            println!("Fill rect Y overflow: {}", bottom);
            bottom = 0x200;
        }

        self.renderer.fill_rect(color,
                                top, left,
                                bottom, right);
    }

    /// Gp0(0x80): Copy rectangle
    fn gp0_copy_rect(&mut self) {
        let size = gp0_position(self.gp0_command[3]);
        let src_top_left = gp0_position(self.gp0_command[1]);
        let dst_top_left = gp0_position(self.gp0_command[2]);

        // XXX Implement me
        println!("Copy Rectangle {:?} {:?} {:?}",
                 size, src_top_left, dst_top_left);
    }

    /// Draw an untextured unshaded triangle
    fn gp0_monochrome_triangle(&mut self) {
        let color = gp0_color(self.gp0_command[0]);

        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[2]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       color),
            ];

        self.renderer.push_triangle(&vertices);
    }

    /// Draw an untextured unshaded quad
    fn gp0_monochrome_quad(&mut self) {
        let color = gp0_color(self.gp0_command[0]);

        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[2]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[4]),
                                       color),
            ];

        self.renderer.push_quad(&vertices);
    }

    /// Draw a monochrome line
    fn gp0_monochrome_line(&mut self) {
        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       gp0_color(self.gp0_command[0])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[2]),
                                       gp0_color(self.gp0_command[0])),
            ];

        self.renderer.push_line(&vertices);
    }

    /// Draw a monochrome polyline
    fn gp0_monochrome_polyline(&mut self) {
        // Start with the first segment. The end-of-polyline marker is
        // ignored for the first two vertices.

        let color = gp0_color(self.gp0_command[0]);
        let start_pos = gp0_position(self.gp0_command[1]);

        let end_pos = gp0_position(self.gp0_command[2]);

        let vertices = [
            self.gp0_attributes.vertex(start_pos, color),
            self.gp0_attributes.vertex(end_pos, color),
            ];

        self.renderer.push_line(&vertices);

        // Store the end point to continue the polyline when we get
        // the next vertex
        self.polyline_prev = (end_pos, color);

        self.gp0_handler = Gpu::gp0_handle_monochrome_polyline_vertex;
    }


    /// Draw a textured unshaded triangle
    fn gp0_textured_triangle(&mut self) {
        let color = gp0_color(self.gp0_command[0]);

        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[5]),
                                       color),
            ];

        self.renderer.push_triangle(&vertices);
    }

    /// Draw a textured unshaded quad
    fn gp0_textured_quad(&mut self) {
        let color = gp0_color(self.gp0_command[0]);

        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[5]),
                                       color),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[7]),
                                       color),
            ];

        self.renderer.push_quad(&vertices);
    }

    /// Draw an untextured shaded triangle
    fn gp0_shaded_triangle(&mut self) {
        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       gp0_color(self.gp0_command[0])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       gp0_color(self.gp0_command[2])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[5]),
                                       gp0_color(self.gp0_command[4])),
            ];

        self.renderer.push_triangle(&vertices);
    }

    /// Draw an untextured shaded quad
    fn gp0_shaded_quad(&mut self) {
        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       gp0_color(self.gp0_command[0])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       gp0_color(self.gp0_command[2])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[5]),
                                       gp0_color(self.gp0_command[4])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[7]),
                                       gp0_color(self.gp0_command[6])),
            ];

        self.renderer.push_quad(&vertices);
    }

    /// Draw a shaded line
    fn gp0_shaded_line(&mut self) {
        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       gp0_color(self.gp0_command[0])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[3]),
                                       gp0_color(self.gp0_command[2])),
            ];

        self.renderer.push_line(&vertices);
    }

    /// Draw a shaded polyline
    fn gp0_shaded_polyline(&mut self) {
        // Start with the first segment. We cannot have an
        // end-of-polyline marker in any of these vertice's color code
        // (if you put the marker in the 2nd vertex color word it's
        // ignored, it only works starting from the third vertex
        // onwards)

        let start_color = gp0_color(self.gp0_command[0]);
        let start_pos = gp0_position(self.gp0_command[1]);

        let end_color = gp0_color(self.gp0_command[2]);
        let end_pos = gp0_position(self.gp0_command[3]);

        let vertices = [
            self.gp0_attributes.vertex(start_pos, start_color),
            self.gp0_attributes.vertex(end_pos, end_color),
            ];

        self.renderer.push_line(&vertices);

        // Store the end point to continue the polyline when we get
        // the next vertex
        self.polyline_prev = (end_pos, end_color);

        self.gp0_handler = Gpu::gp0_handle_shaded_polyline_color;
    }

    /// Draw a textured shaded triangle
    fn gp0_textured_shaded_triangle(&mut self) {
        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       gp0_color(self.gp0_command[0])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[4]),
                                       gp0_color(self.gp0_command[3])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[7]),
                                       gp0_color(self.gp0_command[6])),
            ];

        self.renderer.push_triangle(&vertices);
    }

    /// Draw a textured shaded quad
    fn gp0_textured_shaded_quad(&mut self) {
        let vertices = [
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[1]),
                                       gp0_color(self.gp0_command[0])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[4]),
                                       gp0_color(self.gp0_command[3])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[7]),
                                       gp0_color(self.gp0_command[6])),
            self.gp0_attributes.vertex(gp0_position(self.gp0_command[10]),
                                       gp0_color(self.gp0_command[9])),
            ];

        self.renderer.push_quad(&vertices);
    }

    /// Helper function for drawing rectangles
    fn gp0_rect_sized(&mut self, width: i16, height: i16) {
        let top_left = gp0_position(self.gp0_command[1]);

        let color = gp0_color(self.gp0_command[0]);

        let vertices = [
            self.gp0_attributes.vertex(top_left, color),
            self.gp0_attributes.vertex([top_left[0] + width,
                                        top_left[1]],
                                       color),
            self.gp0_attributes.vertex([top_left[0],
                                        top_left[1] + height],
                                       color),
            self.gp0_attributes.vertex([top_left[0] + width,
                                        top_left[1] + height],
                                       color),
        ];

        self.renderer.push_quad(&vertices);
    }

    /// Draw a textured rectangle
    fn gp0_textured_rect(&mut self) {
        let size = gp0_position(self.gp0_command[3]);

        self.gp0_rect_sized(size[0], size[1]);
    }

    /// Draw a monochrome rectangle
    fn gp0_monochrome_rect(&mut self) {
        let size = gp0_position(self.gp0_command[2]);

        self.gp0_rect_sized(size[0], size[1]);
    }

    /// Draw a 1x1 monochrome rectangle (point)
    fn gp0_monochrome_rect_1x1(&mut self) {
        self.gp0_rect_sized(1, 1);
    }

    /// Draw a 16x16 monochrome rectangle
    fn gp0_monochrome_rect_16x16(&mut self) {
        self.gp0_rect_sized(16, 16);
    }


    /// Draw a 8x8 textured rectangle
    fn gp0_textured_rect_8x8(&mut self) {
        self.gp0_rect_sized(8, 8);
    }

    /// Draw a 16x16 textured rectangle
    fn gp0_textured_rect_16x16(&mut self) {
        self.gp0_rect_sized(16, 16);
    }

    /// GP0(0xA0): Image Load
    fn gp0_image_load(&mut self) {
        // Parameter 2 contains the image resolution
        let res = self.gp0_command[2];

        let width  = res & 0xffff;
        let height = res >> 16;

        // Size of the image in 16bit pixels
        let imgsize = width * height;

        // If we have an odd number of pixels we must round up since
        // we transfer 32bits at a time. There'll be 16bits of padding
        // in the last word.
        let imgsize = (imgsize + 1) & !1;

        // Store number of 32bit words expected for this image
        self.gp0_words_remaining = imgsize / 2;

        if self.gp0_words_remaining > 0 {
            // Use a custom GP0 handler to handle the GP0 image load
            self.gp0_handler = Gpu::gp0_handle_image_load;
        } else {
            println!("GPU: 0-sized image load");
        }

    }

    /// GP0(0xC0): Image Store
    fn gp0_image_store(&mut self) {
        // Parameter 2 contains the image resolution
        let res = self.gp0_command[2];

        let width  = res & 0xffff;
        let height = res >> 16;

        println!("Unhandled image store: {}x{}", width, height);
    }

    /// GP0(0xE1): Draw Mode
    fn gp0_draw_mode(&mut self) {
        let val = self.gp0_command[0];

        self.page_base_x = (val & 0xf) as u8;
        self.page_base_y = ((val >> 4) & 1) as u8;
        self.semi_transparency = ((val >> 5) & 3) as u8;

        self.texture_depth =
            match (val >> 7) & 3 {
                0 => TextureDepth::T4Bit,
                1 => TextureDepth::T8Bit,
                2 => TextureDepth::T15Bit,
                n => panic!("Unhandled texture depth {}", n),
            };

        self.dithering = ((val >> 9) & 1) != 0;
        self.draw_to_display = ((val >> 10) & 1) != 0;
        self.texture_disable = ((val >> 11) & 1) != 0;
        self.rectangle_texture_x_flip = ((val >> 12) & 1) != 0;
        self.rectangle_texture_y_flip = ((val >> 13) & 1) != 0;
    }

    /// GP0(0xE2): Set Texture Window
    fn gp0_texture_window(&mut self) {
        let val = self.gp0_command[0];

        self.texture_window_x_mask = (val & 0x1f) as u8;
        self.texture_window_y_mask = ((val >> 5) & 0x1f) as u8;
        self.texture_window_x_offset = ((val >> 10) & 0x1f) as u8;
        self.texture_window_y_offset = ((val >> 15) & 0x1f) as u8;
    }

    /// GP0(0xE3): Set Drawing Area top left
    fn gp0_drawing_area_top_left(&mut self) {
        let val = self.gp0_command[0];

        self.drawing_area_top = ((val >> 10) & 0x3ff) as u16;
        self.drawing_area_left = (val & 0x3ff) as u16;

        self.update_drawing_area();
    }

    /// GP0(0xE4): Set Drawing Area bottom right
    fn gp0_drawing_area_bottom_right(&mut self) {
        let val = self.gp0_command[0];

        self.drawing_area_bottom = ((val >> 10) & 0x3ff) as u16;
        self.drawing_area_right = (val & 0x3ff) as u16;

        self.update_drawing_area();
    }

    // Called when the drawing area changes to notify the renderer
    fn update_drawing_area(&mut self) {
        self.renderer.set_drawing_area(self.drawing_area_left,
                                       self.drawing_area_top,
                                       self.drawing_area_right,
                                       self.drawing_area_bottom);
    }

    /// GP0(0xE5): Set Drawing Offset
    fn gp0_drawing_offset(&mut self) {
        let val = self.gp0_command[0];

        let x = (val & 0x7ff) as u16;
        let y = ((val >> 11) & 0x7ff) as u16;

        // Values are 11bit two's complement signed values, we need to
        // shift the value to 16bits to force sign extension
        let x = ((x << 5) as i16) >> 5;
        let y = ((y << 5) as i16) >> 5;

        self.drawing_offset = (x, y);
        self.renderer.set_draw_offset(x, y);
    }

    /// GP0(0xE6): Set Mask Bit Setting
    fn gp0_mask_bit_setting(&mut self) {
        let val = self.gp0_command[0];

        self.force_set_mask_bit = (val & 1) != 0;
        self.preserve_masked_pixels = (val & 2) != 0;
    }

    /// Handle writes to the GP1 command register
    pub fn gp1(&mut self,
               val: u32,
               tk: &mut TimeKeeper,
               timers: &mut Timers,
               irq_state: &mut InterruptState) {
        let opcode = (val >> 24) & 0xff;

        match opcode {
            0x00 => {
                self.gp1_reset(tk, irq_state);
                timers.video_timings_changed(tk, irq_state, self);
            },
            0x01 => self.gp1_reset_command_buffer(),
            0x02 => self.gp1_acknowledge_irq(),
            0x03 => self.gp1_display_enable(val),
            0x04 => self.gp1_dma_direction(val),
            0x05 => self.gp1_display_vram_start(val),
            0x06 => self.gp1_display_horizontal_range(val),
            0x07 => self.gp1_display_vertical_range(val, tk, irq_state),
            0x10 => self.gp1_get_info(val),
            0x08 => {
                self.gp1_display_mode(val, tk, irq_state);
                timers.video_timings_changed(tk, irq_state, self);
            }
            _    => panic!("Unhandled GP1 command {:08x}", val),
        }
    }

    /// GP1(0x00): Soft Reset
    fn gp1_reset(&mut self,
                 tk: &mut TimeKeeper,
                 irq_state: &mut InterruptState) {
        self.page_base_x = 0;
        self.page_base_y = 0;
        self.semi_transparency = 0;
        self.texture_depth = TextureDepth::T4Bit;
        self.texture_window_x_mask = 0;
        self.texture_window_y_mask = 0;
        self.texture_window_x_offset = 0;
        self.texture_window_y_offset = 0;
        self.dithering = false;
        self.draw_to_display = false;
        self.texture_disable = false;
        self.rectangle_texture_x_flip = false;
        self.rectangle_texture_y_flip = false;
        self.drawing_area_left = 0;
        self.drawing_area_top = 0;
        self.drawing_area_right = 0;
        self.drawing_area_bottom = 0;
        self.force_set_mask_bit = false;
        self.preserve_masked_pixels = false;

        self.dma_direction = DmaDirection::Off;

        self.display_disabled = true;
        self.display_vram_x_start = 0;
        self.display_vram_y_start = 0;
        self.hres = HorizontalRes::from_fields(0, 0);
        self.vres = VerticalRes::Y240Lines;
        self.field = Field::Top;

        self.vmode = VMode::Ntsc;
        self.interlaced = true;
        self.display_horiz_start = 0x200;
        self.display_horiz_end = 0xc00;
        self.display_line_start = 0x10;
        self.display_line_end = 0x100;
        self.display_depth = DisplayDepth::D15Bits;
        self.display_line = 0;
        self.display_line_tick = 0;

        self.renderer.set_draw_offset(0, 0);

        self.gp1_reset_command_buffer();
        self.gp1_acknowledge_irq();

        self.sync(tk, irq_state);

        // XXX should also invalidate GPU cache if we ever implement it
    }

    /// GP1(0x01): Reset Command Buffer
    fn gp1_reset_command_buffer(&mut self) {
        self.gp0_command.clear();
        self.gp0_words_remaining = 0;
        self.gp0_handler = Gpu::gp0_handle_command;
        // XXX should also clear the command FIFO when we implement it
    }

    /// GP1(0x02): Acknowledge Interrupt
    fn gp1_acknowledge_irq(&mut self) {
        self.gp0_interrupt = false;
    }

    /// GP1(0x03): Display Enable
    fn gp1_display_enable(&mut self, val: u32) {
        self.display_disabled = val & 1 != 0;
    }

    /// GP1(0x04): DMA Direction
    fn gp1_dma_direction(&mut self, val: u32) {
        self.dma_direction =
            match val & 3 {
                0 => DmaDirection::Off,
                1 => DmaDirection::Fifo,
                2 => DmaDirection::CpuToGp0,
                3 => DmaDirection::VRamToCpu,
                _ => unreachable!(),
            };
    }

    /// GP1(0x05): Display VRAM Start
    fn gp1_display_vram_start(&mut self, val: u32) {
        self.display_vram_x_start = (val & 0x3fe) as u16;
        self.display_vram_y_start = ((val >> 10) & 0x1ff) as u16;
    }

    /// GP1(0x06): Display Horizontal Range
    fn gp1_display_horizontal_range(&mut self, val: u32) {
        self.display_horiz_start = (val & 0xfff) as u16;
        self.display_horiz_end   = ((val >> 12) & 0xfff) as u16;
    }

    /// GP1(0x07): Display Vertical Range
    fn gp1_display_vertical_range(&mut self,
                                  val: u32,
                                  tk: &mut TimeKeeper,
                                  irq_state: &mut InterruptState) {
        self.display_line_start = (val & 0x3ff) as u16;
        self.display_line_end   = ((val >> 10) & 0x3ff) as u16;

        self.sync(tk, irq_state);
    }

    /// Return various GPU state information in the GPUREAD register
    fn gp1_get_info(&mut self, val: u32) {
        // XXX what happens if we're in the middle of a framebuffer
        // read?
        let v =
            match val & 0xf {
                3 => {
                    let top = self.drawing_area_top as u32;
                    let left = self.drawing_area_left as u32;

                    left | (top << 10)
                }
                4 => {
                    let bottom = self.drawing_area_bottom as u32;
                    let right = self.drawing_area_right as u32;

                    right | (bottom << 10)
                }
                5 => {
                    let (x, y) = self.drawing_offset;

                    let x = (x as u32) & 0x7ff;
                    let y = (y as u32) & 0x7ff;

                    x | (y << 11)
                }
                // GPU version. Seems to always be 2?
                7 => 2,
                _ => panic!("Unsupported GP1 info command {:08x}", val),
            };

        self.read_word = v;
    }

    /// GP1(0x08): Display Mode
    fn gp1_display_mode(&mut self,
                        val: u32,
                        tk: &mut TimeKeeper,
                        irq_state: &mut InterruptState) {
        let hr1 = (val & 3) as u8;
        let hr2 = ((val >> 6) & 1) as u8;

        self.hres = HorizontalRes::from_fields(hr1, hr2);

        self.vres =
            match val & 0x4 != 0 {
                false => VerticalRes::Y240Lines,
                true  => VerticalRes::Y480Lines,
            };

        self.vmode =
            match val & 0x8 != 0 {
                false => VMode::Ntsc,
                true  => VMode::Pal,
            };

        self.display_depth =
            match val & 0x10 != 0 {
                false => DisplayDepth::D24Bits,
                true  => DisplayDepth::D15Bits,
            };

        self.interlaced = val & 0x20 != 0;
        // XXX Not sure if I should reset field here
        self.field = Field::Top;

        if val & 0x80 != 0 {
            panic!("Unsupported display mode {:08x}", val);
        }

        self.sync(tk, irq_state);
    }
}

/// Depth of the pixel values in a texture page
#[derive(Clone,Copy)]
enum TextureDepth {
    /// 4 bits per pixel
    T4Bit = 0,
    /// 8 bits per pixel
    T8Bit = 1,
    /// 15 bits per pixel
    T15Bit = 2,
}

/// Interlaced output splits each frame in two fields
#[derive(Clone,Copy)]
enum Field {
    /// Top field (odd lines).
    Top = 1,
    /// Bottom field (even lines)
    Bottom = 0,
}

/// Video output horizontal resolution
#[derive(Clone,Copy)]
struct HorizontalRes(u8);

impl HorizontalRes {
    /// Create a new HorizontalRes instance from the 2 bit field `hr1`
    /// and the one bit field `hr2`
    fn from_fields(hr1: u8, hr2: u8) -> HorizontalRes {
        let hr = (hr2 & 1) | ((hr1 & 3) << 1);

        HorizontalRes(hr)
    }

    /// Retrieve value of bits [18:16] of the status register
    fn into_status(self) -> u32 {
        let HorizontalRes(hr) = self;

        (hr as u32) << 16
    }

    /// Return the divider used to generate the dotclock from the GPU
    /// clock.
    fn dotclock_divider(self) -> u8 {
        let hr1 = (self.0 >> 1) & 0x3;
        let hr2 = self.0 & 1 != 0;

        // The encoding of this field is a bit weird, if bit
        // "Horizontal Resolution 2" is set then we're in "368pixel"
        // mode (dotclock = GPU clock / 7). If it's not set then we
        // must check the other two bits of "Horizontal Resolution 2".
        //
        // Note that the horizontal resolutions given here are
        // estimates, it's roughly the number of dotclock ticks
        // necessary to fill a line with the given
        // divider. `display_horiz_start` and `display_horiz_end` will
        // give the actual resolution.
        if hr2 {
            // HRes ~ 368pixels
            7
        } else {
            match hr1 {
                // Hres ~ 256pixels
                0 => 10,
                // Hres ~ 320pixels
                1 => 8,
                // Hres ~ 512pixels
                2 => 5,
                // Hres ~ 640pixels
                3 => 4,
                _ => unreachable!(),
            }
        }
    }
}

/// Video output vertical resolution
#[derive(Clone,Copy)]
enum VerticalRes {
    /// 240 lines
    Y240Lines = 0,
    /// 480 lines (only available for interlaced output)
    Y480Lines = 1,
}

/// Video Modes
#[derive(Clone,Copy)]
enum VMode {
    /// NTSC: 480i60H
    Ntsc = 0,
    /// PAL: 576i50Hz
    Pal  = 1,
}

/// Display area color depth
#[derive(Clone,Copy)]
enum DisplayDepth {
    /// 15 bits per pixel
    D15Bits = 0,
    /// 24 bits per pixel
    D24Bits = 1,
}

/// Requested DMA direction.
#[derive(Clone,Copy)]
enum DmaDirection {
    Off = 0,
    Fifo = 1,
    CpuToGp0 = 2,
    VRamToCpu = 3,
}

/// Buffer holding multi-word fixed-length GP0 command parameters
struct CommandBuffer {
    /// Command buffer: the longuest possible command is GP0(0x3E)
    /// which takes 12 parameters
    buffer: [u32; 12],
    /// Number of words queued in buffer
    len:    u8,
}

impl CommandBuffer {
    fn new() -> CommandBuffer {
        CommandBuffer {
            buffer: [0; 12],
            len:    0,
        }
    }

    /// Clear the command buffer
    fn clear(&mut self) {
        self.len = 0;
    }

    fn push_word(&mut self, word: u32) {
        self.buffer[self.len as usize] = word;

        self.len += 1;
    }
}

impl ::std::ops::Index<usize> for CommandBuffer {
    type Output = u32;

    fn index<'a>(&'a self, index: usize) -> &'a u32 {
        if index >= self.len as usize {
            panic!("Command buffer index out of range: {} ({})",
                   index, self.len);
        }

        &self.buffer[index]
    }
}

/// Attributes of the various GP0 commands. Some attributes don't make
/// sense for certain commands but those will just be ignored by the
/// `parser_callback`
struct Gp0Attributes {
    /// Method called when all the parameters have been received.
    callback: fn(&mut Gpu),
    /// True for semi-transparent primitives
    semi_transparent: bool,
    /// True for textured primitives
    #[allow(dead_code)]
    texture: TextureMethod,
}

impl Gp0Attributes {
    /// Builder for GP0 commands that just need a parser and ignore
    /// the other attributes.
    fn new(callback: fn(&mut Gpu)) -> Gp0Attributes {
        Gp0Attributes {
            callback: callback,
            semi_transparent: false,
            texture: TextureMethod::None,
        }
    }

    /// Build a vertex using the current attributes
    fn vertex(&self, position: [i16; 2], color: [u8; 3]) -> Vertex {
        // XXX handle texturing
        Vertex::new(position, color, self.semi_transparent)
    }
}

/// Primitive texturing methods
enum TextureMethod {
    /// No texture
    None,
    /// Raw texture
    Raw,
    /// Texture + color blending
    Blended,
}

/// Parse a position as written in the GP0 register and return it as
/// an array of two `i16`
fn gp0_position(pos: u32) -> [i16; 2] {
    let x = pos as i16;
    let y = (pos >> 16) as i16;

    [x, y]
}

/// Parse a color as written in the GP0 register and return it as
/// an array of 3 `u8`
fn gp0_color(col: u32) -> [u8; 3] {
    let r = col as u8;
    let g = (col >> 8) as u8;
    let b = (col >> 16) as u8;

    [r, g, b]
}

/// Return true if the word is a polyline end maker. Most games use
/// `0x55555555` but the GPU only tests for `0x5XXX5XXX` (so
/// `0x51235abc` would be a valid marker for instance).
fn is_polyline_end_marker(val: u32) -> bool {
    val & 0xf000f000 == 0x50005000
}

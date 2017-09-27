use memory::Addressable;
use shared::SharedState;
use tracer::module_tracer;

/// Motion Decoder (sometimes called macroblock or movie decoder).
#[derive(RustcDecodable, RustcEncodable)]
pub struct MDec {
    dma_in_enable: bool,
    dma_out_enable: bool,
    output_depth: OutputDepth,
    output_signed: bool,
    /// When the output depth is set to 15bpp this variable is used to
    /// fill the 16th bit
    output_bit15: bool,
    /// Quantization matrices: 8x8 bytes, first one is for luma, 2nd
    /// is for chroma.
    quant_matrices: [QuantMatrix; 2],
    /// Inverse discrete cosine transform matrix. No$ says that "all
    /// known games" use the same values for this matrix, so it could
    /// be possible to optimize the decoding for this particular
    /// table.
    idct_matrix: IdctMatrix,
    /// Callback handling writes to the command register. Returns
    /// `false` when it receives the last word for the command.
    command_handler: CommandHandler,
    /// Remaining words expected for this command
    command_remaining: u16,
    /// Buffer for the currently decoded luma macroblock.
    block_y: Macroblock,
    /// Buffer for the currently decoded Cb macroblock.
    block_u: Macroblock,
    /// Buffer for the currently decoded Cr macroblock.
    block_v: Macroblock,
    /// 16bit coefficients during macroblock decoding
    block_coeffs: MacroblockCoeffs,
    /// Current block type
    current_block: BlockType,
    /// Index in the current macroblock
    block_index: u8,
    /// Quantization factor for the current macroblock, used for the
    /// AC values only.
    qscale: u8,
}

impl MDec {
    pub fn new() -> MDec {
        MDec {
            dma_in_enable: false,
            dma_out_enable: false,
            output_depth: OutputDepth::D4Bpp,
            output_signed: false,
            output_bit15: false,
            current_block: BlockType::CrMono,
            quant_matrices: [QuantMatrix::new(),
                             QuantMatrix::new()],
            idct_matrix: IdctMatrix::new(),
            command_handler: CommandHandler(MDec::handle_command),
            command_remaining: 1,
            block_y: Macroblock::new(),
            block_u: Macroblock::new(),
            block_v: Macroblock::new(),
            block_coeffs: MacroblockCoeffs::new(),
            block_index: 0,
            qscale: 0,
        }
    }

    pub fn load<A: Addressable>(&mut self,
                                 _: &mut SharedState,
                                 offset: u32) -> u32 {

        if A::size() != 4 {
            panic!("Unhandled MDEC load ({})", A::size());
        }

        match offset {
            4 => self.status(),
            _ => panic!("Unhandled MDEC load: {:08x}", offset),
        }
    }


    pub fn store<A: Addressable>(&mut self,
                                 shared: &mut SharedState,
                                 offset: u32,
                                 val: u32) {

        if A::size() != 4 {
            panic!("Unhandled MDEC store ({})", A::size());
        }

        match offset {
            0 => self.command(shared, val),
            4 => self.set_control(val),
            _ => panic!("Unhandled MDEC store: {:08x} {:08x}", offset, val),
        }
    }

    /// Status register
    pub fn status(&self) -> u32 {
        let mut r = 0;

        // Bits [15:0] contain the number of remaining parameter words
        // minus 1, or 0xffff if no parameter is expected.
        r |= self.command_remaining.wrapping_sub(1) as u32;

        // XXX Implement bits [18:16]: current block

        r |= (self.output_bit15 as u32) << 23;
        r |= (self.output_signed as u32) << 24;
        r |= (self.output_depth as u32) << 25;

        // XXX Implement bits 27 and 28: DMA data in/out request

        // Command busy flag. XXX Probably set for a little while
        // after the last parameter is received whilet he command is
        // being processed?
        let command_pending =
            *self.command_handler as usize != MDec::handle_command as usize;

        r |= (command_pending as u32) << 29;

        // XXX Implement bit 30: data in FIFO full
        r |= 0 << 30;
        // XXX Implement bit 31: data out FIFO empty
        r |= 1 << 31;

        r
    }

    /// Handle writes to the command register
    pub fn command(&mut self, shared: &mut SharedState, cmd: u32) {

        module_tracer("MDEC", |m| {
            m.trace(shared.tk().now(),
                    "command_word",
                    cmd);
        });

        self.command_remaining -= 1;

        (self.command_handler)(self, cmd);

        if self.command_remaining == 0 {
            *self.command_handler = MDec::handle_command;
            self.command_remaining = 1;
        }
    }

    /// Return true if we're currently configured to decode luma-only
    /// blocks (i.e. 4 or 8bpp blocks)
    pub fn is_monochrome(&self) -> bool {
        match self.output_depth {
            OutputDepth::D4Bpp | OutputDepth::D8Bpp => true,
            _ => false
        }
    }

    fn handle_command(&mut self, cmd: u32) {
        let opcode = cmd >> 29;

        // Those internal variables (accessible through the status
        // register) are updated no matter the opcode. They're
        // probably meaningless for table loading opcodes though (most
        // likely full 0s).
        self.output_depth =
            match (cmd >> 27) & 3 {
                0 => OutputDepth::D4Bpp,
                1 => OutputDepth::D8Bpp,
                2 => OutputDepth::D24Bpp,
                3 => OutputDepth::D15Bpp,
                _ => unreachable!(),
            };
        self.output_signed = (cmd >> 26) & 1 != 0;
        self.output_bit15 = (cmd >> 25) & 1 != 0;

        let (len, handler): (u16, fn(&mut MDec, u32)) =
            match opcode {
                1 => {
                    let block_len = cmd & 0xffff;

                    self.current_block = BlockType::CrMono;
                    self.block_index = 0;

                    (block_len as u16, MDec::handle_encoded_word)
                }
                // Set quantization matrices. Bit 0 tells us whether we're
                // setting only the luma table or luma + chroma.
                2 => match cmd & 1 != 0 {
                    true => (32, MDec::handle_color_quant_matrices),
                    false => (16, MDec::handle_monochrome_quant_matrix),
                },
                3 => (32, MDec::handle_idct_matrix),
                n => panic!("Unsupported MDEC opcode {} ({:08x})", n, cmd),
            };

        self.command_remaining = len;
        *self.command_handler = handler;
    }

    fn handle_encoded_word(&mut self, cmd: u32) {
        self.decode_rle(cmd as u16);
        self.decode_rle((cmd >> 16) as u16);
    }

    fn decode_rle(&mut self, rle: u16) {
        println!("rle: {:04x}", rle);

        if self.block_index == 0 {
            if rle == 0xfe00 {
                // This is normally the end-of-block marker but if it
                // occurs before the start of a block it's just padding
                // and we should ignore it.
                return;
            }

            // The first value in the block is the DC value (low 10
            // bits) and the AC quantization scaling factor (high 6
            // bits)
            self.qscale = (rle >> 10) as u8;
            let dc = rle & 0x3ff;

            let dc = quantize(dc, self.quantization(), None);
            self.next_block_coeff(dc);
            self.block_index = 1;
        } else {
            if rle == 0xfe00 {
                // End-of-block marker
                while self.block_index < 8 * 8 {
                    self.next_block_coeff(0);
                }
            } else {
                // Decode RLE encoded block
                let zeroes = rle >> 10;
                let ac = rle & 0x3ff;

                // Fill the zeroes
                for _ in 0..zeroes {
                    self.next_block_coeff(0);
                }

                // Compute the value of the AC coefficient
                let ac = quantize(ac, self.quantization(), Some(self.qscale));
                self.next_block_coeff(ac);
            }

            if self.block_index == 8 * 8 {
                // Block full, moving on
                self.next_block();
            }
        }
    }

    /// Return the quantization factor for the current block_index
    fn quantization(&self) -> u8 {
        let index = self.block_index as usize;

        let matrix =
            if self.is_monochrome() {
                0
            } else {
                match self.current_block {
                    BlockType::CrMono | BlockType::Cb => 1,
                    _ => 0,
                }
            };

        self.quant_matrices[matrix][index]
    }

    fn next_block(&mut self) {
        if self.is_monochrome() {
            self.idct_matrix.idct(&self.block_coeffs, &mut self.block_y);

            panic!();
        } else {
            let (idct_target, generate_pixels, next_block) =
                match self.current_block {
                    BlockType::Y1 =>
                        (&mut self.block_y, true, BlockType::Y2),
                    BlockType::Y2 =>
                        (&mut self.block_y, true, BlockType::Y3),
                    BlockType::Y3 =>
                        (&mut self.block_y, true, BlockType::Y4),
                    BlockType::Y4 =>
                        (&mut self.block_y, true, BlockType::CrMono),
                    BlockType::CrMono =>
                        (&mut self.block_v, false, BlockType::Cb),
                    BlockType::Cb =>
                        (&mut self.block_u, false, BlockType::Y1),
                };

            self.idct_matrix.idct(&self.block_coeffs, idct_target);

            println!("{:?}", self.current_block);

            for y in 0..8 {
                for x in 0..8 {
                    print!(" {:02x}", idct_target[y * 8 + x]);
                }
                println!("");
            }

            if generate_pixels {
                // We have Y, U and V macroblocks, we can convert the
                // value and generate RGB pixels
                //unimplemented!();
            }

            self.current_block = next_block;
        }

        // We're ready for the next block's coefficients
        self.block_index = 0;
    }

    /// Set the value of the current block coeff pointed to by
    /// `block_index` and increment `block_index`.
    fn next_block_coeff(&mut self, coeff: i16) {
        self.block_coeffs.set_zigzag(self.block_index, coeff);
        self.block_index += 1;
    }

    fn handle_color_quant_matrices(&mut self, cmd: u32) {
        let index = (31 - self.command_remaining) as usize;

        let matrix = index / 16;
        let index = (index % 16) * 4;

        for i in 0..4 {
            let b = (cmd >> (i * 8)) as u8;

            self.quant_matrices[matrix][index + i] = b;
        }
    }

    fn handle_monochrome_quant_matrix(&mut self, cmd: u32) {
        let index = (15 - self.command_remaining) as usize;

        let index = index * 4;

        for i in 0..4 {
            let b = (cmd >> (i * 8)) as u8;

            self.quant_matrices[0][index + i] = b;
        }
    }

    fn handle_idct_matrix(&mut self, cmd: u32) {
        let index = (31 - self.command_remaining) as usize;

        let index = index * 2;

        let c1 = cmd as i16;
        let c2 = (cmd >> 16) as i16;

        // The loss of precision in the bitshift looks suspicious to
        // me but that's what mednafen does. Probably worth
        // investigating on the real hardware.
        self.idct_matrix[index]     = c1 >> 3;
        self.idct_matrix[index + 1] = c2 >> 3;
    }

    /// Set the value of the control register
    fn set_control(&mut self, val: u32) {
        let reset = val & (1 << 31) != 0;

        self.dma_in_enable = val & (1 << 30) != 0;
        self.dma_out_enable = val & (1 << 29) != 0;

        if reset {
            // XXX Does this reset anything else? DMA IN/DMA OUT
            // flags for instance? How about the various tables?

            // XXX clear FIFOs
            self.output_depth = OutputDepth::D4Bpp;
            self.output_signed = false;
            self.output_bit15 = false;
            self.current_block = BlockType::CrMono;
            *self.command_handler = MDec::handle_command;
            self.command_remaining = 1;
        }
    }
}

callback!(struct CommandHandler (fn(&mut MDec, u32)) {
    MDec::handle_command,
    MDec::handle_color_quant_matrices,
    MDec::handle_monochrome_quant_matrix,
});

/// Serializable container for the quantization matrices
buffer!(struct QuantMatrix([u8; 64]));

/// Serializable container for a macroblock
buffer!(struct Macroblock([i8; 8 * 8]));

/// Coefficients during macroblock decoding
buffer!(struct MacroblockCoeffs([i16; 8 * 8]));

impl MacroblockCoeffs {
    /// RLE-encoded values are encoded using a "zigzag" pattern in
    /// order to maximise the number of consecutive zeroes.
    fn set_zigzag(&mut self, pos: u8, coeff: i16) {
        // Zigzag LUT
        let zigzag: [u8; 64] = [
            0x00, 0x01, 0x08, 0x10, 0x09, 0x02, 0x03, 0x0a,
            0x11, 0x18, 0x20, 0x19, 0x12, 0x0b, 0x04, 0x05,
            0x0c, 0x13, 0x1a, 0x21, 0x28, 0x30, 0x29, 0x22,
            0x1b, 0x14, 0x0d, 0x06, 0x07, 0x0e, 0x15, 0x1c,
            0x23, 0x2a, 0x31, 0x38, 0x39, 0x32, 0x2b, 0x24,
            0x1d, 0x16, 0x0f, 0x17, 0x1e, 0x25, 0x2c, 0x33,
            0x3a, 0x3b, 0x34, 0x2d, 0x26, 0x1f, 0x27, 0x2e,
            0x35, 0x3c, 0x3d, 0x36, 0x2f, 0x37, 0x3e, 0x3f ];

        if pos >= 64 {
            // XXX Not sure how the MDEC deals with index
            // overflows. Does it wrap around somehow? Does it move to
            // the next block?
            panic!("Block index overflow!");
        }

        let index = zigzag[pos as usize];

        self[index as usize] = coeff
    }
}

/// Serializable container for the IDCT matrix
buffer!(struct IdctMatrix([i16; 64]));

impl IdctMatrix {
    /// Compute the Inverse Discrete Cosine Transform of `coeffs` and
    /// store the result in `block`
    fn idct(&self, coeffs: &MacroblockCoeffs, block: &mut Macroblock) {
        // XXX This function could greatly benefit from SIMD code when
        // Rust supports it. The full IDCT takes 1024 multiplications.
        let mut block_tmp = [0i16; 8 * 8];

        // First pass, store intermediate results in `block_tmp`
        for y in 0..8 {
            for x in 0..8 {
                let mut sum = 0i32;

                for c in 0..8 {
                    let coef = coeffs[y * 8 + c] as i32;

                    // XXX what happens in case of overflow? Should
                    // test on real hardware.
                    sum += coef * self[c * 8 + x] as i32
                }

                let v = (sum + 0x4000) >> 15;

                block_tmp[x * 8 + y] = v as i16;
            }
        }

        // 2nd pass, saturate the values into `block`
        for y in 0..8 {
            for x in 0..8 {
                let mut sum = 0i32;

                for c in 0..8 {
                    let coef = block_tmp[y * 8 + c] as i32;

                    // XXX what happens in case of overflow? Should
                    // test on real hardware.
                    sum += coef * self[c * 8 + x] as i32
                }

                let v = (sum + 0x4000) >> 15;

                // Sign extend 9bit value
                let v = v as u16;
                let v = v << (16 - 9);
                let v = (v as i16) >> (16 - 9);

                // Saturate
                let v =
                    if v < -128 {
                        -128
                    } else if v > 127 {
                        127
                    } else {
                        v as i8
                    };

                block[y * 8 + x] = v;
            }
        }
    }
}

/// Pixel color depths supported by the MDEC
#[derive(Copy, Clone, PartialEq, Eq, Debug, RustcDecodable, RustcEncodable)]
enum OutputDepth {
    D4Bpp = 0,
    D8Bpp = 1,
    D15Bpp = 3,
    D24Bpp = 2,
}

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, RustcDecodable, RustcEncodable)]
enum BlockType {
    Y1 = 0,
    Y2 = 1,
    Y3 = 2,
    Y4 = 3,
    /// Luma (Y) for monochrome, Cr otherwise
    CrMono = 4,
    Cb = 5,
}

/// Convert `val` into a signed 10 bit value
fn to_10bit_signed(val: u16) -> i16 {
    ((val << 6) as i16) >> 6
}

/// Quantization function for the macroblock coefficients. For the DC
/// coeffs qscale should be None.
fn quantize(coef: u16, quantization: u8, qscale: Option<u8>) -> i16 {
    if coef == 0 {
        0
    } else {
        let c = to_10bit_signed(coef);
        let (qscale, qshift) =
            match qscale {
                Some(qs) => (qs, 3),
                // DC doesn't use the qscale value and does not
                // require right shifting after the multiplication.
                _ => (1, 0)
            };

        let q = quantization as i32 * qscale as i32;

        let c =
            if q == 0 {
                (c << 5) as i32
            } else {
                let c = c as i32;
                let c = (c * q) >> qshift;
                let c = c << 4;

                // This is from mednafen, not sure why this is
                // needed.
                if c < 0 {
                    c + 8
                } else {
                    c - 8
                }
            };

        // Saturate
        if c > 0x3fff {
            0x3fff
        } else if c < -0x4000 {
            -0x4000
        } else {
            c as i16
        }
    }
}

#[test]
fn test_quantize_dc() {
    // XXX These values are taken from Mednafen at the moment, it
    // would be better to validate against the real hardware.
    assert_eq!(quantize(0, 0, None), 0);
    assert_eq!(quantize(0, 255, None), 0);
    assert_eq!(quantize(1, 0, None), 32);
    assert_eq!(quantize(1, 1, None), 8);
    assert_eq!(quantize(1, 2, None), 24);
    assert_eq!(quantize(1, 255, None), 4072);
    assert_eq!(quantize(2, 0, None), 64);
    assert_eq!(quantize(5, 204, None), 16312);
    assert_eq!(quantize(5, 205, None), 16383);
    assert_eq!(quantize(5, 206, None), 16383);
    assert_eq!(quantize(5, 255, None), 16383);
    assert_eq!(quantize(512, 0, None), -16384);
    assert_eq!(quantize(512, 1, None), -8184);
    assert_eq!(quantize(512, 2, None), -16376);
    assert_eq!(quantize(589, 0, None), -13920);
    assert_eq!(quantize(589, 1, None), -6952);
    assert_eq!(quantize(589, 2, None), -13912);
    assert_eq!(quantize(1023, 255, None), -4072);
    assert_eq!(quantize(1023, 0, None), -32);
}

#[test]
fn test_quantize_ac() {
    // XXX These values are taken from Mednafen at the moment, it
    // would be better to validate against the real hardware.
    assert_eq!(quantize(0, 0, Some(0)), 0);
    assert_eq!(quantize(0, 0, Some(63)), 0);
    assert_eq!(quantize(0, 1, Some(1)), 0);
    assert_eq!(quantize(0, 255, Some(63)), 0);
    assert_eq!(quantize(1, 0, Some(0)), 32);
    assert_eq!(quantize(1, 1, Some(0)), 32);
    assert_eq!(quantize(1, 1, Some(1)), -8);
    assert_eq!(quantize(1, 1, Some(7)), -8);
    assert_eq!(quantize(1, 1, Some(8)), 8);
    assert_eq!(quantize(1, 1, Some(15)), 8);
    assert_eq!(quantize(1, 1, Some(16)), 24);
    assert_eq!(quantize(1, 39, Some(62)), 4824);
    assert_eq!(quantize(1, 255, Some(63)), 16383);
    assert_eq!(quantize(1, 255, Some(32)), 16312);
    assert_eq!(quantize(2, 0, Some(0)), 64);
    assert_eq!(quantize(511, 255, Some(63)), 16383);
    assert_eq!(quantize(512, 0, Some(0)), -16384);
    assert_eq!(quantize(1000, 0, Some(0)), -768);
    assert_eq!(quantize(1000, 2, Some(57)), -5464);
    assert_eq!(quantize(1000, 220, Some(27)), -16384);
    assert_eq!(quantize(1003, 80, Some(3)), -10072);
}

#[test]
fn test_idct() {
    let coeffs = MacroblockCoeffs::from_array([
        0, 257, 514, 771, 1028, 1285, 1542, 1799,
        8, 265, 522, 779, 1036, 1293, 1550, 1807,
        16, 273, 530, 787, 1044, 1301, 1558, 1815,
        24, 281, 538, 795, 1052, 1309, 1566, 1823,
        32, 289, 546, 803, 1060, 1317, 1574, 1831,
        40, 297, 554, 811, 1068, 1325, 1582, 1839,
        48, 305, 562, 819, 1076, 1333, 1590, 1847,
        56, 313, 570, 827, 1084, 1341, 1598, 1855]);

    // This is the "standard" IDCT table used in most PSX games
    let mut matrix = IdctMatrix::from_array([
        23170,  23170,  23170,  23170,  23170,  23170,  23170,  23170,
        32138,  27245,  18204,   6392,  -6393, -18205, -27246, -32139,
        30273,  12539, -12540, -30274, -30274, -12540,  12539,  30273,
        27245,  -6393, -32139, -18205,  18204,  32138,   6392, -27246,
        23170, -23171, -23171,  23170,  23170, -23171, -23171,  23170,
        18204, -32139,   6392,  27245, -27246,  -6393,  32138, -18205,
        12539, -30274,  30273, -12540, -12540,  30273, -30274,  12539,
        6392,  -18205,  27245, -32139,  32138, -27246,  18204,  -6393]);

    // The "weird" bitshift used by mednafen
    for b in matrix.iter_mut() {
        *b = *b >> 3;
    }

    let mut block = Macroblock::new();

    matrix.idct(&coeffs, &mut block);

    let expected = Macroblock::from_array([
        -128, -95,  71, -27,  38, -5,  22,   9,
         127,  96, -75,  27, -40,  4, -23, -10,
         127, -39,  30, -11,  16, -2,   9,   4,
        -117,  33, -26,   9, -14,  1,  -8,  -3,
          62, -18,  14,  -5,   7, -1,   4,   2,
         -52,  14, -11,   4,  -6,  1,  -4,  -2,
          21,  -6,   5,  -2,   3,  0,   1,   1,
         -14,   3,  -3,   1,  -1,  0,  -1,   0]);

    assert_eq!(expected, block);
}

use memory::Addressable;
use shared::SharedState;

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
    /// Buffer holding the block being currently decoded
    macroblock: MacroBlock,
    /// Current index in `macroblock`
    block_index: u8,
    /// Current block type
    block_type: BlockType,
    /// Quantization scaling factor for the current block
    quantization_factor: u8,
    /// Array of decoded component blocks: Cr, Cb, Y. Only Y is used
    /// for monochrome.
    component_blocks: [ComponentBlock; 3],
}

impl MDec {
    pub fn new() -> MDec {
        MDec {
            dma_in_enable: false,
            dma_out_enable: false,
            output_depth: OutputDepth::D4Bpp,
            output_signed: false,
            output_bit15: false,
            quant_matrices: [QuantMatrix::new(),
                             QuantMatrix::new()],
            idct_matrix: IdctMatrix::new(),
            command_handler: CommandHandler(MDec::handle_command),
            command_remaining: 1,
            macroblock: MacroBlock::new(),
            block_index: 0,
            block_type: BlockType::CrMono,
            quantization_factor: 0,
            component_blocks: [ComponentBlock::new(),
                               ComponentBlock::new(),
                               ComponentBlock::new()],
        }
    }

    pub fn load<T: Addressable>(&mut self,
                                 _: &mut SharedState,
                                 offset: u32) -> u32 {

        if T::size() != 4 {
            panic!("Unhandled MDEC load ({})", T::size());
        }

        match offset {
            4 => self.status(),
            _ => panic!("Unhandled MDEC load: {:08x}", offset),
        }
    }

    pub fn store<T: Addressable>(&mut self,
                                 _: &mut SharedState,
                                 offset: u32,
                                 val: u32) {

        if T::size() != 4 {
            panic!("Unhandled MDEC store ({})", T::size());
        }

        match offset {
            0 => self.command(val),
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

        r |= (self.block_type as u32) << 16;

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
            self.block_type = BlockType::CrMono;
            *self.command_handler = MDec::handle_command;
            self.command_remaining = 1;
        }
    }

    /// Parse a single RLE-encoded macroblock halfword
    fn parse_block_data(&mut self, v: u16) {
        debug!("BLOCK DATA {:04x}", v);

        self.rle_block_data(v);

        if self.block_index == 64 {
            // Current block is complete
            if self.output_depth.is_monochrome() {
                // XXX Should be easy, only one Y block at a time
                panic!("Handle monochrome block");
            } else {
                let next_type =
                    match self.block_type {
                        BlockType::CrMono => {
                            self.idct(Component::Cr);

                            debug!("DECODED Cr block");

                            BlockType::Cb
                        }
                        BlockType::Cb => {
                            self.idct(Component::Cb);

                            debug!("DECODED Cb block");

                            BlockType::Y1
                        }
                        BlockType::Y1 => {
                            self.idct(Component::Y);

                            panic!("DECODED Cb block");
                        }
                        t => panic!("Unhandled block type {:?}", t),
                    };

                self.block_type = next_type;
                self.block_index = 0;
            }
        }
    }

    /// Decode a single Run Length Encoded halfword
    fn rle_block_data(&mut self, v: u16) {

        // Figure out which quantization table to use based on the
        // color format and block type
        let qtable =
            match self.block_type {
                BlockType::CrMono | BlockType::Cb => 1,
                _ => 0,
            };

        let quant = self.quant_matrices[qtable][self.block_index as usize];

        // Saturate value to signed 11bit
        let saturate_s11 = |v| {
            if v < -0x400 {
                -0x400
            } else if v > 0x3ff {
                0x3ff
            } else {
                v as i16
            }
        };

        if self.block_index == 0 {
            if v == 0xfe00 {
                // This value before the start of a block denotes
                // padding, ignore
                return;
            }

            // The first value in a block contains the quantization
            // scaling factor in bits [15:11] and the (unscaled) DC
            // factor in [10:0].
            self.quantization_factor = (v >> 10) as u8;

            // Sign extend the 11 bit value
            let dc = ((v << 6) as i16) >> 6;

            // The DC factor has a slightly different formula than the
            // rest of the AC coefficients, quantization factor is not
            // used and we don't divide by 8.
            //
            // XXX Mednafen seems to add 4 bits of precision to the
            // coefficients (both here and AC).

            // Use 32bit arithmetics to handle overflows
            let dc = dc as i32;

            let dc =
                if quant == 0 {
                    dc << 1
                } else {
                    dc * quant as i32
                };

            let dc = saturate_s11(dc);

            self.macroblock.set_zigzag(0, dc);
            self.block_index = 1;
            return;
        }

        // Subsequent block values contain RLE-encoded "AC"
        // values.
        if v == 0xfe00 {
            // End-of-block marker, complete the current block
            // with zeroes.

            while self.block_index < 64 {
                self.macroblock.set_zigzag(self.block_index, 0);
                self.block_index += 1;
            }

            return;
        }

        // Zeroes are RLE-encoded. Bits [15:11] of the value
        // contains the number of preceeding zeroes.
        let zeroes = v >> 10;

        for _ in 0..zeroes {
            self.macroblock.set_zigzag(self.block_index, 0);
            self.block_index += 1;
        }

        // Sign extend the 11 bit value
        let ac = ((v << 6) as i16) >> 6;

        // Use 32bit arithmetics to handle overflows
        let ac = ac as i32;

        let q = quant as i32 * self.quantization_factor as i32;

        let ac =
            if q == 0 {
                ac << 1
            } else {
                (ac * q) >> 3
            };

        // Saturate result to 11bits
        let ac = saturate_s11(ac);

        self.macroblock.set_zigzag(self.block_index, ac);
        self.block_index += 1;
    }

    /// Compute the inverse discrete cosine transform of the current
    /// macroblock and store it in `component`'s buffer.
    fn idct(&mut self, component: Component) {
        // XXX This could probably be sped up significantly by using
        // SIMD, unfortunately there's no way to do so in stable rustc
        // 1.11 as far as I know.

        let mut tmp = MacroBlock::new();

        for i in 0..8 {
            for j in 0..8 {
                let mut sum = 0i32;

                for k in 0..8 {
                    // This is a simple matrix multiplication, but
                    // "macroblock" is mirorred diagonaly so we
                    // multiply column by column
                    let val = self.macroblock[k * 8 + j] as i32;
                    let coef = self.idct_matrix[k * 8 + i] as i32;

                    // XXX Doesn't make a lot of sense to divide coef
                    // by 8 before the multiplication here, but that's
                    // how No$ documents it. Need to run some tests on
                    // the real hardware to figure out how this works.
                    sum = sum + val * (coef / 8);
                }

                tmp[j * 8 + i] = ((sum + 0xfff) / 0x2000) as i16;
            }
        }

        let out = &mut self.component_blocks[component as usize];

        // 2nd pass
        for i in 0..8 {
            for j in 0..8 {
                let mut sum = 0i32;

                for k in 0..8 {
                    let val = tmp[k * 8 + j] as i32;
                    let coef = self.idct_matrix[k * 8 + i] as i32;

                    // XXX Doesn't make a lot of sense to divide coef
                    // by 8 before the multiplication here, but that's
                    // how No$ documents it. Need to run some tests on
                    // the real hardware to figure out how this works.
                    sum = sum + val * (coef / 8);
                }

                let sum = (sum + 0xfff) / 0x2000;

                // Truncate value to 9bit signed
                let sum = (sum as u32) << 21;
                let sum = (sum as i32) >> 21;

                // Saturate to 8bits
                out[j * 8 + i] =
                    if sum > 0x7f {
                        0x7f
                    } else if sum < -0x80 {
                        -0x80
                    } else {
                        sum as i8
                    };

                   print!("{}, ", out[j * 8 + i]);
            }

            println!("");
        }
    }

    /// Handle writes to the command register
    pub fn command(&mut self, cmd: u32) {
        self.command_remaining -= 1;

        (self.command_handler)(self, cmd);

        if self.command_remaining == 0 {
            *self.command_handler = MDec::handle_command;
            self.command_remaining = 1;
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
                // Decode macroblocks
                1 => {
                    // Number of words to be decoded
                    let len = cmd & 0xffff;

                    self.block_index = 0;

                    self.block_type = BlockType::CrMono;

                    (len as u16, MDec::handle_block_data)
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

        self.idct_matrix[index] = cmd as i16;
        self.idct_matrix[index + 1] = (cmd >> 16) as i16;
    }

    fn handle_block_data(&mut self, cmd: u32) {
        // Two 16-byte values packed in each word
        self.parse_block_data(cmd as u16);
        self.parse_block_data((cmd >> 16) as u16);
    }
}

callback!(struct CommandHandler(fn (&mut MDec, u32)) {
    MDec::handle_command,
    MDec::handle_color_quant_matrices,
    MDec::handle_monochrome_quant_matrix,
    MDec::handle_block_data,
});

/// Serializable container for the quantization matrices
buffer!(struct QuantMatrix([u8; 64]));

/// Serializable container for the IDCT matrix
buffer!(struct IdctMatrix([i16; 64]));

/// Serializable container for a component's 8x8 pixel macroblock.
buffer!(struct ComponentBlock([i8; 64]));

/// Serializable container for the decoded macroblock DCT coefficients
buffer!(struct MacroBlock([i16; 64]));

impl MacroBlock {
    /// RLE-encoded values are encoded using a "zigzag" pattern in
    /// order to maximise the number of consecutive zeroes.
    fn set_zigzag(&mut self, pos: u8, val: i16) {
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

        debug!("MACROBLOCK VAL {} {} {:04x}", pos, index, val);

        self[index as usize] = val
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

impl OutputDepth {
    fn is_monochrome(self) -> bool {
        match self {
            OutputDepth::D4Bpp | OutputDepth::D8Bpp => true,
            _ => false,
        }
    }
}

#[derive(Copy, Clone, Debug, RustcDecodable, RustcEncodable)]
enum BlockType {
    Y1 = 0,
    Y2 = 1,
    Y3 = 2,
    Y4 = 3,
    /// Luma (Y) for monochrome, Cr otherwise
    CrMono = 4,
    Cb = 5,
}

/// Enum used to index `component_blocks`.
#[derive(Copy, Clone, Debug)]
enum Component {
    Cr = 0,
    Cb = 1,
    Y  = 2,
}

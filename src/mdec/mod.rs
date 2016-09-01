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
    current_block: BlockType,
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
}

impl MDec {
    pub fn new() -> MDec {
        MDec {
            dma_in_enable: false,
            dma_out_enable: false,
            output_depth: OutputDepth::D4Bpp,
            output_signed: false,
            output_bit15: false,
            current_block: BlockType::CrLuma,
            quant_matrices: [QuantMatrix::new(),
                             QuantMatrix::new()],
            idct_matrix: IdctMatrix::new(),
            command_handler: CommandHandler(MDec::handle_command),
            command_remaining: 1,
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
                // Set quantization matrices. Bit 0 tells us whether we're
                // setting only the luma table or luma + chroma.
                2 => match cmd & 1 != 0 {
                    true => (32, MDec::handle_color_quant_matrices),
                    false => (16, MDec::handle_monochrome_quant_matrix),
                },
                3 => (32, MDec::handle_idct_matrix),
                n => {
                    warn!("Unsupported MDEC opcode {} ({:08x})", n, cmd);
                    (1, MDec::handle_command)
                }
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
            self.current_block = BlockType::CrLuma;
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

/// Serializable container for the IDCT matrix
buffer!(struct IdctMatrix([i16; 64]));

/// Pixel color depths supported by the MDEC
#[derive(Copy, Clone, PartialEq, Eq, Debug, RustcDecodable, RustcEncodable)]
enum OutputDepth {
    D4Bpp = 0,
    D8Bpp = 1,
    D15Bpp = 3,
    D24Bpp = 2,
}

#[allow(dead_code)]
#[derive(RustcDecodable, RustcEncodable)]
enum BlockType {
    Y1 = 0,
    Y2 = 1,
    Y3 = 2,
    Y4 = 3,
    /// Luma (Y) for monochrome, Cr otherwise
    CrLuma = 4,
    Cb = 5,
}

use memory::Addressable;
use shared::SharedState;

/// Motion Decoder (sometimes called macroblock or movie decoder).
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
    quant_matrices: [[u8; 64]; 2],
    /// Inverse discrete cosine transform matrix. No$ says that "all
    /// known games" use the same values for this matrix, so it could
    /// be possible to optimize the decoding for this particular
    /// table.
    idct_matrix: [i16; 64],
    /// Callback handling writes to the command register. Returns
    /// `false` when it receives the last word for the command.
    command_handler: fn (&mut MDec, u32),
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
            quant_matrices: [[0; 64]; 2],
            idct_matrix: [0; 64],
            command_handler: MDec::handle_command,
            command_remaining: 1,
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

    /// Handle writes to the command register
    pub fn command(&mut self, cmd: u32) {
        self.command_remaining -= 1;

        (self.command_handler)(self, cmd);

        if self.command_remaining == 0 {
            self.command_handler = MDec::handle_command;
            self.command_remaining = 1;
        }
    }

    fn handle_command(&mut self, cmd: u32) {
        let opcode = cmd >> 29;

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
                n => panic!("Unsupported MDEC opcode {} ({:08x})", n, cmd),
            };

        self.command_remaining = len;
        self.command_handler = handler;
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
            self.command_handler = MDec::handle_command;
            self.command_remaining = 1;
        }
    }
}

/// Pixel color depths supported by the MDEC
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum OutputDepth {
    D4Bpp = 0,
    D8Bpp = 1,
    D15Bpp = 3,
    D24Bpp = 2,
}

#[allow(dead_code)]
enum BlockType {
    Y1 = 0,
    Y2 = 1,
    Y3 = 2,
    Y4 = 3,
    /// Luma (Y) for monochrome, Cr otherwise
    CrLuma = 4,
    Cb = 5,
}

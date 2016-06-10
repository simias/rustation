//! Basic MIPS assembler to generate PlayStation machine code in pure
//! rust

use std::collections::HashMap;

pub mod syntax {
    #[derive(Clone, Copy)]
    pub struct Register(pub u8);

    #[derive(Clone, Copy)]
    pub enum Label {
        Local(&'static str, char),
        Global(&'static str),
        Absolute(u32),
    }

    #[derive(Clone, Copy)]
    pub enum Instruction {
        Sll(Register, Register, u8),
        Srl(Register, Register, u8),
        Sra(Register, Register, u8),
        Sllv(Register, Register, Register),
        Srlv(Register, Register, Register),
        Srav(Register, Register, Register),
        Jr(Register),
        Jalr(Register, Register),
        Syscall(u32),
        Break(u32),
        Mfhi(Register),
        Mthi(Register),
        Mflo(Register),
        Mtlo(Register),
        Mult(Register, Register),
        Multu(Register, Register),
        Div(Register, Register),
        Divu(Register, Register),
        Add(Register, Register, Register),
        Addu(Register, Register, Register),
        Sub(Register, Register, Register),
        Subu(Register, Register, Register),
        And(Register, Register, Register),
        Or(Register, Register, Register),
        Xor(Register, Register, Register),
        Nor(Register, Register, Register),
        Slt(Register, Register, Register),
        Sltu(Register, Register, Register),
        Bgez(Register, Label),
        Bltz(Register, Label),
        Bgezal(Register, Label),
        Bltzal(Register, Label),
        J(Label),
        Jal(Label),
        Beq(Register, Register, Label),
        Bne(Register, Register, Label),
        Blez(Register, Label),
        Bgtz(Register, Label),
        Addi(Register, Register, i16),
        Addiu(Register, Register, i16),
        Slti(Register, Register, i16),
        Sltiu(Register, Register, i16),
        Andi(Register, Register, u16),
        Ori(Register, Register, u16),
        Xori(Register, Register, u16),
        Lui(Register, u16),
        Lb(Register, Register, i16),
        Lh(Register, Register, i16),
        Lwl(Register, Register, i16),
        Lw(Register, Register, i16),
        Lbu(Register, Register, i16),
        Lhu(Register, Register, i16),
        Lwr(Register, Register, i16),
        Sb(Register, Register, i16),
        Sh(Register, Register, i16),
        Swl(Register, Register, i16),
        Sw(Register, Register, i16),
        Swr(Register, Register, i16),

        // Coprocessor opcodes
        Mfc0(Register, u8),
        Mtc0(Register, u8),

        /// Global labels: can't be redefined
        Global(&'static str),
        /// Local labels: can be redefined
        Local(&'static str),

        /// Add padding (if necessary) to reach the desired byte
        /// alignment expressed as a power of two. E.g. Align(2)
        /// aligns on 4 bytes.
        Align(u8),

        // Pseudo-instructions
        Nop,
        Move(Register, Register),
        /// Load immediate, can take two instructions if the immediate
        /// has low and high halfword both non-zero.
        Li(Register, u32),
        /// Load address: always takes two instructions
        La(Register, Label),
        B(Label),
        Beqz(Register, Label),
        Bnez(Register, Label),
    }

    impl Instruction {
        // Length of the instruction in bytes
        pub fn bytes(&self, here: u32) -> u32 {
            match *self {
                Local(_) | Global(_) => 0,
                Li(_, v) => {
                    let mut b = 0;

                    if v & 0xffff0000 != 0{
                        b += 4;
                    }

                    if v & 0xffff != 0 || v == 0 {
                        b += 4;
                    }

                    b
                }
                La(..) => 8,
                Align(o) => {
                    super::pad_to_order(here, o)
                }
                _ => 4,
            }
        }
    }

    pub use self::Instruction::*;

    pub const R0: Register = Register(0);
    pub const R1: Register = Register(1);
    pub const R2: Register = Register(2);
    pub const R3: Register = Register(3);
    pub const R4: Register = Register(4);
    pub const R5: Register = Register(5);
    pub const R6: Register = Register(6);
    pub const R7: Register = Register(7);
    pub const R8: Register = Register(8);
    pub const R9: Register = Register(9);
    pub const R10: Register = Register(10);
    pub const R11: Register = Register(11);
    pub const R12: Register = Register(12);
    pub const R13: Register = Register(13);
    pub const R14: Register = Register(14);
    pub const R15: Register = Register(15);
    pub const R16: Register = Register(16);
    pub const R17: Register = Register(17);
    pub const R18: Register = Register(18);
    pub const R19: Register = Register(19);
    pub const R20: Register = Register(20);
    pub const R21: Register = Register(21);
    pub const R22: Register = Register(22);
    pub const R23: Register = Register(23);
    pub const R24: Register = Register(24);
    pub const R25: Register = Register(25);
    pub const R26: Register = Register(26);
    pub const R27: Register = Register(27);
    pub const R28: Register = Register(28);
    pub const R29: Register = Register(29);
    pub const R30: Register = Register(30);
    pub const R31: Register = Register(31);

    pub const AT: Register = R1;
    pub const V0: Register = R2;
    pub const V1: Register = R3;
    pub const A0: Register = R4;
    pub const A1: Register = R5;
    pub const A2: Register = R6;
    pub const A3: Register = R7;
    pub const T0: Register = R8;
    pub const T1: Register = R9;
    pub const T2: Register = R10;
    pub const T3: Register = R11;
    pub const T4: Register = R12;
    pub const T5: Register = R13;
    pub const T6: Register = R14;
    pub const T7: Register = R15;
    pub const S0: Register = R16;
    pub const S1: Register = R17;
    pub const S2: Register = R18;
    pub const S3: Register = R19;
    pub const S4: Register = R20;
    pub const S5: Register = R21;
    pub const S6: Register = R22;
    pub const S7: Register = R23;
    pub const T8: Register = R24;
    pub const T9: Register = R25;
    pub const K0: Register = R26;
    pub const K1: Register = R27;
    pub const GP: Register = R28;
    pub const SP: Register = R29;
    pub const FP: Register = R30;
    pub const RA: Register = R31;
}

use self::syntax::*;

/// Assembler state
pub struct Assembler {
    /// Currently generated machine code
    machine_code: Vec<u8>,
    /// Address of the first instruction
    base: u32,
    /// Hash table containing the absolute address of all known global
    /// labels. Global labels are unique and can't be redefined.
    globals: HashMap<&'static str, u32>,
    /// List of all the local labels with their absolute
    /// address. Local labels can be redefined.
    locals: Vec<(u32, &'static str)>,
}

impl Assembler {
    /// Create a new assembler instance which will generate code meant
    /// to be loaded at the `base` address
    pub fn from_base(base: u32) -> Assembler {
        Assembler {
            machine_code: Vec::new(),
            base: base,
            globals: HashMap::new(),
            locals: Vec::new(),
        }
    }

    /// Attempt to assemble the provided instructions. Returns the
    /// size of the generated machine code in bytes on success, a
    /// String describing the assembler error on failure.
    pub fn assemble(&mut self,
                    instructions: &[Instruction]) -> Result<u32, String> {
        let start_loc = self.location();

        // Clear local labels, seems convenient?
        self.locals.clear();

        // First we map the labels
        try!(self.parse_labels(instructions));

        for &i in instructions {
            try!(self.assemble_instruction(i));
        }

        Ok(self.location() - start_loc)
    }

    /// Consume the Assembler and return the generated machine code
    /// alongside the base address
    pub fn machine_code(self) -> (Vec<u8>, u32) {
        (self.machine_code, self.base)
    }

    fn location(&self) -> u32 {
        self.base + self.machine_code.len() as u32
    }

    /// Look for global and local labels in `instructions` and collect
    /// them
    fn parse_labels(&mut self,
                    instructions: &[Instruction]) -> Result<(), String> {
        let mut loc = self.location();

        for &i in instructions {
            match i {
                Global(name) =>
                    if let Some(_) = self.globals.insert(name, loc) {
                        // Globals can't be redefined
                        return Err(
                            format!("Global label '{}' is redefined", name));
                    },
                // Locals can be redefined any number of times
                Local(id) => self.locals.push((loc, id)),
                _ => loc += i.bytes(loc),
            }
        }

        Ok(())
    }

    fn label_address(&self, label: Label) -> Result<u32, String> {
        match label {
            Label::Global(l) =>
                match self.globals.get(l) {
                    Some(&v) => Ok(v),
                    None => Err(format!("Unknown global label '{}'", l)),
                },
            Label::Local(l, d) => {
                let here = self.location();

                // Search for the closest label with name `l` in the
                // relative direction gived by `d`
                let v =
                    match d {
                        // Search for the closest label *before* (and
                        // including) `here`. We start by the end and
                        // iterate backwards.
                        'b' => self.locals.iter().rev()
                            .find(|&&(addr, name)| {
                                addr <= here && l == name
                            }),
                        // Search for the closest label *after* `here`
                        'f' => self.locals.iter()
                            .find(|&&(addr, name)| {
                                addr > here && l == name
                            }),
                        _ => return
                            Err(format!("Unknown local label direction '{}'",
                                        d)),
                    };

                match v {
                    Some(&(addr, _)) => Ok(addr),
                    None => Err(format!("Unknown local label '{}'", l)),
                }
            },
            Label::Absolute(a) => Ok(a),
        }
    }

    fn branch_target(&self, label: Label) -> Result<i16, String> {
        // The offset is relative to the *next* instruction
        let here = (self.location() + 4) as i32;

        let there = try!(self.label_address(label)) as i32;

        let delta = (there - here) as i16;

        // 2 MSBs are truncated since PC addresses are always word aligned
        Ok(delta >> 2)
    }

    fn jump_target(&self, label: Label) -> Result<u32, String> {
        let there = try!(self.label_address(label));

        // 2 MSBs are truncated since PC addresses are always word aligned
        Ok(there >> 2)
    }

    fn assemble_instruction(&mut self,
                            instruction: Instruction) -> Result<(), String> {
        match instruction {
            Sll(r0, r1, shift) =>
                self.emit_code(MachineCode::sub(0b000000)
                               .d(r0)
                               .t(r1)
                               .shift(shift)),
            Srl(r0, r1, shift) =>
                self.emit_code(MachineCode::sub(0b000010)
                               .d(r0)
                               .t(r1)
                               .shift(shift)),
            Sra(r0, r1, shift) =>
                self.emit_code(MachineCode::sub(0b000011)
                               .d(r0)
                               .t(r1)
                               .shift(shift)),
            Sllv(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b000100)
                               .d(r0)
                               .t(r1)
                               .s(r2)),
            Srlv(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b000110)
                               .d(r0)
                               .t(r1)
                               .s(r2)),
            Srav(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b000111)
                               .d(r0)
                               .t(r1)
                               .s(r2)),
            Jr(r0) =>
                self.emit_code(MachineCode::sub(0b001000)
                               .s(r0)),

            Jalr(r0, r1) =>
                self.emit_code(MachineCode::sub(0b001001)
                               .d(r0)
                               .s(r1)),
            Syscall(v) =>
                self.emit_code(MachineCode::sub(0b001100)
                               .sys_brk(v)),
            Break(v) =>
                self.emit_code(MachineCode::sub(0b001101)
                               .sys_brk(v)),
            Mfhi(r0) =>
                self.emit_code(MachineCode::sub(0b010000)
                               .d(r0)),
            Mthi(r0) =>
                self.emit_code(MachineCode::sub(0b010001)
                               .s(r0)),
            Mflo(r0) =>
                self.emit_code(MachineCode::sub(0b010010)
                               .d(r0)),
            Mtlo(r0) =>
                self.emit_code(MachineCode::sub(0b010011)
                               .s(r0)),
            Mult(r0, r1) =>
                self.emit_code(MachineCode::sub(0b011000)
                               .s(r0)
                               .t(r1)),
            Multu(r0, r1) =>
                self.emit_code(MachineCode::sub(0b011001)
                               .s(r0)
                               .t(r1)),
            Div(r0, r1) =>
                self.emit_code(MachineCode::sub(0b011010)
                               .s(r0)
                               .t(r1)),
            Divu(r0, r1) =>
                self.emit_code(MachineCode::sub(0b011011)
                               .s(r0)
                               .t(r1)),
            Add(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100000)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Addu(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100001)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Sub(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100010)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Subu(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100011)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            And(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100100)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Or(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100101)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Xor(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100110)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Nor(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b100111)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Slt(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b101010)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Sltu(r0, r1, r2) =>
                self.emit_code(MachineCode::sub(0b101011)
                               .d(r0)
                               .s(r1)
                               .t(r2)),
            Bgez(r0, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000001)
                               .is_link(false)
                               .is_bgez(true)
                               .s(r0)
                               .imm_se(i));
            }
            Bltz(r0, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000001)
                               .is_link(false)
                               .is_bgez(false)
                               .s(r0)
                               .imm_se(i));
            }
            Bgezal(r0, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000001)
                               .is_link(true)
                               .is_bgez(true)
                               .s(r0)
                               .imm_se(i));
            }
            Bltzal(r0, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000001)
                               .is_link(true)
                               .is_bgez(false)
                               .s(r0)
                               .imm_se(i));
            }
            J(l) => {
                let i = try!(self.jump_target(l));

                self.emit_code(MachineCode::op(0b000010)
                               .imm_jump(i));
            }
            Jal(l) => {
                let i = try!(self.jump_target(l));

                self.emit_code(MachineCode::op(0b000011)
                               .imm_jump(i));
            }
            Beq(r0, r1, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000100)
                               .s(r0)
                               .t(r1)
                               .imm_se(i));
            }
            Bne(r0, r1, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000101)
                               .s(r0)
                               .t(r1)
                               .imm_se(i));
            }
            Blez(r0, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000110)
                               .s(r0)
                               .imm_se(i));
            }
            Bgtz(r0, l) => {
                let i = try!(self.branch_target(l));

                self.emit_code(MachineCode::op(0b000111)
                               .s(r0)
                               .imm_se(i));
            }
            Addi(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b001000)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Addiu(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b001001)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Slti(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b001010)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Sltiu(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b001011)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Andi(r0, r1, u) => {
                self.emit_code(MachineCode::op(0b001100)
                               .t(r0)
                               .s(r1)
                               .imm(u));
            }
            Ori(r0, r1, u) => {
                self.emit_code(MachineCode::op(0b001101)
                               .t(r0)
                               .s(r1)
                               .imm(u));
            }
            Xori(r0, r1, u) => {
                self.emit_code(MachineCode::op(0b001110)
                               .t(r0)
                               .s(r1)
                               .imm(u));
            }
            Lui(r0, u) => {
                self.emit_code(MachineCode::op(0b001111)
                               .t(r0)
                               .imm(u));
            }
            Lb(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100000)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Lh(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100001)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Lwl(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100010)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Lw(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100011)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Lbu(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100100)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Lhu(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100101)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Lwr(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b100110)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Sb(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b101000)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Sh(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b101001)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Swl(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b101010)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Sw(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b101011)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Swr(r0, r1, i) => {
                self.emit_code(MachineCode::op(0b101110)
                               .t(r0)
                               .s(r1)
                               .imm_se(i));
            }
            Mfc0(r0, cop_r) => {
                self.emit_code(MachineCode::op(0b010000)
                               .cop_opcode(0b00000)
                               .t(r0)
                               .cop_r(cop_r))
            }
            Mtc0(r0, cop_r) => {
                self.emit_code(MachineCode::op(0b010000)
                               .cop_opcode(0b00100)
                               .t(r0)
                               .cop_r(cop_r))
            }

            /// Alignment padding
            Align(o) =>
                for _ in 0..pad_to_order(self.location(), o) {
                    self.emit_byte(0);
                },

            // Pseudo instructions
            Nop =>
                try!(self.assemble_instruction(Sll(R0, R0, 0))),
            Move(r0, r1) =>
                try!(self.assemble_instruction(Addu(r0, r1, R0))),
            Li(r0, v) => {
                let hi = (v >> 16) as u16;
                let lo = v as u16;

                if hi != 0 {
                    try!(self.assemble_instruction(Lui(r0, hi)));

                    if lo != 0 {
                        try!(self.assemble_instruction(Ori(r0, r0, lo)));
                    }
                } else {
                    try!(self.assemble_instruction(Ori(r0, R0, lo)));
                }
            }
            La(r0, l) => {
                let loc = try!(self.label_address(l));

                let hi = (loc >> 16) as u16;
                let lo = loc as u16;

                try!(self.assemble_instruction(Lui(r0, hi)));
                try!(self.assemble_instruction(Ori(r0, r0, lo)));
            }
            B(l) =>
                try!(self.assemble_instruction(Beq(R0, R0, l))),
            Beqz(r0, l) =>
                try!(self.assemble_instruction(Beq(r0, R0, l))),
            Bnez(r0, l) =>
                try!(self.assemble_instruction(Bne(r0, R0, l))),

            // Labels should already have been handled
            Local(..) | Global(..) => (),
        }

        Ok(())
    }

    fn emit_byte(&mut self, b: u8) {
        self.machine_code.push(b);
    }

    fn emit_code(&mut self, code: MachineCode) {
        let word = code.0;

        self.emit_byte(word as u8);
        self.emit_byte((word >> 8) as u8);
        self.emit_byte((word >> 16) as u8);
        self.emit_byte((word >> 24) as u8);
    }
}

#[derive(Clone, Copy)]
struct MachineCode(u32);

impl MachineCode {
    fn op(op: u8) -> MachineCode {
        MachineCode((op as u32) << 26)
    }

    fn sub(op: u8) -> MachineCode {
        MachineCode(op as u32)
    }

    fn cop_opcode(self, op: u32) -> MachineCode {
        MachineCode(self.0 | (op << 21))
    }

    fn s(self, r: Register) -> MachineCode {
        MachineCode(self.0 | ((r.0 as u32) << 21))
    }

    fn t(self, r: Register) -> MachineCode {
        MachineCode(self.0 | ((r.0 as u32) << 16))
    }

    fn d(self, r: Register) -> MachineCode {
        MachineCode(self.0 | ((r.0 as u32) << 11))
    }

    fn cop_r(self, cop_r: u8) -> MachineCode {
        MachineCode(self.0 | ((cop_r as u32) << 11))
    }

    fn shift(self, s: u8) -> MachineCode {
        MachineCode(self.0 | ((s as u32) << 6))
    }

    fn sys_brk(self, v: u32) -> MachineCode {
        MachineCode(self.0 | ((v & 0xfffff) << 6))
    }

    fn is_link(self, is_link: bool) -> MachineCode {
        MachineCode(self.0 | ((is_link as u32) << 20))
    }

    fn is_bgez(self, is_bgez: bool) -> MachineCode {
        MachineCode(self.0| ((is_bgez as u32) << 16))
    }

    fn imm(self, v: u16) -> MachineCode {
        MachineCode(self.0 | (v as u32))
    }

    fn imm_se(self, v: i16) -> MachineCode {
        MachineCode(self.0 | (v as u16 as u32))
    }

    fn imm_jump(self, v: u32) -> MachineCode {
        MachineCode(self.0 | (v & 0x3ffffff))
    }
}

/// Return the number of bytes necessary to add after `loc` in order
/// to reach an address aligned on `1 << order`
fn pad_to_order(loc: u32, order: u8) -> u32 {
    let mask = (1u32 << order) - 1;

    ((!loc).wrapping_add(1)) & mask
}

#[test]
fn opcodes() {
    fn test_instruction(instruction: Instruction, expected: &[u8]) {
        let mut asm = Assembler::from_base(0);

        asm.assemble(&[instruction]).unwrap();

        let (mc, _) = asm.machine_code();

        println!("{:?}", mc);

        assert!(mc == expected);
    }

    let tests = [
        (Nop,                         [0, 0, 0, 0]),
        (Sll(R0, R0, 0),              [0, 0, 0, 0]),
        (Lui(T5, 0xdead),             [0xad, 0xde, 0x0d, 0x3c]),
        (Ori(T5, T5, 0xbeef),         [0xef, 0xbe, 0xad, 0x35]),
        (Break(0x1234),               [0x0d, 0x8d, 0x04, 0x00]),
        (Jal(Label::Absolute(0xabc)), [0xaf, 0x02, 0x00, 0x0c]),
    ];

    for &(instruction, ref expected) in &tests {
        test_instruction(instruction, expected);
    }
}

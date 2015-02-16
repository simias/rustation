use std::fmt::{Debug, Formatter, Error};

use memory::Interconnect;

mod asm;

/// CPU state
pub struct Cpu {
    /// The program counter register
    pc: u32,
    /// Next instruction to be executed, used to simulate the branch
    /// delay slot
    next_instruction: Instruction,
    /// General Purpose Registers. The first entry must always contain 0
    regs: [u32; 32],
    /// Memory interface
    inter: Interconnect,
}

impl Cpu {

    pub fn new(inter: Interconnect) -> Cpu {
        // Not sure what the reset values are...
        let mut regs = [0xdeadbeef; 32];

        // ... but R0 is hardwired to 0
        regs[0] = 0;

        Cpu {
            // PC reset value at the beginning of the BIOS
            pc: 0xbfc00000,
            regs: regs,
            inter: inter,
            // Start execution with a NOP while the real first
            // instruction is fetched.
            next_instruction: Instruction(0x0),
        }
    }

    pub fn run_next_instruction(&mut self) {
        let pc = self.pc;

        // Use previously loaded instruction
        let instruction = self.next_instruction;

        // Fetch instruction at PC
        self.next_instruction = Instruction(self.load32(pc));

        // Increment PC to point to the next instruction. All
        // instructions are 32bit long.
        self.pc = pc.wrapping_add(4);

        self.decode_and_execute(instruction);
    }

    /// Load 32bit value from the memory
    fn load32(&self, addr: u32) -> u32 {
        self.inter.load32(addr)
    }

    /// Store 32bit value into the memory
    fn store32(&mut self, addr: u32, val: u32) {
        self.inter.store32(addr, val);
    }

    fn decode_and_execute(&mut self, instruction: Instruction) {
        match instruction.function() {
            0b000000 => match instruction.subfunction() {
                0b000000 => self.op_sll(instruction),
                0b100101 => self.op_or(instruction),
                _        => panic!("Unhandled opcode {:08x}", instruction.0),
            },
            0b000010 => self.op_j(instruction),
            0b001001 => self.op_addiu(instruction),
            0b001101 => self.op_ori(instruction),
            0b001111 => self.op_lui(instruction),
            0b101011 => self.op_sw(instruction),
            _        => panic!("Unhandled opcode {:08x}", instruction.0),
        }
    }

    fn reg(&self, index: RegisterIndex) -> u32 {
        self.regs[index.0 as usize]
    }

    fn set_reg(&mut self, index: RegisterIndex, val: u32) {
        self.regs[index.0 as usize] = val;

        // Make sure R0 is always 0
        self.regs[0] = 0;
    }

    /// Shift Left Logical
    fn op_sll(&mut self, instruction: Instruction) {
        let i = instruction.shift();
        let t = instruction.t();
        let d = instruction.d();

        let v = self.reg(t) << i;

        self.set_reg(d, v);
    }

    /// Bitwise Or
    fn op_or(&mut self, instruction: Instruction) {
        let d = instruction.d();
        let s = instruction.s();
        let t = instruction.t();

        let v = self.reg(s) | self.reg(t);

        self.set_reg(d, v);
    }

    /// Jump
    fn op_j(&mut self, instruction: Instruction) {
        let i = instruction.imm_jump();

        self.pc = (self.pc & 0xf0000000) | (i << 2);
    }

    /// Add Immediate Unsigned
    fn op_addiu(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let v = self.reg(s).wrapping_add(i);

        self.set_reg(t, v);
    }

    /// Bitwise Or Immediate
    fn op_ori(&mut self, instruction: Instruction) {
        let i = instruction.imm();
        let t = instruction.t();
        let s = instruction.s();

        let v = self.reg(s) | i;

        self.set_reg(t, v);
    }

    /// Load Upper Immediate
    fn op_lui(&mut self, instruction: Instruction) {
        let i = instruction.imm();
        let t = instruction.t();

        // Low 16bits are set to 0
        let v = i << 16;

        self.set_reg(t, v);
    }

    /// Store Word
    fn op_sw(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let addr = self.reg(s).wrapping_add(i);
        let v    = self.reg(t);

        self.store32(addr, v);
    }
}

#[derive(Clone,Copy)]
pub struct Instruction(u32);

impl Instruction {
    /// Return bits [31:26] of the instruction
    fn function(self) -> u32 {
        let Instruction(op) = self;

        op >> 26
    }

    /// Return bits [5:0] of the instruction
    fn subfunction(self) -> u32 {
        let Instruction(op) = self;

        op & 0x3f
    }

    /// Return register index in bits [25:21]
    fn s(self) -> RegisterIndex {
        let Instruction(op) = self;

        RegisterIndex((op >> 21) & 0x1f)
    }

    /// Return register index in bits [20:16]
    fn t(self) -> RegisterIndex {
        let Instruction(op) = self;

        RegisterIndex((op >> 16) & 0x1f)
    }

    /// Return register index in bits [15:11]
    fn d(self) -> RegisterIndex {
        let Instruction(op) = self;

        RegisterIndex((op >> 11) & 0x1f)
    }

    /// Return immediate value in bits [16:0]
    fn imm(self) -> u32 {
        let Instruction(op) = self;

        op & 0xffff
    }

    /// Return immediate value in bits [16:0] as a sign-extended 32bit
    /// value
    fn imm_se(self) -> u32 {
        let Instruction(op) = self;

        let v = (op & 0xffff) as i16;

        v as u32
    }

    /// Shift Immediate values are stored in bits [10:6]
    fn shift(self) -> u32 {
        let Instruction(op) = self;

        (op >> 6) & 0x1f
    }

    /// Jump target stored in bits [25:0]
    fn imm_jump(self) -> u32 {
        let Instruction(op) = self;

        op & 0x3ffffff
    }
}

struct RegisterIndex(u32);

impl Debug for Cpu {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {

        let instruction = self.next_instruction;

        try!(writeln!(f, "PC: {:08x}", self.pc));

        for i in 0..8 {
            let r1 = i * 4;
            let r2 = r1 + 1;
            let r3 = r2 + 1;
            let r4 = r3 + 1;

            try!(writeln!(f, "{}: {:08x}  {}: {:08x}  {}: {:08x}  {}: {:08x}",
                          REGISTER_MNEMONICS[r1], self.regs[r1],
                          REGISTER_MNEMONICS[r2], self.regs[r2],
                          REGISTER_MNEMONICS[r3], self.regs[r3],
                          REGISTER_MNEMONICS[r4], self.regs[r4]));
        }

        try!(writeln!(f, "Next instruction: 0x{:08x} {}",
                      instruction.0, asm::decode(instruction)));

        Ok(())
    }
}

const REGISTER_MNEMONICS: [&'static str; 32] = [
    "R00",
    "R01",
    "R02", "R03",
    "R04", "R05", "R06", "R07",
    "R08", "R09", "R10", "R11",
    "R12", "R13", "R14", "R15",
    "R16", "R17", "R18", "R19",
    "R20", "R21", "R22", "R23",
    "R24", "R25",
    "R26", "R27",
    "R28",
    "R29",
    "R30",
    "R31",
    ];

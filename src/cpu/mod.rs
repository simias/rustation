use std::fmt::{Debug, Display, Formatter, Error};

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
    /// 2nd set of registers used to emulate the load delay slot
    /// accurately. They contain the output of the current
    /// instruction.
    out_regs: [u32; 32],
    /// Memory interface
    inter: Interconnect,
    /// Cop0 register 12: Status Register
    sr: u32,
    /// Load initiated by the current instruction (will take effect
    /// after the load delay slot)
    load: (RegisterIndex, u32),
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
            out_regs: regs,
            inter: inter,
            // Start execution with a NOP while the real first
            // instruction is fetched.
            next_instruction: Instruction(0x0),
            sr: 0,
            load: (RegisterIndex(0), 0),
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

        // Execute the pending load (if any, otherwise it will load
        // `R0` which is a NOP). `set_reg` works only on `out_regs` so
        // this operation won't be visible by the next instruction.
        let (reg, val) = self.load;
        self.set_reg(reg, val);

        // We reset the load to target register 0 for the next
        // instruction
        self.load = (RegisterIndex(0), 0);

        self.decode_and_execute(instruction);

        // Copy the output registers as input for the next instruction
        self.regs = self.out_regs;
    }

    /// Load 32bit value from the memory
    fn load32(&self, addr: u32) -> u32 {
        self.inter.load32(addr)
    }

    /// Load 8bit value from the memory
    fn load8(&self, addr: u32) -> u8 {
        self.inter.load8(addr)
    }

    /// Store 32bit value into the memory
    fn store32(&mut self, addr: u32, val: u32) {
        self.inter.store32(addr, val);
    }

    /// Branch to immediate value `offset`.
    fn branch(&mut self, offset: u32) {
        // Offset immediates are always shifted two places to the
        // right since `PC` addresses have to be aligned on 32bits at
        // all times.
        let offset = offset << 2;

        let mut pc = self.pc;

        pc = pc.wrapping_add(offset);

        // We need to compensate for the hardcoded
        // `pc.wrapping_add(4)` in `run_next_instruction`
        pc = pc.wrapping_sub(4);

        self.pc = pc;
    }

    /// Store 16bit value into the memory
    fn store16(&mut self, addr: u32, val: u16) {
        self.inter.store16(addr, val);
    }

    /// Store 8bit value into the memory
    fn store8(&mut self, addr: u32, val: u8) {
        self.inter.store8(addr, val);
    }

    /// Decode `instruction`'s opcode and run the function
    fn decode_and_execute(&mut self, instruction: Instruction) {
        match instruction.function() {
            0b000000 => match instruction.subfunction() {
                0b000000 => self.op_sll(instruction),
                0b001000 => self.op_jr(instruction),
                0b001001 => self.op_jalr(instruction),
                0b100000 => self.op_add(instruction),
                0b100001 => self.op_addu(instruction),
                0b100100 => self.op_and(instruction),
                0b100101 => self.op_or(instruction),
                0b101011 => self.op_sltu(instruction),
                _        => panic!("Unhandled instruction {}", instruction),
            },
            0b000001 => self.op_bxx(instruction),
            0b000010 => self.op_j(instruction),
            0b000011 => self.op_jal(instruction),
            0b000100 => self.op_beq(instruction),
            0b000101 => self.op_bne(instruction),
            0b000110 => self.op_blez(instruction),
            0b000111 => self.op_bgtz(instruction),
            0b001000 => self.op_addi(instruction),
            0b001001 => self.op_addiu(instruction),
            0b001010 => self.op_slti(instruction),
            0b001100 => self.op_andi(instruction),
            0b001101 => self.op_ori(instruction),
            0b001111 => self.op_lui(instruction),
            0b010000 => self.op_cop0(instruction),
            0b100000 => self.op_lb(instruction),
            0b100011 => self.op_lw(instruction),
            0b100100 => self.op_lbu(instruction),
            0b101000 => self.op_sb(instruction),
            0b101001 => self.op_sh(instruction),
            0b101011 => self.op_sw(instruction),
            _        => panic!("Unhandled instruction {}", instruction),
        }
    }

    fn reg(&self, index: RegisterIndex) -> u32 {
        self.regs[index.0 as usize]
    }

    fn set_reg(&mut self, index: RegisterIndex, val: u32) {
        self.out_regs[index.0 as usize] = val;

        // Make sure R0 is always 0
        self.out_regs[0] = 0;
    }

    /// Shift Left Logical
    fn op_sll(&mut self, instruction: Instruction) {
        let i = instruction.shift();
        let t = instruction.t();
        let d = instruction.d();

        let v = self.reg(t) << i;

        self.set_reg(d, v);
    }

    /// Various branch instructions: BGEZ, BLTZ, BGEZAL, BLTZAL. Bits
    /// 16 and 20 are used to figure out which one to use
    fn op_bxx(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let s = instruction.s();

        let instruction = instruction.0;

        let is_bgez = (instruction >> 16) & 1;
        let is_link = (instruction >> 20) & 1 != 0;

        let v = self.reg(s) as i32;

        // Test "less than zero"
        let test = (v < 0) as u32;

        // If the test is "greater than or equal to zero" we need to
        // negate the comparison above ("a >= 0" <=> "!(a < 0)"). The
        // xor takes care of that.
        let test = test ^ is_bgez;

        if test != 0 {
            if is_link {
                let ra = self.pc;

                // Store return address in R31
                self.set_reg(RegisterIndex(31), ra);
            }

            self.branch(i);
        }
    }

    /// Jump Register
    fn op_jr(&mut self, instruction: Instruction) {
        let s = instruction.s();

        self.pc = self.reg(s);
    }

    /// Jump And Link Register
    fn op_jalr(&mut self, instruction: Instruction) {
        let d = instruction.d();
        let s = instruction.s();

        let ra = self.pc;

        // Store return address in `d`
        self.set_reg(d, ra);

        self.pc = self.reg(s);
    }

    /// Add and generate an exception on overflow
    fn op_add(&mut self, instruction: Instruction) {
        let s = instruction.s();
        let t = instruction.t();
        let d = instruction.d();

        let s = self.reg(s) as i32;
        let t = self.reg(t) as i32;

        let v = match s.checked_add(t) {
            Some(v) => v as u32,
            None    => panic!("ADD overflow"),
        };

        self.set_reg(d, v);
    }

    /// Add Unsigned
    fn op_addu(&mut self, instruction: Instruction) {
        let s = instruction.s();
        let t = instruction.t();
        let d = instruction.d();

        let v = self.reg(s).wrapping_add(self.reg(t));

        self.set_reg(d, v);
    }

    /// Bitwise And
    fn op_and(&mut self, instruction: Instruction) {
        let d = instruction.d();
        let s = instruction.s();
        let t = instruction.t();

        let v = self.reg(s) & self.reg(t);

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

    /// Set on Less Than Unsigned
    fn op_sltu(&mut self, instruction: Instruction) {
        let d = instruction.d();
        let s = instruction.s();
        let t = instruction.t();

        let v = self.reg(s) < self.reg(t);

        self.set_reg(d, v as u32);
    }

    /// Jump
    fn op_j(&mut self, instruction: Instruction) {
        let i = instruction.imm_jump();

        self.pc = (self.pc & 0xf0000000) | (i << 2);
    }

    /// Jump And Link
    fn op_jal(&mut self, instruction: Instruction) {
        let ra = self.pc;

        // Store return address in R31
        self.set_reg(RegisterIndex(31), ra);

        self.op_j(instruction);
    }

    /// Branch if Equal
    fn op_beq(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let s = instruction.s();
        let t = instruction.t();

        if self.reg(s) == self.reg(t) {
            self.branch(i);
        }
    }

    /// Branch if Not Equal
    fn op_bne(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let s = instruction.s();
        let t = instruction.t();

        if self.reg(s) != self.reg(t) {
            self.branch(i);
        }
    }

    /// Branch if Less than or Equal to Zero
    fn op_blez(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let s = instruction.s();

        let v = self.reg(s) as i32;

        if v <= 0 {
            self.branch(i);
        }
    }

    /// Branch if Greater Than Zero
    fn op_bgtz(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let s = instruction.s();

        let v = self.reg(s) as i32;

        if v > 0 {
            self.branch(i);
        }
    }

    /// Add Immediate Unsigned and check for overflow
    fn op_addi(&mut self, instruction: Instruction) {
        let i = instruction.imm_se() as i32;
        let t = instruction.t();
        let s = instruction.s();

        let s = self.reg(s) as i32;

        let v = match s.checked_add(i) {
            Some(v) => v as u32,
            None    => panic!("ADDI overflow"),
        };

        self.set_reg(t, v);
    }

    /// Add Immediate Unsigned
    fn op_addiu(&mut self, instruction: Instruction) {
        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let v = self.reg(s).wrapping_add(i);

        self.set_reg(t, v);
    }

    /// Set if Less Than Immediate (signed)
    fn op_slti(&mut self, instruction: Instruction) {
        let i = instruction.imm_se() as i32;
        let s = instruction.s();
        let t = instruction.t();

        let v = (self.reg(s) as i32) < i;

        self.set_reg(t, v as u32);
    }

    /// Bitwise And Immediate
    fn op_andi(&mut self, instruction: Instruction) {
        let i = instruction.imm();
        let t = instruction.t();
        let s = instruction.s();

        let v = self.reg(s) & i;

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

    /// Coprocessor 0 opcode
    fn op_cop0(&mut self, instruction: Instruction) {
        match instruction.cop_opcode() {
            0b00000 => self.op_mfc0(instruction),
            0b00100 => self.op_mtc0(instruction),
            _       => panic!("unhandled cop0 instruction {}", instruction)
        }
    }

    /// Move From Coprocessor 0
    fn op_mfc0(&mut self, instruction: Instruction) {
        let cpu_r = instruction.t();
        let cop_r = instruction.d().0;

        let v = match cop_r {
            12 => self.sr,
            13 => // Cause register
                panic!("Unhandled read from CAUSE register"),
            _  =>
                panic!("Unhandled read from cop0r{}", cop_r),
        };

        self.load = (cpu_r, v)
    }

    /// Move To Coprocessor 0
    fn op_mtc0(&mut self, instruction: Instruction) {
        let cpu_r = instruction.t();
        let cop_r = instruction.d().0;

        let v = self.reg(cpu_r);

        match cop_r {
            3 | 5 | 6 | 7 | 9 | 11  => // Breakpoints registers
                if v != 0 {

                    panic!("Unhandled write to cop0r{}: {:08x}", cop_r, v)
                },
            12 => self.sr = v,
            13 => // Cause register
                if v != 0 {
                    panic!("Unhandled write to CAUSE register: {:08x}", v)
                },
            _  => panic!("Unhandled cop0 register {}", cop_r),
        }
    }

    /// Load Byte (signed)
    fn op_lb(&mut self, instruction: Instruction) {

        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let addr = self.reg(s).wrapping_add(i);

        // Cast as i8 to force sign extension
        let v = self.load8(addr) as i8;

        // Put the load in the delay slot
        self.load = (t, v as u32);
    }

    /// Load Word
    fn op_lw(&mut self, instruction: Instruction) {

        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let addr = self.reg(s).wrapping_add(i);

        let v = self.load32(addr);

        // Put the load in the delay slot
        self.load = (t, v);
    }

    /// Load Byte Unsigned
    fn op_lbu(&mut self, instruction: Instruction) {

        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let addr = self.reg(s).wrapping_add(i);

        let v = self.load8(addr);

        // Put the load in the delay slot
        self.load = (t, v as u32);
    }

    /// Store Byte
    fn op_sb(&mut self, instruction: Instruction) {

        if self.sr & 0x10000 != 0 {
            // Cache is isolated, ignore write
            println!("Ignoring store while cache is isolated");
            return;
        }

        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let addr = self.reg(s).wrapping_add(i);
        let v    = self.reg(t);

        self.store8(addr, v as u8);
    }

    /// Store Halfword
    fn op_sh(&mut self, instruction: Instruction) {

        if self.sr & 0x10000 != 0 {
            // Cache is isolated, ignore write
            println!("Ignoring store while cache is isolated");
            return;
        }

        let i = instruction.imm_se();
        let t = instruction.t();
        let s = instruction.s();

        let addr = self.reg(s).wrapping_add(i);
        let v    = self.reg(t);

        self.store16(addr, v as u16);
    }

    /// Store Word
    fn op_sw(&mut self, instruction: Instruction) {

        if self.sr & 0x10000 != 0 {
            // Cache is isolated, ignore write
            println!("Ignoring store while cache is isolated");
            return;
        }

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

    /// Return coprocessor opcode in bits [25:21]
    fn cop_opcode(self) -> u32 {
        let Instruction(op) = self;

        (op >> 21) & 0x1f
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

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        try!(write!(f, "{:08x}", self.0));

        Ok(())
    }
}

#[derive(Clone,Copy)]
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

        let (RegisterIndex(reg), val) = self.load;

        if reg != 0 {
            try!(writeln!(f, "Pending load: {} <- {:08x}",
                          REGISTER_MNEMONICS[reg as usize], val));
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

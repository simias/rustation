use super::{Instruction, RegisterIndex};

pub fn decode(instruction: Instruction) -> String {
    match instruction.function() {
        0b000000 => match instruction.subfunction() {
            0b000000 => op_sll(instruction),
            0b000010 => op_srl(instruction),
            0b000011 => op_sra(instruction),
            0b001000 => op_jr(instruction),
            0b001001 => op_jalr(instruction),
            0b010010 => op_mflo(instruction),
            0b011010 => op_div(instruction),
            0b011011 => op_divu(instruction),
            0b100000 => op_add(instruction),
            0b100001 => op_addu(instruction),
            0b100011 => op_subu(instruction),
            0b100100 => op_and(instruction),
            0b100101 => op_or(instruction),
            0b101011 => op_sltu(instruction),
            _        => format!("!UNKNOWN!"),
        },
        0b000001 => op_bxx(instruction),
        0b000010 => op_j(instruction),
        0b000011 => op_jal(instruction),
        0b000100 => op_beq(instruction),
        0b000101 => op_bne(instruction),
        0b000110 => op_blez(instruction),
        0b000111 => op_bgtz(instruction),
        0b001000 => op_addi(instruction),
        0b001001 => op_addiu(instruction),
        0b001010 => op_slti(instruction),
        0b001011 => op_sltiu(instruction),
        0b001100 => op_andi(instruction),
        0b001101 => op_ori(instruction),
        0b001111 => op_lui(instruction),
        0b010000 => op_cop0(instruction),
        0b100000 => op_lb(instruction),
        0b100011 => op_lw(instruction),
        0b100100 => op_lbu(instruction),
        0b101000 => op_sb(instruction),
        0b101001 => op_sh(instruction),
        0b101011 => op_sw(instruction),
        _        => format!("!UNKNOWN!"),
    }
}

fn reg(idx: RegisterIndex) -> &'static str {
    super::REGISTER_MNEMONICS[idx.0 as usize]
}

fn op_sll(instruction: Instruction) -> String {
    let i = instruction.shift();
    let t = instruction.t();
    let d = instruction.d();

    format!("sll {}, {} << {}", reg(d), reg(t), i)
}

fn op_srl(instruction: Instruction) -> String {
    let i = instruction.shift();
    let t = instruction.t();
    let d = instruction.d();

    format!("srl {}, {} >> {}", reg(d), reg(t), i)
}

fn op_sra(instruction: Instruction) -> String {
    let i = instruction.shift();
    let t = instruction.t();
    let d = instruction.d();

    format!("sra {}, {} >> {}", reg(d), reg(t), i)
}

fn op_jr(instruction: Instruction) -> String {
    let s = instruction.s();

    format!("jr {}", reg(s))
}

fn op_jalr(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();

    format!("jalr {}, {}", reg(d), reg(s))
}

fn op_mflo(instruction: Instruction) -> String {
    let d = instruction.d();

    format!("mflo {}", reg(d))
}

fn op_div(instruction: Instruction) -> String {
    let s = instruction.s();
    let t = instruction.t();

    format!("div {}, {}", reg(s), reg(t))
}

fn op_divu(instruction: Instruction) -> String {
    let s = instruction.s();
    let t = instruction.t();

    format!("divu {}, {}", reg(s), reg(t))
}

fn op_add(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();
    let t = instruction.t();

    format!("add {}, {}, {}", reg(d), reg(s), reg(t))
}

fn op_addu(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();
    let t = instruction.t();

    format!("addu {}, {}, {}", reg(d), reg(s), reg(t))
}

fn op_subu(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();
    let t = instruction.t();

    format!("subu {}, {}, {}", reg(d), reg(s), reg(t))
}

fn op_and(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();
    let t = instruction.t();

    format!("and {}, {}, {}", reg(d), reg(s), reg(t))
}

fn op_or(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();
    let t = instruction.t();

    format!("or {}, {}, {}", reg(d), reg(s), reg(t))
}

fn op_sltu(instruction: Instruction) -> String {
    let d = instruction.d();
    let s = instruction.s();
    let t = instruction.t();

    format!("sltu {}, {}, {}", reg(d), reg(s), reg(t))
}

fn op_bxx(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();

    let op = match instruction.0 & (1 << 16) != 0 {
        true  => "bgez",
        false => "bltz",
    };

    let al = match instruction.0 & (1 << 20) != 0 {
        true  => "al",
        false => "",
    };

    format!("{}{} {}, {}", op, al, reg(s), (i << 2) as i32)
}

fn op_j(instruction: Instruction) -> String {
    let i = instruction.imm_jump();

    format!("j (PC & 0xf0000000) | {:x}", i << 2)
}

fn op_jal(instruction: Instruction) -> String {
    let i = instruction.imm_jump();

    format!("jal (PC & 0xf0000000) | {:x}", i << 2)
}

fn op_beq(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();
    let t = instruction.t();

    format!("beq {}, {}, {}", reg(s), reg(t), (i << 2) as i32)
}

fn op_bne(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();
    let t = instruction.t();

    format!("bne {}, {}, {}", reg(s), reg(t), (i << 2) as i32)
}

fn op_blez(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();

    format!("blez {}, {}", reg(s), (i << 2) as i32)
}

fn op_bgtz(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();

    format!("bgtz {}, {}", reg(s), (i << 2) as i32)
}

fn op_addi(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("addi {}, {}, 0x{:x}", reg(t), reg(s), i)
}

fn op_addiu(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("addiu {}, {}, 0x{:x}", reg(t), reg(s), i)
}

fn op_slti(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();
    let t = instruction.t();

    format!("slti {}, {}, {}", reg(t), reg(s), i as i32)
}

fn op_sltiu(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let s = instruction.s();
    let t = instruction.t();

    format!("sltiu {}, {}, 0x{:x}", reg(t), reg(s), i)
}

fn op_andi(instruction: Instruction) -> String {
    let i = instruction.imm();
    let t = instruction.t();
    let s = instruction.s();

    format!("andi {}, {}, 0x{:x}", reg(t), reg(s), i)
}

fn op_ori(instruction: Instruction) -> String {
    let i = instruction.imm();
    let t = instruction.t();
    let s = instruction.s();

    format!("ori {}, {}, 0x{:x}", reg(t), reg(s), i)
}

fn op_lui(instruction: Instruction) -> String {
    let i = instruction.imm();
    let t = instruction.t();

    format!("lui {}, 0x{:x}", reg(t), i)
}

fn op_cop0(instruction: Instruction) -> String {
    match instruction.cop_opcode() {
        0b00000 => op_mfc0(instruction),
        0b00100 => op_mtc0(instruction),
        _       => format!("!UNKNOWN cop0 instruction {}!", instruction)
    }
}

fn op_mfc0(instruction: Instruction) -> String{
    let cpu_r = instruction.t();
    let cop_r = instruction.d().0;

    format!("mfc0 {}, cop0r{}", reg(cpu_r), cop_r)
}

fn op_mtc0(instruction: Instruction) -> String{
    let cpu_r = instruction.t();
    let cop_r = instruction.d().0;

    format!("mtc0 {}, cop0r{}", reg(cpu_r), cop_r)
}

fn op_lb(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("lb {}, [{} + 0x{:x}]", reg(t), reg(s), i)
}

fn op_lw(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("lw {}, [{} + 0x{:x}]", reg(t), reg(s), i)
}

fn op_lbu(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("lbu {}, [{} + 0x{:x}]", reg(t), reg(s), i)
}

fn op_sb(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("sb {}, [{} + 0x{:x}]", reg(t), reg(s), i)
}

fn op_sh(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("sh {}, [{} + 0x{:x}]", reg(t), reg(s), i)
}

fn op_sw(instruction: Instruction) -> String {
    let i = instruction.imm_se();
    let t = instruction.t();
    let s = instruction.s();

    format!("sw {}, [{} + 0x{:x}]", reg(t), reg(s), i)
}

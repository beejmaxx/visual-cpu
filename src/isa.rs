use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Lbu,
    Lhu,
    Sb,
    Sh,
    Sw,
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
    Slli,
    Srli,
    Srai,
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
    Fence,
    Ecall,
    Ebreak,
}

pub const REG_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

#[derive(Debug, Clone, Serialize)]
pub struct Instruction {
    pub pc: u32,
    pub raw: u32,
    #[serde(skip)]
    pub op: Op,
    pub rd: usize,
    pub rs1: usize,
    pub rs2: usize,
    pub imm: i32,
    pub text: String,
}

fn sext(value: u32, bits: u32) -> i32 {
    ((value << (32 - bits)) as i32) >> (32 - bits)
}

pub fn decode(pc: u32, raw: u32) -> Result<Instruction, String> {
    use Op::*;
    let opcode = raw & 0x7f;
    let rd = ((raw >> 7) & 31) as usize;
    let rs1 = ((raw >> 15) & 31) as usize;
    let rs2 = ((raw >> 20) & 31) as usize;
    let f3 = (raw >> 12) & 7;
    let f7 = raw >> 25;
    let invalid = || format!("Unsupported instruction 0x{raw:08x} at 0x{pc:08x}");
    let mut imm = sext(raw >> 20, 12);
    let op = match opcode {
        0x37 => {
            imm = (raw & 0xfffff000) as i32;
            Lui
        }
        0x17 => {
            imm = (raw & 0xfffff000) as i32;
            Auipc
        }
        0x6f => {
            imm = sext(
                ((raw >> 31) << 20)
                    | (((raw >> 12) & 255) << 12)
                    | (((raw >> 20) & 1) << 11)
                    | (((raw >> 21) & 1023) << 1),
                21,
            );
            Jal
        }
        0x67 if f3 == 0 => Jalr,
        0x63 => {
            imm = sext(
                ((raw >> 31) << 12)
                    | (((raw >> 7) & 1) << 11)
                    | (((raw >> 25) & 63) << 5)
                    | (((raw >> 8) & 15) << 1),
                13,
            );
            match f3 {
                0 => Beq,
                1 => Bne,
                4 => Blt,
                5 => Bge,
                6 => Bltu,
                7 => Bgeu,
                _ => return Err(invalid()),
            }
        }
        0x03 => match f3 {
            0 => Lb,
            1 => Lh,
            2 => Lw,
            4 => Lbu,
            5 => Lhu,
            _ => return Err(invalid()),
        },
        0x23 => {
            imm = sext(((raw >> 25) << 5) | ((raw >> 7) & 31), 12);
            match f3 {
                0 => Sb,
                1 => Sh,
                2 => Sw,
                _ => return Err(invalid()),
            }
        }
        0x13 => match f3 {
            0 => Addi,
            2 => Slti,
            3 => Sltiu,
            4 => Xori,
            6 => Ori,
            7 => Andi,
            1 if f7 == 0 => {
                imm = rs2 as i32;
                Slli
            }
            5 if f7 == 0 => {
                imm = rs2 as i32;
                Srli
            }
            5 if f7 == 0x20 => {
                imm = rs2 as i32;
                Srai
            }
            _ => return Err(invalid()),
        },
        0x33 => match (f7, f3) {
            (0, 0) => Add,
            (0x20, 0) => Sub,
            (0, 1) => Sll,
            (0, 2) => Slt,
            (0, 3) => Sltu,
            (0, 4) => Xor,
            (0, 5) => Srl,
            (0x20, 5) => Sra,
            (0, 6) => Or,
            (0, 7) => And,
            (1, 0) => Mul,
            (1, 1) => Mulh,
            (1, 2) => Mulhsu,
            (1, 3) => Mulhu,
            (1, 4) => Div,
            (1, 5) => Divu,
            (1, 6) => Rem,
            (1, 7) => Remu,
            _ => return Err(invalid()),
        },
        0x0f if f3 == 0 => Fence,
        0x73 if raw == 0x00000073 => Ecall,
        0x73 if raw == 0x00100073 => Ebreak,
        _ => return Err(invalid()),
    };
    let name = format!("{op:?}").to_lowercase();
    let (d, a, b) = (REG_NAMES[rd], REG_NAMES[rs1], REG_NAMES[rs2]);
    let text = match op {
        Lui | Auipc => format!("{name} {d}, 0x{:x}", (imm as u32) >> 12),
        Jal => format!("jal {d}, 0x{:08x}", pc.wrapping_add(imm as u32)),
        Jalr => format!("jalr {d}, {imm}({a})"),
        Beq | Bne | Blt | Bge | Bltu | Bgeu => {
            format!("{name} {a}, {b}, 0x{:08x}", pc.wrapping_add(imm as u32))
        }
        Lb | Lh | Lw | Lbu | Lhu => format!("{name} {d}, {imm}({a})"),
        Sb | Sh | Sw => format!("{name} {b}, {imm}({a})"),
        Addi | Slti | Sltiu | Xori | Ori | Andi | Slli | Srli | Srai => {
            format!("{name} {d}, {a}, {imm}")
        }
        Fence | Ecall | Ebreak => name,
        _ => format!("{name} {d}, {a}, {b}"),
    };
    Ok(Instruction {
        pc,
        raw,
        op,
        rd,
        rs1,
        rs2,
        imm,
        text,
    })
}

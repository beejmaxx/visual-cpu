use crate::{
    alu::{self, Function},
    isa::{Instruction, Op, REG_NAMES},
};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct ReadPort {
    pub register: usize,
    pub value: u32,
}

#[derive(Clone, Serialize)]
pub struct Datapath {
    pub pc: u32,
    pub raw: u32,
    pub instruction: String,
    pub operation: String,
    pub read_a: Option<ReadPort>,
    pub read_b: Option<ReadPort>,
    pub source_a: Option<u64>,
    pub source_b: Option<u64>,
    pub left_source: String,
    pub right_source: String,
    pub left: u32,
    pub right: u32,
    pub immediate: i32,
    pub rd: Option<usize>,
    pub before: u32,
    pub write_enable: bool,
    pub write_source: String,
    pub write_value: Option<u32>,
    pub committed: bool,
    pub memory_address: Option<u32>,
    pub store_data: Option<u32>,
    pub branch_taken: Option<bool>,
    pub next_pc: Option<u32>,
    pub alu: Option<alu::Trace>,
    pub function: Option<Function>,
}

impl Datapath {
    pub fn decode(i: &Instruction, regs: &[u32; 32]) -> Self {
        use Op::*;
        let uses_a = !matches!(i.op, Lui | Auipc | Jal | Fence | Ecall | Ebreak);
        let uses_b = matches!(
            i.op,
            Beq | Bne
                | Blt
                | Bge
                | Bltu
                | Bgeu
                | Sb
                | Sh
                | Sw
                | Add
                | Sub
                | Sll
                | Slt
                | Sltu
                | Xor
                | Srl
                | Sra
                | Or
                | And
                | Mul
                | Mulh
                | Mulhsu
                | Mulhu
                | Div
                | Divu
                | Rem
                | Remu
        );
        let uses_imm = matches!(
            i.op,
            Lui | Auipc
                | Jal
                | Jalr
                | Lb
                | Lh
                | Lw
                | Lbu
                | Lhu
                | Sb
                | Sh
                | Sw
                | Addi
                | Slti
                | Sltiu
                | Xori
                | Ori
                | Andi
                | Slli
                | Srli
                | Srai
        );
        let store = matches!(i.op, Sb | Sh | Sw);
        let load = matches!(i.op, Lb | Lh | Lw | Lbu | Lhu);
        let no_write = store
            || matches!(
                i.op,
                Beq | Bne | Blt | Bge | Bltu | Bgeu | Fence | Ecall | Ebreak
            );
        let read_a = uses_a.then(|| ReadPort {
            register: i.rs1,
            value: regs[i.rs1],
        });
        let read_b = uses_b.then(|| ReadPort {
            register: i.rs2,
            value: regs[i.rs2],
        });
        let (left_source, left) = if matches!(i.op, Auipc | Jal) {
            ("PC".into(), i.pc)
        } else if let Some(port) = &read_a {
            (REG_NAMES[port.register].into(), port.value)
        } else {
            ("zero".into(), 0)
        };
        let (right_source, right) = if uses_imm {
            ("immediate".into(), i.imm as u32)
        } else if let Some(port) = &read_b {
            (REG_NAMES[port.register].into(), port.value)
        } else {
            ("zero".into(), 0)
        };
        let function = match i.op {
            Lui | Auipc | Jal | Jalr | Lb | Lh | Lw | Lbu | Lhu | Sb | Sh | Sw | Add | Addi => {
                Some(Function::Add)
            }
            Sub => Some(Function::Sub),
            And | Andi => Some(Function::And),
            Or | Ori => Some(Function::Or),
            Xor | Xori => Some(Function::Xor),
            Sll | Slli => Some(Function::Sll),
            Srl | Srli => Some(Function::Srl),
            Sra | Srai => Some(Function::Sra),
            Slt | Slti | Blt => Some(Function::Slt),
            Sltu | Sltiu | Bltu => Some(Function::Sltu),
            Beq => Some(Function::Eq),
            Bne => Some(Function::Ne),
            Bge => Some(Function::Ge),
            Bgeu => Some(Function::Geu),
            Mul => Some(Function::Mul),
            Mulh => Some(Function::Mulh),
            Mulhsu => Some(Function::Mulhsu),
            Mulhu => Some(Function::Mulhu),
            Div => Some(Function::Div),
            Divu => Some(Function::Divu),
            Rem => Some(Function::Rem),
            Remu => Some(Function::Remu),
            Fence | Ecall | Ebreak => None,
        };
        Self {
            pc: i.pc,
            raw: i.raw,
            instruction: i.text.clone(),
            operation: format!("{:?}", i.op).to_lowercase(),
            read_a,
            read_b,
            source_a: None,
            source_b: None,
            left_source,
            right_source,
            left,
            right,
            immediate: i.imm,
            rd: (!no_write).then_some(i.rd),
            before: if no_write { 0 } else { regs[i.rd] },
            write_enable: !no_write && i.rd != 0,
            write_source: if no_write {
                "none"
            } else if load {
                "memory"
            } else if matches!(i.op, Jal | Jalr) {
                "PC + 4"
            } else {
                "ALU"
            }
            .into(),
            write_value: None,
            committed: false,
            memory_address: None,
            store_data: store.then_some(regs[i.rs2]),
            branch_taken: None,
            next_pc: None,
            alu: None,
            function,
        }
    }
}

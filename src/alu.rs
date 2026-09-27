//! Combinational integer ALU plus a clocked iterative multiply/divide unit.
//! Carry propagation is a signal, while multiply/divide steps occupy cycles.
//! The CPU consumes the result computed by these circuits.
use crate::muldiv::MulDiv;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Function {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Sll,
    Srl,
    Sra,
    Slt,
    Sltu,
    Eq,
    Ne,
    Ge,
    Geu,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
}

#[derive(Clone, Serialize)]
pub struct BitSlice {
    pub bit: u32,
    pub a: u32,
    pub b: u32,
    pub effective_b: u32,
    pub carry_in: u32,
    pub propagate: u32,
    pub generate: u32,
    pub sum: u32,
    pub carry_out: u32,
}

#[derive(Clone, Serialize)]
pub struct Adder {
    pub subtract: bool,
    pub result: u32,
    pub carry_out: u32,
    pub overflow: bool,
    pub bits: Vec<BitSlice>,
}

pub fn add(a: u32, b: u32, subtract: bool) -> Adder {
    let mut carry = u32::from(subtract);
    let mut result = 0;
    let mut bits = Vec::with_capacity(32);
    for bit in 0..32 {
        let ai = (a >> bit) & 1;
        let bi = (b >> bit) & 1;
        let effective_b = bi ^ u32::from(subtract);
        let propagate = ai ^ effective_b;
        let generate = ai & effective_b;
        let sum = propagate ^ carry;
        let carry_out = generate | (propagate & carry);
        bits.push(BitSlice {
            bit,
            a: ai,
            b: bi,
            effective_b,
            carry_in: carry,
            propagate,
            generate,
            sum,
            carry_out,
        });
        result |= sum << bit;
        carry = carry_out;
    }
    let overflow = bits[31].carry_in != carry;
    Adder {
        subtract,
        result,
        carry_out: carry,
        overflow,
        bits,
    }
}

#[derive(Clone, Serialize)]
pub struct ShiftStage {
    pub distance: u32,
    pub enabled: bool,
    pub input: u32,
    pub output: u32,
}

#[derive(Clone, Serialize)]
pub struct Shifter {
    pub direction: String,
    pub arithmetic: bool,
    pub amount: u32,
    pub stages: Vec<ShiftStage>,
    pub result: u32,
}

#[derive(Clone, Serialize)]
pub struct Compare {
    pub equal: bool,
    pub signed_less: bool,
    pub unsigned_less: bool,
    pub a_negative: bool,
    pub b_negative: bool,
}

#[derive(Clone, Serialize)]
pub struct Trace {
    pub function: Function,
    pub unit: String,
    pub left: u32,
    pub right: u32,
    pub result: u32,
    pub ready: bool,
    pub adder: Adder,
    pub and: u32,
    pub or: u32,
    pub xor: u32,
    pub shifter: Shifter,
    pub compare: Compare,
    pub muldiv: Option<MulDiv>,
}

pub fn start(function: Function, a: u32, b: u32) -> Trace {
    use Function::*;
    let subtract = matches!(function, Sub | Slt | Sltu | Eq | Ne | Ge | Geu);
    let adder = add(a, b, subtract);
    let comparison_sub = if subtract {
        adder.result
    } else {
        a.wrapping_sub(b)
    };
    let unsigned_less = if subtract {
        adder.carry_out == 0
    } else {
        a < b
    };
    let compare = Compare {
        equal: a ^ b == 0,
        signed_less: if (a ^ b) >> 31 != 0 {
            a >> 31 != 0
        } else {
            comparison_sub >> 31 != 0
        },
        unsigned_less,
        a_negative: a >> 31 != 0,
        b_negative: b >> 31 != 0,
    };
    let right_shift = matches!(function, Srl | Sra);
    let arithmetic = function == Sra;
    let amount = b & 31;
    let mut shifted = a;
    let mut stages = Vec::with_capacity(5);
    for distance in [1, 2, 4, 8, 16] {
        let input = shifted;
        let enabled = amount & distance != 0;
        if enabled {
            shifted = if arithmetic {
                ((shifted as i32) >> distance) as u32
            } else if right_shift {
                shifted >> distance
            } else {
                shifted << distance
            };
        }
        stages.push(ShiftStage {
            distance,
            enabled,
            input,
            output: shifted,
        });
    }
    let shifter = Shifter {
        direction: if right_shift { "right" } else { "left" }.into(),
        arithmetic,
        amount,
        stages,
        result: shifted,
    };
    let muldiv = MulDiv::supports(function).then(|| MulDiv::new(function, a, b));
    let result = match function {
        Add | Sub => adder.result,
        And => a & b,
        Or => a | b,
        Xor => a ^ b,
        Sll | Srl | Sra => shifted,
        Slt => u32::from(compare.signed_less),
        Sltu => u32::from(compare.unsigned_less),
        Eq => u32::from(compare.equal),
        Ne => u32::from(!compare.equal),
        Ge => u32::from(!compare.signed_less),
        Geu => u32::from(!compare.unsigned_less),
        Mul | Mulh | Mulhsu | Mulhu | Div | Divu | Rem | Remu => 0,
    };
    let unit = match function {
        Add | Sub => "adder",
        And | Or | Xor => "logic",
        Sll | Srl | Sra => "shifter",
        Slt | Sltu | Eq | Ne | Ge | Geu => "compare",
        _ => "muldiv",
    }
    .into();
    Trace {
        function,
        unit,
        left: a,
        right: b,
        result,
        ready: muldiv.is_none(),
        adder,
        and: a & b,
        or: a | b,
        xor: a ^ b,
        shifter,
        compare,
        muldiv,
    }
}

impl Trace {
    pub fn advance(&mut self) {
        if let Some(unit) = &mut self.muldiv {
            unit.advance();
            self.ready = unit.ready;
            if let Some(result) = unit.result {
                self.result = result;
            }
        }
    }
}

/// Functional convenience entry point; the CPU uses start/advance so every
/// multiply/divide iteration occupies a separate simulated execute cycle.
pub fn evaluate(function: Function, a: u32, b: u32) -> Trace {
    let mut trace = start(function, a, b);
    while !trace.ready {
        trace.advance();
    }
    trace
}

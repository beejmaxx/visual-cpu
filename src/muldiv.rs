//! Iterative integer multiply/divide. One call advances one execute cycle.
//! Arithmetic results come from full-adder gates, including sign conversion.
use crate::alu::{BitSlice, Function};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Circuit {
    pub name: String,
    pub width: u32,
    pub subtract: bool,
    // Hex strings preserve all 64 bits when the snapshot reaches JavaScript.
    pub left: String,
    pub right: String,
    pub output: String,
    pub carry_out: u32,
    pub bits: Vec<BitSlice>,
}

pub fn ripple(name: &str, a: u64, b: u64, subtract: bool, width: u32) -> (u64, Circuit) {
    assert!((1..=64).contains(&width));
    let mut carry = u32::from(subtract);
    let mut output = 0u64;
    let mut bits = Vec::with_capacity(width as usize);
    for bit in 0..width {
        let ai = ((a >> bit) & 1) as u32;
        let bi = ((b >> bit) & 1) as u32;
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
        output |= u64::from(sum) << bit;
        carry = carry_out;
    }
    (
        output,
        Circuit {
            name: name.into(),
            width,
            subtract,
            left: word(a, width),
            right: word(b, width),
            output: word(output, width),
            carry_out: carry,
            bits,
        },
    )
}

fn word(value: u64, width: u32) -> String {
    let mask = u64::MAX >> (64 - width);
    format!(
        "{:0digits$x}",
        value & mask,
        digits = width.div_ceil(4) as usize
    )
}

#[derive(Clone, Serialize)]
pub struct Register {
    pub name: String,
    pub width: u32,
    pub before: String,
    pub after: String,
}

fn register(name: &str, width: u32, before: u64, after: u64) -> Register {
    Register {
        name: name.into(),
        width,
        before: word(before, width),
        after: word(after, width),
    }
}

#[derive(Clone, Serialize)]
pub struct Control {
    pub kind: String,
    pub label: String,
    pub select: u32,
    pub width: u32,
    pub input_one: String,
    pub input_zero: String,
    pub output: String,
}

#[derive(Clone, Default)]
struct Working {
    product: u64,
    multiplicand: u64,
    quotient: u32,
    remainder: u64,
    divisor: u32,
}

#[derive(Clone, Serialize)]
pub struct MulDiv {
    pub operation: Function,
    pub kind: String,
    pub stage: String,
    pub execute_cycle: u32,
    pub iteration: u32,
    pub ready: bool,
    pub result: Option<u32>,
    pub high: Option<u32>,
    pub low: Option<u32>,
    pub quotient: Option<u32>,
    pub remainder: Option<u32>,
    pub signed_a: bool,
    pub signed_b: bool,
    pub negative_a: bool,
    pub negative_b: bool,
    pub negate_result: bool,
    pub negate_remainder: bool,
    pub divide_by_zero: bool,
    pub signed_overflow: bool,
    pub description: String,
    pub registers: Vec<Register>,
    pub circuits: Vec<Circuit>,
    pub control: Option<Control>,
    #[serde(skip)]
    working: Working,
}

impl MulDiv {
    pub fn supports(function: Function) -> bool {
        use Function::*;
        matches!(
            function,
            Mul | Mulh | Mulhsu | Mulhu | Div | Divu | Rem | Remu
        )
    }

    /// Cycle 1: form operand magnitudes through conditional two's complement.
    pub fn new(operation: Function, a: u32, b: u32) -> Self {
        use Function::*;
        assert!(Self::supports(operation));
        let multiply = matches!(operation, Mul | Mulh | Mulhsu | Mulhu);
        let signed_a = matches!(operation, Mul | Mulh | Mulhsu | Div | Rem);
        let signed_b = matches!(operation, Mul | Mulh | Div | Rem);
        let negative_a = signed_a && a >> 31 != 0;
        let negative_b = signed_b && b >> 31 != 0;
        let (a_magnitude, ca) = ripple("A magnitude", 0, u64::from(a), negative_a, 32);
        let (b_magnitude, cb) = ripple("B magnitude", 0, u64::from(b), negative_b, 32);
        let working = if multiply {
            Working {
                multiplicand: a_magnitude,
                quotient: b_magnitude as u32,
                ..Working::default()
            }
        } else {
            Working {
                quotient: a_magnitude as u32,
                divisor: b_magnitude as u32,
                ..Working::default()
            }
        };
        let registers = if multiply {
            vec![
                register("P · product", 64, 0, 0),
                register("M · multiplicand", 64, 0, a_magnitude),
                register("Q · multiplier", 32, 0, b_magnitude),
            ]
        } else {
            vec![
                register("R · remainder", 33, 0, 0),
                register("D · divisor", 33, 0, b_magnitude),
                register("Q · dividend / quotient", 32, 0, a_magnitude),
            ]
        };
        Self {
            operation,
            kind: if multiply { "multiply" } else { "divide" }.into(),
            stage: "prepare".into(),
            execute_cycle: 1,
            iteration: 0,
            ready: false,
            result: None,
            high: None,
            low: None,
            quotient: None,
            remainder: None,
            signed_a,
            signed_b,
            negative_a,
            negative_b,
            negate_result: (negative_a ^ negative_b) && (multiply || b != 0),
            negate_remainder: !multiply && negative_a,
            divide_by_zero: !multiply && b == 0,
            signed_overflow: !multiply && signed_a && a == 0x80000000 && b == u32::MAX,
            description: "Load working registers; convert signed operands to magnitudes".into(),
            registers,
            circuits: vec![ca, cb],
            control: None,
            working,
        }
    }

    pub fn advance(&mut self) {
        if self.ready {
            return;
        }
        self.execute_cycle += 1;
        if self.iteration == 32 {
            self.finish();
        } else {
            self.stage = "iterate".into();
            self.iteration += 1;
            if self.kind == "multiply" {
                self.multiply_bit();
            } else {
                self.divide_bit();
            }
        }
    }

    fn multiply_bit(&mut self) {
        let w = &mut self.working;
        let select = w.quotient & 1;
        // Every bit of M passes through an AND gate with Q[0].
        let mask = if select == 1 { u64::MAX } else { 0 };
        let partial = w.multiplicand & mask;
        let (sum, circuit) = ripple("Product adder", w.product, partial, false, 64);
        self.control = Some(Control {
            kind: "and".into(),
            label: format!(
                "Q[0] = {select} · original multiplier bit {}",
                self.iteration - 1
            ),
            select,
            width: 64,
            input_one: word(w.multiplicand, 64),
            input_zero: word(0, 64),
            output: word(partial, 64),
        });
        self.registers = vec![
            register("P · product", 64, w.product, sum),
            register("M · multiplicand", 64, w.multiplicand, w.multiplicand << 1),
            register(
                "Q · multiplier",
                32,
                u64::from(w.quotient),
                u64::from(w.quotient >> 1),
            ),
        ];
        self.description = format!(
            "Bit {}: AND with Q[0]={select}; P ← P + partial; M ← M << 1; Q ← Q >> 1",
            self.iteration - 1
        );
        w.product = sum;
        w.multiplicand <<= 1;
        w.quotient >>= 1;
        self.circuits = vec![circuit];
    }

    fn divide_bit(&mut self) {
        let w = &mut self.working;
        let incoming = w.quotient >> 31;
        let shifted = (w.remainder << 1) | u64::from(incoming);
        let (trial, circuit) = ripple("Trial subtractor", shifted, u64::from(w.divisor), true, 33);
        // Subtraction carry-out is the no-borrow signal. It drives both the
        // remainder mux and the new quotient bit; no host division is used.
        let keep = circuit.carry_out;
        let mask = if keep == 1 { u64::MAX } else { 0 };
        let remainder = (trial & mask) | (shifted & !mask);
        let quotient = (w.quotient << 1) | keep;
        self.control = Some(Control {
            kind: "mux".into(),
            label: format!(
                "C33 = {keep} · {} · quotient bit {keep}",
                if keep == 1 {
                    "keep subtraction"
                } else {
                    "restore shifted R"
                }
            ),
            select: keep,
            width: 33,
            input_one: word(trial, 33),
            input_zero: word(shifted, 33),
            output: word(remainder, 33),
        });
        self.registers = vec![
            register("R · remainder", 33, w.remainder, remainder),
            register(
                "D · divisor",
                33,
                u64::from(w.divisor),
                u64::from(w.divisor),
            ),
            register(
                "Q · dividend / quotient",
                32,
                u64::from(w.quotient),
                u64::from(quotient),
            ),
        ];
        self.description = format!("Bit {}: shift Q[31]={incoming} into R; subtract D; {} subtraction; shift {keep} into Q", 32 - self.iteration, if keep == 1 { "keep" } else { "reject" });
        w.remainder = remainder;
        w.quotient = quotient;
        self.circuits = vec![circuit];
    }

    /// Cycle 34: correct signs through the same gates, then select the RV32 word.
    fn finish(&mut self) {
        use Function::*;
        let w = &self.working;
        self.stage = "finish".into();
        self.control = None;
        if self.kind == "multiply" {
            let (product, circuit) = ripple(
                "Product sign correction",
                0,
                w.product,
                self.negate_result,
                64,
            );
            let high = (product >> 32) as u32;
            let low = product as u32;
            self.high = Some(high);
            self.low = Some(low);
            self.result = Some(if self.operation == Mul { low } else { high });
            self.registers = vec![register("P · final product", 64, w.product, product)];
            self.circuits = vec![circuit];
            self.description = format!(
                "Apply product sign; select {} 32 bits",
                if self.operation == Mul { "low" } else { "high" }
            );
        } else {
            let (q, cq) = ripple(
                "Quotient sign correction",
                0,
                u64::from(w.quotient),
                self.negate_result,
                32,
            );
            let (r, cr) = ripple(
                "Remainder sign correction",
                0,
                w.remainder,
                self.negate_remainder,
                32,
            );
            self.quotient = Some(q as u32);
            self.remainder = Some(r as u32);
            self.result = Some(if matches!(self.operation, Rem | Remu) {
                r
            } else {
                q
            } as u32);
            self.registers = vec![
                register("Q · final quotient", 32, u64::from(w.quotient), q),
                register("R · final remainder", 32, w.remainder, r),
            ];
            self.circuits = vec![cq, cr];
            self.description = format!(
                "Apply quotient/remainder signs; select {}",
                if matches!(self.operation, Rem | Remu) {
                    "remainder"
                } else {
                    "quotient"
                }
            );
        }
        self.ready = true;
    }
}

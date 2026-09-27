use visual_cpu::{
    alu::Function,
    muldiv::{ripple, MulDiv},
    Machine, Phase, Simulator, EVENT_ALU, EVENT_MULDIV, RAM_BASE,
};

fn expected(function: Function, a: u32, b: u32) -> u32 {
    use Function::*;
    match function {
        Mul => a.wrapping_mul(b),
        Mulh => (((a as i32 as i64) * (b as i32 as i64)) >> 32) as u32,
        Mulhsu => (((a as i32 as i64) * i64::from(b)) >> 32) as u32,
        Mulhu => ((u64::from(a) * u64::from(b)) >> 32) as u32,
        Div if b == 0 => u32::MAX,
        Div => (a as i32).wrapping_div(b as i32) as u32,
        Divu => a.checked_div(b).unwrap_or(u32::MAX),
        Rem if b == 0 => a,
        Rem => (a as i32).wrapping_rem(b as i32) as u32,
        Remu => a.checked_rem(b).unwrap_or(a),
        _ => unreachable!(),
    }
}

#[test]
fn iterative_gates_match_all_m_operations_and_corner_cases() {
    use Function::*;
    let mut pairs = vec![];
    for a in [0, 1, 5, 17, u32::MAX, 0x80000000, 0x7fffffff, 0xffffffef] {
        for b in [0, 1, 3, 5, u32::MAX, 0x80000000, 0x7fffffff] {
            pairs.push((a, b));
        }
    }
    let mut seed = 0x44564944u32;
    for _ in 0..300 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = seed;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        pairs.push((a, seed));
    }
    for (a, b) in pairs {
        for function in [Mul, Mulh, Mulhsu, Mulhu, Div, Divu, Rem, Remu] {
            let mut unit = MulDiv::new(function, a, b);
            assert!(!unit.ready);
            assert_eq!(unit.result, None);
            for iteration in 1..=32 {
                unit.advance();
                assert_eq!(unit.iteration, iteration);
                assert!(!unit.ready);
                assert_eq!(unit.result, None);
            }
            unit.advance();
            assert!(unit.ready);
            assert_eq!(unit.execute_cycle, 34);
            assert_eq!(
                unit.result,
                Some(expected(function, a, b)),
                "{function:?} {a:x}, {b:x}"
            );
            unit.advance();
            assert_eq!(unit.execute_cycle, 34, "completed unit must not advance");
        }
    }
}

#[test]
fn partial_products_and_restoring_remainders_are_real_register_state() {
    for (a, b) in [
        (5u32, 3u32),
        (0x80000000, u32::MAX),
        (u32::MAX, 0),
        (u32::MAX, 0x80000000),
    ] {
        let mut mul = MulDiv::new(Function::Mulhu, a, b);
        let mut div = MulDiv::new(Function::Divu, a, b);
        for iteration in 1..=32 {
            mul.advance();
            div.advance();
            let mask = (1u64 << iteration) - 1;
            let product = u64::from_str_radix(&mul.registers[0].after, 16).unwrap();
            assert_eq!(product, u64::from(a) * (u64::from(b) & mask));
            let prefix = u64::from(a) >> (32 - iteration);
            let r = u64::from_str_radix(&div.registers[0].after, 16).unwrap();
            let q = u64::from_str_radix(&div.registers[2].after, 16).unwrap() & mask;
            assert_eq!(
                r,
                if b == 0 {
                    prefix
                } else {
                    prefix % u64::from(b)
                }
            );
            assert_eq!(q, if b == 0 { mask } else { prefix / u64::from(b) });
            for circuit in [&mul.circuits[0], &div.circuits[0]] {
                let mut carry = u32::from(circuit.subtract);
                for bit in &circuit.bits {
                    assert_eq!(bit.carry_in, carry);
                    let total = bit.a + bit.effective_b + carry;
                    assert_eq!(bit.sum, total & 1);
                    carry = total >> 1;
                    assert_eq!(bit.carry_out, carry);
                }
                assert_eq!(circuit.carry_out, carry);
            }
            assert_eq!(
                div.control.as_ref().unwrap().select,
                div.circuits[0].carry_out
            );
        }
    }
    // Exercise the 33rd subtractor bit independently of a 32-bit remainder.
    let (value, trace) = ripple("wide subtract", 0x1fffffffe, 0xffffffff, true, 33);
    assert_eq!(value, 0xffffffff);
    assert_eq!(trace.carry_out, 1);
    let (_, trace) = ripple("borrow", 1, 2, true, 33);
    assert_eq!(trace.carry_out, 0);
}

#[test]
fn cpu_stalls_in_execute_until_final_result_and_rewinds_mid_operation() {
    let mut cpu = Machine::empty();
    // mul ra, ra, sp: aliasing the destination must not overwrite latched input.
    let word = (1u32 << 25) | (2 << 20) | (1 << 15) | (1 << 7) | 0x33;
    cpu.ram[..4].copy_from_slice(&word.to_le_bytes());
    cpu.executable.push((RAM_BASE, RAM_BASE + 4));
    cpu.regs[1] = 5;
    cpu.regs[2] = 3;
    while cpu.phase != Phase::Execute {
        cpu.tick();
    }
    let start = cpu.cycle;
    for cycle in 1..=33 {
        cpu.tick();
        assert_eq!(cpu.phase, Phase::Execute);
        assert_eq!(cpu.regs[1], 5);
        let datapath = cpu.datapath.as_ref().unwrap();
        assert!(datapath.write_value.is_none());
        assert_eq!(
            datapath
                .alu
                .as_ref()
                .unwrap()
                .muldiv
                .as_ref()
                .unwrap()
                .execute_cycle,
            cycle
        );
    }
    cpu.tick();
    assert_eq!(cpu.cycle - start, 34);
    assert_eq!(cpu.phase, Phase::Memory);
    assert_eq!(cpu.datapath.as_ref().unwrap().write_value, Some(15));
    assert_eq!(cpu.regs[1], 5);
    cpu.step();
    assert_eq!(cpu.regs[1], 15);
    assert_eq!(
        cpu.datapath
            .as_ref()
            .unwrap()
            .read_a
            .as_ref()
            .unwrap()
            .value,
        5
    );
    cpu.back();
    assert_eq!((cpu.cycle, cpu.regs[1]), (0, 5));
    while cpu.phase != Phase::Execute {
        cpu.tick();
    }
    for _ in 0..8 {
        cpu.tick();
    }
    assert!(cpu.back());
    assert!(cpu.datapath.is_none());
    cpu.step();
    assert_eq!(cpu.regs[1], 15);
}

#[test]
fn multiply_divide_event_stops_on_each_cycle_without_an_early_alu_result() {
    let mut sim = Simulator::new(include_bytes!("../web/programs/alu.elf")).unwrap();
    sim.run_until(100000, &[], false, EVENT_MULDIV);
    assert_eq!(sim.stop_reason(), "multiply/divide cycle");
    for iteration in 1..=32 {
        assert_eq!(
            sim.run_until(100000, &[], false, EVENT_MULDIV | EVENT_ALU),
            1
        );
        assert_eq!(sim.stop_reason(), "multiply/divide cycle");
        let s: serde_json::Value = serde_json::from_str(&sim.snapshot()).unwrap();
        assert_eq!(s["datapath"]["alu"]["muldiv"]["iteration"], iteration);
        assert_eq!(s["datapath"]["alu"]["ready"], false);
    }
    assert_eq!(sim.run_until(100000, &[], false, EVENT_ALU), 1);
    assert_eq!(sim.stop_reason(), "ALU result");
    let s: serde_json::Value = serde_json::from_str(&sim.snapshot()).unwrap();
    assert_eq!(s["datapath"]["alu"]["result"], 15);
    assert_eq!(s["datapath"]["committed"], false);
}

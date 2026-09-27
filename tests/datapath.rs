use visual_cpu::{
    alu::{add, evaluate, Function},
    Machine, Phase, Simulator, RAM_BASE,
};

#[test]
fn full_adders_match_integer_arithmetic_and_carry_truth_table() {
    let mut seed = 0x52101u32;
    let mut values = vec![
        (5, 3),
        (0, 1),
        (u32::MAX, 1),
        (0x7fffffff, 1),
        (0x80000000, 1),
    ];
    for _ in 0..2000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = seed;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        values.push((a, seed));
    }
    for (a, b) in values {
        for subtract in [false, true] {
            let trace = add(a, b, subtract);
            assert_eq!(
                trace.result,
                if subtract {
                    a.wrapping_sub(b)
                } else {
                    a.wrapping_add(b)
                }
            );
            let wide = a as u64 + (if subtract { !b } else { b }) as u64 + u64::from(subtract);
            assert_eq!(trace.carry_out, (wide >> 32) as u32);
            for bit in &trace.bits {
                let total = bit.a + bit.effective_b + bit.carry_in;
                assert_eq!((bit.sum, bit.carry_out), (total & 1, total >> 1));
            }
        }
        let signed = evaluate(Function::Slt, a, b);
        assert_eq!(signed.result, u32::from((a as i32) < (b as i32)));
        assert_eq!(evaluate(Function::Sltu, a, b).result, u32::from(a < b));
    }
    let trace = add(5, 3, false);
    assert_eq!(
        trace.bits[..4]
            .iter()
            .map(|b| b.carry_in)
            .collect::<Vec<_>>(),
        [0, 1, 1, 1]
    );
    assert!(add(0x7fffffff, 1, false).overflow);
    assert!(!add(u32::MAX, 1, false).overflow);
}

#[test]
fn five_stage_shifter_matches_rv32_results() {
    for value in [0, 1, u32::MAX, 0x80000000, 0x12345678] {
        for amount in 0..64 {
            for (function, expected) in [
                (Function::Sll, value << (amount & 31)),
                (Function::Srl, value >> (amount & 31)),
                (Function::Sra, ((value as i32) >> (amount & 31)) as u32),
            ] {
                let trace = evaluate(function, value, amount);
                assert_eq!(trace.result, expected);
                assert_eq!(trace.shifter.stages.len(), 5);
                assert_eq!(trace.shifter.stages[4].output, expected);
                for pair in trace.shifter.stages.windows(2) {
                    assert_eq!(pair[0].output, pair[1].input);
                }
            }
        }
    }
}

#[test]
fn ports_remain_latched_when_destination_overwrites_source() {
    let mut cpu = Machine::empty();
    let word = 0xfff08093u32; // addi ra, ra, -1
    cpu.ram[..4].copy_from_slice(&word.to_le_bytes());
    cpu.executable.push((RAM_BASE, RAM_BASE + 4));
    cpu.regs[1] = 5;
    while cpu.phase != Phase::Execute {
        cpu.tick();
    }
    let d = cpu.datapath.as_ref().unwrap();
    assert_eq!(d.read_a.as_ref().unwrap().value, 5);
    assert!(d.read_b.is_none());
    assert_eq!(d.right, u32::MAX);
    assert!(d.alu.is_none());
    cpu.step();
    assert_eq!(cpu.regs[1], 4);
    let d = cpu.datapath.as_ref().unwrap();
    assert_eq!(d.read_a.as_ref().unwrap().value, 5);
    assert_eq!(d.alu.as_ref().unwrap().result, 4);
    assert_eq!((d.before, d.write_value, d.committed), (5, Some(4), true));
    cpu.back();
    assert!(cpu.datapath.is_none());
    assert_eq!(cpu.regs[1], 5);
}

#[test]
fn cache_probe_preserves_pre_fill_valid_and_tag_comparison() {
    let mut cpu = Machine::empty();
    for (n, word) in [0x0000a183u32, 0x0000a203].iter().enumerate() {
        cpu.ram[n * 4..n * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    cpu.executable.push((RAM_BASE, RAM_BASE + 8));
    cpu.regs[1] = RAM_BASE + 4096 + 12;
    cpu.ram[4108..4112].copy_from_slice(&42u32.to_le_bytes());
    cpu.step();
    let access = cpu.cache_accesses[1].as_ref().unwrap();
    let probe = &access.probes[0];
    assert_eq!(
        (probe.index, probe.offset, probe.tag),
        (0, 12, (RAM_BASE + 4096) >> 9)
    );
    assert!(!probe.valid && !probe.hit && probe.stored_tag.is_none());
    assert!(cpu.caches[1].lines[0].valid);
    assert!(access.complete && access.filled.contains(&1));
    assert_eq!(access.value, 42);
    cpu.step();
    let access = cpu.cache_accesses[1].as_ref().unwrap();
    assert!(access.probes[0].hit);
    assert_eq!(access.probes[0].stored_tag, Some(access.probes[0].tag));
    assert!(access.filled.is_empty());
    cpu.back();
    assert!(!cpu.cache_accesses[1].as_ref().unwrap().probes[0].hit);
}

#[test]
fn cold_fetch_walks_down_and_refills_up_one_level_at_a_time() {
    let mut cpu = Machine::empty();
    cpu.ram[..4].copy_from_slice(&0x00500513u32.to_le_bytes());
    cpu.executable.push((RAM_BASE, RAM_BASE + 4));
    for (cycle, expected_probes, stage, filled) in [
        (1, 1, "L2 lookup", vec![]),
        (7, 2, "L3 lookup", vec![]),
        (25, 3, "RAM read", vec![]),
        (85, 3, "L3 refill", vec![]),
        (86, 3, "L2 refill", vec![3]),
        (87, 3, "L1I refill", vec![3, 2]),
        (88, 3, "Complete", vec![3, 2, 0]),
    ] {
        while cpu.cycle < cycle {
            cpu.tick();
        }
        let a = cpu.access.as_ref().unwrap();
        assert_eq!(a.probes.len(), expected_probes);
        assert_eq!(a.stage, stage);
        assert_eq!(a.filled, filled);
        for index in [0, 2, 3] {
            assert_eq!(
                cpu.caches[index].contains(RAM_BASE),
                filled.contains(&index)
            );
        }
    }
    assert_eq!(cpu.phase, Phase::Decode);
    assert_eq!(cpu.regs[10], 0);
    cpu.step();
    assert_eq!(cpu.regs[10], 5);
    cpu.back();
    assert!(cpu.caches.iter().all(|c| c.fills == 0));
}

#[test]
fn event_controls_stop_on_exact_cycle_and_retain_trace() {
    let bytes = include_bytes!("../web/programs/array_sum.elf");
    let mut sim = Simulator::new(bytes).unwrap();
    assert_eq!(sim.run_until(10000, &[], false, 2), 1);
    assert_eq!(sim.stop_reason(), "cache miss");
    sim.run_until(10000, &[], false, 4);
    assert_eq!(sim.stop_reason(), "cache fill");
    sim.run_until(10000, &[], false, 8);
    assert_eq!(sim.stop_reason(), "ALU result");
    let before: serde_json::Value = serde_json::from_str(&sim.snapshot()).unwrap();
    assert_eq!(before["retired"], 0);
    assert_eq!(before["datapath"]["committed"], false);
    let expected = before["datapath"]["write_value"].clone();
    assert_eq!(sim.run_until(10000, &[], false, 1), 2);
    assert_eq!(sim.stop_reason(), "register write");
    let after: serde_json::Value = serde_json::from_str(&sim.snapshot()).unwrap();
    assert_eq!(after["datapath"]["committed"], true);
    assert_eq!(after["regs"][2], expected);
    assert_eq!(
        after["log"].as_array().unwrap().last().unwrap()["kind"],
        "register_write"
    );
}

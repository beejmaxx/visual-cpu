use serde_json::Value;
use visual_cpu::{Machine, Phase, RAM_BASE};

fn report(m: &Machine) -> Value {
    serde_json::from_str::<Value>(&m.snapshot()).unwrap()["cycle_detail"].clone()
}
fn first() -> Machine {
    let mut m = Machine::empty();
    m.ram[..8].copy_from_slice(&[0x97, 0, 0, 0, 0x13, 1, 0x40, 0]);
    m.executable.push((RAM_BASE, RAM_BASE + 8));
    m
}
fn at(m: &mut Machine, cycle: u64) {
    while m.cycle < cycle {
        m.tick();
        assert!(m.fault.is_none());
    }
}

#[test]
fn cold_first_instruction_explains_each_stall_and_completed_phase() {
    let mut m = first();
    assert_eq!(report(&m)["phase"], Value::Null);
    for field in ["before", "after", "memory_clock"] {
        assert_eq!(report(&m)[field], Value::Null);
    }
    at(&mut m, 1);
    assert_eq!(m.pc, RAM_BASE);
    assert_eq!(m.retired, 0);
    assert!(report(&m)["title"]
        .as_str()
        .unwrap()
        .starts_with("L1I miss"));
    assert_eq!(report(&m)["phase"], "Fetch");
    assert!(report(&m)["next"]
        .as_str()
        .unwrap()
        .contains("L2 lookup: 6"));
    assert_eq!(
        report(&m)["memory_clock"],
        serde_json::json!({
            "component": "L1I", "stage": "L1I lookup", "stage_kind": "lookup",
            "remaining_before": 1, "remaining_after": 0, "latency": 1
        })
    );
    assert_eq!(m.access.as_ref().unwrap().stage, "L2 lookup");
    at(&mut m, 2);
    assert_eq!(report(&m)["waiting"], true);
    assert_eq!(report(&m)["progress"]["elapsed"], 1);
    assert_eq!(report(&m)["progress"]["total"], 6);
    assert_eq!(
        report(&m)["memory_clock"],
        serde_json::json!({
            "component": "L2", "stage": "L2 lookup", "stage_kind": "lookup",
            "remaining_before": 6, "remaining_after": 5, "latency": 6
        })
    );
    assert!(report(&m)["reason"]
        .as_str()
        .unwrap()
        .contains("modeled lookup latency"));
    assert_eq!(report(&m)["before"], report(&m)["after"]);
    at(&mut m, 25);
    assert!(report(&m)["title"].as_str().unwrap().starts_with("L3 miss"));
    assert!(report(&m)["next"]
        .as_str()
        .unwrap()
        .contains("RAM read: 60"));
    at(&mut m, 26);
    assert!(report(&m)["title"]
        .as_str()
        .unwrap()
        .contains("59 clocks remaining"));
    assert_eq!(report(&m)["changes"], serde_json::json!([]));
    let waiting = report(&m);
    assert_eq!(
        waiting["memory_clock"],
        serde_json::json!({
            "component": "RAM", "stage": "RAM read", "stage_kind": "read",
            "remaining_before": 60, "remaining_after": 59, "latency": 60
        })
    );
    assert_eq!(waiting["before"], waiting["after"]);
    at(&mut m, 84);
    assert!(report(&m)["title"]
        .as_str()
        .unwrap()
        .contains("1 clock remaining"));
    at(&mut m, 85);
    assert_eq!(report(&m)["title"], "RAM returned a 64-byte line");
    assert_eq!(report(&m)["waiting"], false);
    assert_eq!(report(&m)["memory_clock"]["component"], "RAM");
    assert_eq!(report(&m)["memory_clock"]["remaining_before"], 1);
    assert_eq!(report(&m)["memory_clock"]["remaining_after"], 0);
    assert_eq!(m.access.as_ref().unwrap().stage, "L3 refill");
    assert_eq!(
        report(&m)["after"]["cache_valid"],
        serde_json::json!([0, 0, 0, 0])
    );
    for (cycle, name) in [(86, "L3"), (87, "L2"), (88, "L1I")] {
        at(&mut m, cycle);
        assert!(report(&m)["title"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{name} filled")));
        assert_eq!(report(&m)["memory_clock"]["component"], name);
        assert_eq!(report(&m)["memory_clock"]["stage_kind"], "fill");
        assert_eq!(report(&m)["memory_clock"]["remaining_after"], 0);
        let level = match name {
            "L1I" => 0,
            "L2" => 2,
            "L3" => 3,
            _ => unreachable!(),
        };
        assert_eq!(report(&m)["before"]["cache_valid"][level], 0);
        assert_eq!(report(&m)["after"]["cache_valid"][level], 1);
    }
    assert_eq!(m.phase, Phase::Decode);
    assert_eq!(report(&m)["phase"], "Fetch");
    assert_eq!(report(&m)["instruction_cycle"], 88);
    assert_eq!(report(&m)["before"]["instruction_word"], Value::Null);
    assert_eq!(report(&m)["after"]["instruction_word"], 0x97);
    at(&mut m, 89);
    assert_eq!(report(&m)["phase"], "Decode");
    assert_eq!(report(&m)["changes"][0]["name"], "Latched operands A / B");
    assert_eq!(report(&m)["before"]["operand_a"], Value::Null);
    assert_eq!(report(&m)["after"]["operand_a"], RAM_BASE);
    assert_eq!(report(&m)["after"]["operand_b"], 0);
    assert_eq!(report(&m)["memory_clock"], Value::Null);
    at(&mut m, 90);
    assert_eq!(report(&m)["phase"], "Execute");
    assert_eq!(m.regs[1], 0);
    let executed = report(&m);
    assert_eq!(executed["before"]["alu_result"], Value::Null);
    assert_eq!(executed["after"]["alu_result"], RAM_BASE);
    assert_eq!(executed["before"]["write_value"], Value::Null);
    assert_eq!(executed["after"]["write_value"], RAM_BASE);
    assert_eq!(executed["before"]["regs"], executed["after"]["regs"]);
    at(&mut m, 91);
    assert_eq!(report(&m)["title"], "Memory phase: no data access");
    assert_eq!(m.regs[1], 0);
    at(&mut m, 92);
    assert_eq!(m.retired, 1);
    assert_eq!(m.regs[1], RAM_BASE);
    assert_eq!(m.phase, Phase::Fetch);
    let written = report(&m);
    assert_eq!(written["phase"], "Writeback");
    assert_eq!(written["instruction_cycle"], 92);
    assert_eq!(written["changes"][0]["before"], "0 (0x00000000)");
    assert_eq!(written["inputs"][2]["name"], "Write port data");
    assert_eq!(written["inputs"][2]["value"], "2147483648 (0x80000000)");
    assert_eq!(written["before"]["regs"][1], 0);
    assert_eq!(written["after"]["regs"][1], RAM_BASE);
    assert_eq!(written["before"]["pc"], RAM_BASE);
    assert_eq!(written["after"]["pc"], RAM_BASE + 4);
    assert!(m.seek_cycle(26));
    assert_eq!(report(&m), waiting);
    assert!(report(&m)["title"]
        .as_str()
        .unwrap()
        .contains("59 clocks remaining"));
    assert!(m.seek_cycle(90));
    assert_eq!(report(&m), executed);
    assert!(m.seek_cycle(92));
    assert_eq!(report(&m), written);
    at(&mut m, 93);
    assert_eq!(report(&m)["instruction_cycle"], 1);
    assert!(report(&m)["title"].as_str().unwrap().starts_with("L1I hit"));
}

#[test]
fn multiplication_exposes_real_internal_latches_without_early_register_write() {
    let mut m = Machine::empty();
    let raw = 1u32 << 25 | 2 << 20 | 1 << 15 | 3 << 7 | 0x33;
    m.ram[..4].copy_from_slice(&raw.to_le_bytes());
    m.executable.push((RAM_BASE, RAM_BASE + 4));
    m.regs[1] = 3;
    m.regs[2] = 42;
    while m.phase != Phase::Execute {
        m.tick();
    }
    m.tick();
    assert!(report(&m)["title"].as_str().unwrap().contains("prepare"));
    assert_eq!(report(&m)["before"]["alu_result"], Value::Null);
    assert_eq!(report(&m)["after"]["alu_result"], Value::Null);
    for iteration in 1..=32 {
        m.tick();
        let r = report(&m);
        let md = m
            .datapath
            .as_ref()
            .unwrap()
            .alu
            .as_ref()
            .unwrap()
            .muldiv
            .as_ref()
            .unwrap();
        assert_eq!(r["progress"]["elapsed"], iteration + 1);
        assert_eq!(m.regs[3], 0);
        assert_eq!(r["before"]["alu_result"], Value::Null);
        assert_eq!(r["after"]["alu_result"], Value::Null);
        for (i, register) in md.registers.iter().enumerate() {
            assert_eq!(r["inputs"][i + 2]["name"], register.name);
            assert_eq!(
                r["inputs"][i + 2]["value"],
                format!("0x{}", register.before)
            );
            assert_eq!(r["changes"][i]["before"], format!("0x{}", register.before));
            assert_eq!(r["changes"][i]["after"], format!("0x{}", register.after));
        }
    }
    m.tick();
    assert_eq!(report(&m)["progress"]["elapsed"], 34);
    assert_eq!(report(&m)["before"]["alu_result"], Value::Null);
    assert_eq!(report(&m)["after"]["alu_result"], 126);
    assert_eq!(m.regs[3], 0);
    m.tick();
    assert_eq!(report(&m)["phase"], "Memory");
    m.tick();
    assert_eq!(m.regs[3], 126);
    assert_eq!(report(&m)["phase"], "Writeback");
}

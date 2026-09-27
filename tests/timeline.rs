use serde_json::{json, Value};
use visual_cpu::{Machine, Phase, Simulator, RAM_BASE, UART};

fn i(op: u32, f3: u32, rd: u32, rs1: u32, imm: u32) -> u32 {
    (imm & 0xfff) << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op
}
fn store(f3: u32, rs1: u32, rs2: u32, imm: u32) -> u32 {
    (imm >> 5) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | (imm & 31) << 7 | 0x23
}
fn machine(words: &[u32]) -> Machine {
    let mut m = Machine::empty();
    for (n, word) in words.iter().enumerate() {
        m.ram[n * 4..n * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    m.executable
        .push((RAM_BASE, RAM_BASE + words.len() as u32 * 4));
    m
}
fn state(m: &Machine) -> Value {
    let mut v: Value = serde_json::from_str(&m.snapshot()).unwrap();
    for key in ["timeline_start", "timeline_end", "history_depth"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v["data"] = json!(m.memory(RAM_BASE + 4096, 16));
    v["origins"] = serde_json::from_str(&m.provenance.graph("memory", RAM_BASE + 4096, 4)).unwrap();
    v
}

#[test]
fn every_cycle_replays_exact_cache_mul_store_output_state() {
    let mut m = machine(&[
        i(3, 2, 3, 1, 0),                            // lw x3, 0(x1)
        1 << 25 | 2 << 20 | 3 << 15 | 4 << 7 | 0x33, // mul x4, x3, x2
        store(2, 1, 4, 4),
        i(3, 2, 5, 1, 4),
        store(0, 6, 5, 0),
    ]);
    m.regs[1] = RAM_BASE + 4096;
    m.regs[2] = 3;
    m.regs[6] = UART;
    m.ram[4096] = 21;
    let mut expected = vec![state(&m)];
    while m.retired < 5 {
        m.tick();
        assert!(m.fault.is_none());
        expected.push(state(&m));
    }
    assert_eq!(m.console, b"?");
    let end = m.cycle;
    for cycle in (0..=end).rev() {
        assert!(m.seek_cycle(cycle), "seek {cycle}");
        assert_eq!(state(&m), expected[cycle as usize], "cycle {cycle}");
    }
    assert!(m.seek_cycle(end));
    assert_eq!(state(&m), expected[end as usize]);
    assert!(!m.seek_cycle(end + 1));
    assert_eq!(m.cycle, end);
    let waves: Value = serde_json::from_str(&m.wave_window(4, 128)).unwrap();
    let samples = waves["samples"].as_array().unwrap();
    let write = samples.iter().find(|s| s["we"] == true).unwrap();
    assert_eq!(write["q"], 63);
    assert_eq!(write["selected_d"], 63);
    let cycle = write["cycle"].as_u64().unwrap();
    let before = samples.iter().find(|s| s["cycle"] == cycle - 1).unwrap();
    assert_eq!(before["q"], 0);
    assert_eq!(before["selected_d"], 63);
    assert_eq!(before["we"], false);
}

#[test]
fn input_replays_once_and_new_input_forks_future() {
    let mut m = machine(&[
        i(3, 4, 3, 1, 4),
        store(0, 1, 3, 0),
        i(3, 4, 3, 1, 4),
        store(0, 1, 3, 0),
    ]);
    m.regs[1] = UART;
    m.tick();
    let before = m.cycle;
    m.tick();
    m.input(b"AB");
    while m.retired < 4 {
        m.tick();
    }
    let end = m.cycle;
    assert_eq!(m.console, b"AB");
    assert!(m.seek_cycle(before));
    assert!(m.seek_cycle(end));
    assert_eq!(m.console, b"AB");
    assert_eq!(
        serde_json::from_str::<Value>(&m.snapshot()).unwrap()["input_pending"],
        0
    );
    assert!(m.seek_cycle(before));
    m.input(b"CD");
    assert_eq!(m.timeline_end(), before);
    assert!(!m.seek_cycle(end));
    while m.retired < 4 {
        m.tick();
    }
    assert_eq!(m.console, b"CD");
}

#[test]
fn provenance_tracks_versions_and_partial_stores_not_equal_numbers() {
    let mut m = machine(&[
        i(3, 2, 3, 1, 0),
        i(3, 2, 4, 1, 4),
        i(0x13, 0, 5, 3, 1),
        i(0x13, 0, 3, 0, 99),
        store(0, 1, 5, 1),
        i(3, 2, 6, 1, 0),
    ]);
    m.regs[1] = RAM_BASE + 4096;
    m.ram[4096] = 7;
    m.ram[4100] = 7;
    m.step();
    let source_a = m.provenance.memory_roots(RAM_BASE + 4096, 4);
    let old_r3 = m.provenance.registers[3];
    m.step();
    let source_b = m.provenance.memory_roots(RAM_BASE + 4100, 4);
    assert_ne!(source_a, source_b);
    assert!(!m
        .provenance
        .depends_on(m.provenance.registers[4], &source_a));
    m.step();
    m.step();
    assert!(m.provenance.depends_on(
        m.provenance.registers[5],
        &old_r3.into_iter().collect::<Vec<_>>()
    ));
    assert!(!m
        .provenance
        .depends_on(m.provenance.registers[3], &source_a));
    let before = m.cycle;
    m.step();
    m.step();
    assert_eq!(m.regs[6], 0x807);
    assert_eq!(m.provenance.memory_roots(RAM_BASE + 4096, 4).len(), 2);
    let graph: Value = serde_json::from_str(&m.provenance.graph("register", 6, 4)).unwrap();
    let store = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "store")
        .unwrap();
    assert_eq!(store["access"]["complete"], true);
    assert!(m.seek_cycle(before));
    assert_eq!(m.provenance.memory_roots(RAM_BASE + 4096, 4), source_a);
    assert_eq!(m.memory(RAM_BASE + 4096, 4), [7, 0, 0, 0]);
}

#[test]
fn bounded_history_remains_seekable() {
    let mut m = machine(&[0x0000006f]); // jal x0, 0
    for _ in 0..5000 {
        m.tick();
    }
    let start = m.timeline_start();
    let end = m.timeline_end();
    assert!(start > 0 && end - start <= 4096);
    assert!(!m.seek_cycle(start - 1));
    assert!(m.seek_cycle(start));
    assert_eq!(m.phase, Phase::Fetch);
    assert!(m.seek_cycle(end));
}

#[test]
fn watch_follows_selected_array_word_through_multiply() {
    let binary = std::fs::read("web/programs/flow.elf").unwrap();
    let mut sim = Simulator::new(&binary).unwrap();
    let s: Value = serde_json::from_str(&sim.snapshot()).unwrap();
    let samples = s["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "samples")
        .unwrap()["address"]
        .as_u64()
        .unwrap() as u32;
    sim.watch(samples + 8, 4);
    let mut saw_cache = false;
    let mut saw_mul = false;
    let mut saw_result = false;
    for _ in 0..150 {
        sim.run_until(10000, &[], false, 128);
        let s: Value = serde_json::from_str(&sim.snapshot()).unwrap();
        if s["halted"] == true {
            break;
        }
        assert_eq!(sim.stop_reason(), "watched value");
        if s["access"]["address"] == samples + 8 {
            saw_cache = true;
        }
        if s["datapath"]["alu"]["function"] == "mul" {
            saw_mul = true;
            if s["datapath"]["committed"] == true && s["datapath"]["write_value"] == 126 {
                saw_result = true;
                break;
            }
        }
    }
    assert!(saw_cache && saw_mul && saw_result);
}

#[test]
fn oldest_wave_sample_can_be_inside_a_stalled_instruction() {
    let mut m = machine(&[store(2, 1, 2, 0); 100]);
    m.regs[1] = RAM_BASE + 4096;
    m.regs[2] = 42;
    let mut stalled = std::collections::HashSet::new();
    while m.retired < 90 {
        m.tick();
        if m.phase == Phase::Memory && m.access.as_ref().is_some_and(|a| !a.complete) {
            stalled.insert(m.cycle);
        }
    }
    while !stalled.contains(&m.timeline_start()) {
        m.tick();
    }
    let end = m.cycle;
    let expected = state(&m);
    let start = m.timeline_start();
    assert!(start > 0);
    assert!(m.seek_cycle(start));
    assert_ne!(m.phase, Phase::Fetch);
    assert!(
        !m.back(),
        "instruction back must not leave the retained cycle window"
    );
    assert!(!m.back_cycle());
    assert!(m.seek_cycle(end));
    assert_eq!(state(&m), expected);
}

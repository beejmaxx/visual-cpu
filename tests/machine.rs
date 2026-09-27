use visual_cpu::{isa::decode, Machine, RAM_BASE, UART};

fn r(f7: u32, f3: u32, rd: u32, rs1: u32, rs2: u32) -> u32 {
    f7 << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | rd << 7 | 0x33
}
fn i(op: u32, f3: u32, rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32) & 0xfff) << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op
}
fn machine(words: &[u32]) -> Machine {
    let mut cpu = Machine::empty();
    for (n, word) in words.iter().enumerate() {
        cpu.ram[n * 4..n * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    cpu.executable
        .push((RAM_BASE, RAM_BASE + words.len() as u32 * 4));
    cpu
}

#[test]
fn integer_corner_cases_follow_rv32im() {
    let cases = [
        (r(0, 0, 3, 1, 2), u32::MAX, 1, 0),
        (r(0x20, 0, 3, 1, 2), 0, 1, u32::MAX),
        (r(0, 1, 3, 1, 2), 1, 33, 2),
        (r(0x20, 5, 3, 1, 2), 0x80000000, 4, 0xf8000000),
        (r(0, 5, 3, 1, 2), 0x80000000, 4, 0x08000000),
        (r(0, 2, 3, 1, 2), u32::MAX, 1, 1),
        (r(0, 3, 3, 1, 2), u32::MAX, 1, 0),
        (r(1, 0, 3, 1, 2), u32::MAX, 2, 0xfffffffe),
        (r(1, 1, 3, 1, 2), 0x80000000, 2, u32::MAX),
        (r(1, 2, 3, 1, 2), 0x80000000, u32::MAX, 0x80000000),
        (r(1, 3, 3, 1, 2), u32::MAX, u32::MAX, 0xfffffffe),
        (r(1, 4, 3, 1, 2), 0x80000000, u32::MAX, 0x80000000),
        (r(1, 4, 3, 1, 2), 123, 0, u32::MAX),
        (r(1, 5, 3, 1, 2), 123, 0, u32::MAX),
        (r(1, 6, 3, 1, 2), 123, 0, 123),
        (r(1, 6, 3, 1, 2), 0x80000000, u32::MAX, 0),
        (r(1, 7, 3, 1, 2), 123, 0, 123),
    ];
    for (word, a, b, expected) in cases {
        let mut c = machine(&[word]);
        c.regs[1] = a;
        c.regs[2] = b;
        c.step();
        assert_eq!(c.fault, None);
        assert_eq!(
            c.regs[3],
            expected,
            "{}",
            decode(RAM_BASE, word).unwrap().text
        );
    }
}

#[test]
fn signed_immediates_and_zero_register() {
    let mut c = machine(&[
        i(0x13, 0, 1, 0, -1),
        i(0x13, 0, 0, 1, 7),
        i(0x13, 3, 2, 0, -1),
    ]);
    for _ in 0..3 {
        c.step();
    }
    assert_eq!(c.regs[1], u32::MAX);
    assert_eq!(c.regs[0], 0);
    assert_eq!(c.regs[2], 1);
}

#[test]
fn cold_and_warm_data_reads_have_real_latency() {
    let mut c = machine(&[i(3, 2, 3, 1, 0), i(3, 2, 4, 1, 4)]);
    c.regs[1] = RAM_BASE + 4096;
    c.ram[4096..4104].copy_from_slice(&[42, 0, 0, 0, 43, 0, 0, 0]);
    c.step();
    assert_eq!(c.regs[3], 42);
    let cold = c.cycle;
    assert_eq!(c.caches[1].misses, 1);
    assert!(c.caches[3].contains(RAM_BASE + 4096));
    c.step();
    assert_eq!(c.regs[4], 43);
    assert_eq!(c.caches[1].hits, 1);
    assert_eq!(c.cycle - cold, 5);
    assert!(cold > 100);
}

#[test]
fn conflicts_fall_through_l2_l3_and_ram_without_stale_data() {
    let mut c = machine(&[i(3, 2, 3, 1, 0); 7]);
    let base = RAM_BASE + 0x10000;
    for (offset, value) in [(0, 11u32), (512, 22), (2048, 33), (8192, 44)] {
        let at = (base - RAM_BASE + offset) as usize;
        c.ram[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (offset, expected, latency) in [
        (0, 11, 88),
        (512, 22, 88),
        (0, 11, 8),
        (2048, 33, 88),
        (0, 11, 27),
        (8192, 44, 88),
        (0, 11, 88),
    ] {
        c.regs[1] = base + offset;
        c.step();
        assert_eq!(c.fault, None);
        assert_eq!(c.regs[3], expected);
        assert_eq!(c.access.as_ref().unwrap().latency, latency);
    }
    assert!(c.caches[1..].iter().all(|cache| cache.evictions > 0));
}

#[test]
fn signed_loads_and_little_endian() {
    let mut c = machine(&[
        i(3, 0, 3, 1, 0),
        i(3, 4, 4, 1, 0),
        i(3, 1, 5, 1, 0),
        i(3, 5, 6, 1, 0),
    ]);
    c.regs[1] = RAM_BASE + 4096;
    c.ram[4096..4098].copy_from_slice(&[0x80, 0xff]);
    for _ in 0..4 {
        c.step();
    }
    assert_eq!(
        [c.regs[3], c.regs[4], c.regs[5], c.regs[6]],
        [0xffffff80, 128, 0xffffff80, 0xff80]
    );
}

#[test]
fn stores_and_back_restore_ram_caches_registers_and_cycles() {
    let sw = 2 << 20 | 1 << 15 | 2 << 12 | 0x23;
    let mut c = machine(&[sw, i(3, 2, 3, 1, 0)]);
    c.regs[1] = RAM_BASE + 4096;
    c.regs[2] = 0x12345678;
    c.step();
    assert_eq!(&c.ram[4096..4100], &[0x78, 0x56, 0x34, 0x12]);
    for cache in &c.caches {
        if cache.contains(RAM_BASE + 4096) {
            assert_eq!(
                &cache.lines[cache.index(RAM_BASE + 4096)].data[..4],
                &[0x78, 0x56, 0x34, 0x12]
            );
        }
    }
    assert!(c.back());
    assert_eq!(&c.ram[4096..4100], &[0, 0, 0, 0]);
    assert_eq!(c.cycle, 0);
    assert!(c.caches.iter().all(|x| x.lines.iter().all(|l| !l.valid)));
    c.step();
    c.step();
    assert_eq!(c.regs[3], 0x12345678);
}

#[test]
fn uart_input_output_and_rewind() {
    let sb = 2 << 20 | 1 << 15 | 0x23;
    let mut c = machine(&[sb, i(3, 0, 3, 1, 4)]);
    c.regs[1] = UART;
    c.regs[2] = b'A' as u32;
    c.input(b"Z");
    c.step();
    assert_eq!(c.console, b"A");
    c.back();
    assert!(c.console.is_empty());
    c.step();
    c.step();
    assert_eq!(c.regs[3], b'Z' as u32);
    c.back();
    c.step();
    assert_eq!(c.regs[3], b'Z' as u32);
}

#[test]
fn guest_errors_do_not_crash_host() {
    let mut c = machine(&[i(3, 2, 3, 1, 0)]);
    c.regs[1] = RAM_BASE + 1;
    c.step();
    assert!(c.fault.unwrap().contains("Misaligned"));
    let mut c = machine(&[u32::MAX]);
    c.step();
    assert!(c.fault.unwrap().contains("Unsupported"));
    let mut c = machine(&[i(3, 2, 3, 1, 0)]);
    c.step();
    assert!(c.fault.unwrap().contains("Unmapped"));
}

fn run_program(bytes: &[u8], input: &[u8]) -> Machine {
    let mut c = Machine::from_elf(bytes).unwrap();
    c.input(input);
    while !c.halted && c.cycle < 5_000_000 {
        c.tick();
    }
    assert!(c.halted, "Program exceeded cycle limit");
    assert_eq!(c.fault, None);
    assert_eq!(c.exit_code, Some(0));
    c
}

#[test]
fn compiled_c_examples_execute() {
    let c = run_program(include_bytes!("../web/programs/flow.elf"), b"");
    assert_eq!(String::from_utf8_lossy(&c.console), "126\n");
    let c = run_program(include_bytes!("../web/programs/array_sum.elf"), b"");
    assert_eq!(
        String::from_utf8_lossy(&c.console),
        "First pass:  136\nSecond pass: 136\n"
    );
    let c = run_program(include_bytes!("../web/programs/fibonacci.elf"), b"");
    assert!(String::from_utf8_lossy(&c.console).contains("0 1 1 2 3 5 8 13 21 34\n"));
    let c = run_program(include_bytes!("../web/programs/sort.elf"), b"");
    assert_eq!(
        String::from_utf8_lossy(&c.console),
        "Quicksort\n2 3 5 7 11 18 29 42 55 64 76 91\n"
    );
    let c = run_program(include_bytes!("../web/programs/echo.elf"), b"hello\n");
    assert_eq!(String::from_utf8_lossy(&c.console), "> hello\n");
}

#[test]
fn elf_validation_rejects_truncation_and_wrong_targets() {
    let bytes = include_bytes!("../web/programs/array_sum.elf");
    for end in [0, 4, 20, 51, 52, 64, 100] {
        assert!(Machine::from_elf(&bytes[..end]).is_err());
    }
    let mut altered = bytes.to_vec();
    altered[18] = 0;
    assert!(Machine::from_elf(&altered).is_err());
    let mut altered = bytes.to_vec();
    altered[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Machine::from_elf(&altered).is_err());
    let mut altered = bytes.to_vec();
    altered[32..36].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
    assert!(Machine::from_elf(&altered).is_err());
}

#[test]
fn inspection_does_not_change_execution() {
    let mut c = Machine::from_elf(include_bytes!("../web/programs/array_sum.elf")).unwrap();
    for _ in 0..3 {
        c.step();
    }
    let before = c.snapshot();
    let _ = c.memory(RAM_BASE, 128);
    let _ = c.program();
    assert_eq!(before, c.snapshot());
}

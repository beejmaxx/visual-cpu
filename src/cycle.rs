//! What one clock actually did, recorded alongside its waveform sample.
use crate::{isa::REG_NAMES, Machine, MemoryStage, Phase, EVENT_REGISTER};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Value {
    pub name: String,
    pub value: String,
}
#[derive(Clone, Serialize)]
pub struct Change {
    pub name: String,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Serialize)]
pub struct Progress {
    pub name: String,
    pub elapsed: u32,
    pub total: u32,
}
#[derive(Clone, Serialize)]
pub struct ClockState {
    pub pc: u32,
    pub regs: [u32; 32],
    pub instruction_word: Option<u32>,
    pub operand_a: Option<u32>,
    pub operand_b: Option<u32>,
    pub alu_result: Option<u32>,
    pub write_value: Option<u32>,
    pub memory_address: Option<u32>,
    pub cache_valid: [usize; 4],
}

impl ClockState {
    fn capture(m: &Machine) -> Self {
        let d = m.datapath.as_ref();
        Self {
            pc: m.pc,
            regs: m.regs,
            instruction_word: m.instruction.as_ref().map(|i| i.raw),
            operand_a: d.map(|d| d.left),
            operand_b: d.map(|d| d.right),
            alu_result: d
                .and_then(|d| d.alu.as_ref())
                .filter(|a| a.ready)
                .map(|a| a.result),
            write_value: d.and_then(|d| d.write_value),
            memory_address: d.and_then(|d| d.memory_address),
            cache_valid: std::array::from_fn(|level| {
                m.caches[level]
                    .lines
                    .iter()
                    .filter(|line| line.valid)
                    .count()
            }),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct MemoryClock {
    pub component: String,
    pub stage: String,
    pub stage_kind: String,
    pub remaining_before: u32,
    pub remaining_after: u32,
    pub latency: u32,
}

#[derive(Clone, Serialize)]
pub struct Detail {
    pub phase: Option<Phase>,
    pub pc: u32,
    pub instruction: String,
    pub instruction_cycle: u64,
    pub title: String,
    pub reason: String,
    pub inputs: Vec<Value>,
    pub changes: Vec<Change>,
    pub held: String,
    pub next: String,
    pub waiting: bool,
    pub progress: Option<Progress>,
    pub components: Vec<String>,
    pub inspector: String,
    pub before: Option<ClockState>,
    pub after: Option<ClockState>,
    pub memory_clock: Option<MemoryClock>,
}

pub(crate) struct Before {
    pub phase: Phase,
    pub pc: u32,
    regs: [u32; 32],
    raw: Option<u32>,
    stage: Option<(MemoryStage, u32)>,
    old_line: Option<u32>,
    old_bytes: Vec<u8>,
    state: ClockState,
}

impl Before {
    pub fn capture(m: &Machine) -> Self {
        let stage = m
            .transaction
            .as_ref()
            .map(|t| (t.stage, t.access.remaining));
        let old_line = m.transaction.as_ref().and_then(|t| {
            if let MemoryStage::Fill(level) = t.stage {
                let c = &m.caches[level];
                let line = &c.lines[c.index(t.access.address)];
                line.valid.then_some(line.address)
            } else {
                None
            }
        });
        let old_bytes = m
            .transaction
            .as_ref()
            .filter(|t| matches!(t.stage, MemoryStage::RamWrite) && t.access.remaining == 1)
            .map(|t| m.memory(t.access.address, t.access.size))
            .unwrap_or_default();
        Self {
            phase: m.phase,
            pc: m.pc,
            regs: m.regs,
            raw: m.instruction.as_ref().map(|i| i.raw),
            stage,
            old_line,
            old_bytes,
            state: ClockState::capture(m),
        }
    }
}

fn hex(value: u32) -> String {
    format!("0x{value:08x}")
}
fn number(value: u32) -> String {
    format!("{value} ({})", hex(value))
}
fn reg(index: usize) -> String {
    format!("x{index} / {}", REG_NAMES[index])
}
fn bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Detail {
    pub fn initial(pc: u32) -> Self {
        Self {
            phase:None, pc, instruction:"Instruction not fetched yet".into(), instruction_cycle:0,
            title:"Ready to fetch the first instruction".into(),
            reason:"A clock advances one part of an instruction. Empty caches make the first fetch take 88 clocks; Decode, Execute, Memory and Writeback then take one each for a simple integer instruction.".into(),
            inputs:vec![Value{name:"PC".into(),value:hex(pc)}],changes:vec![],
            held:"No clock has run. Registers still contain their initial values.".into(),
            next:format!("Look up the instruction at {} in L1I.",hex(pc)), waiting:false,
            progress:None,components:vec!["pc".into()],inspector:"cache".into(),
            before:None,after:None,memory_clock:None,
        }
    }
    fn input(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.inputs.push(Value {
            name: name.into(),
            value: value.into(),
        });
    }
    fn change(
        &mut self,
        name: impl Into<String>,
        before: impl Into<String>,
        after: impl Into<String>,
    ) {
        self.changes.push(Change {
            name: name.into(),
            before: before.into(),
            after: after.into(),
        });
    }
}

fn next(m: &Machine) -> String {
    if m.halted {
        return "Execution stopped.".into();
    }
    if let Some(t) = &m.transaction {
        return format!(
            "{}: {} clock{} remaining.",
            t.access.stage,
            t.access.remaining,
            if t.access.remaining == 1 { "" } else { "s" }
        );
    }
    match m.phase {
        Phase::Fetch => format!("Fetch the instruction at {}.", hex(m.pc)),
        Phase::Decode => "Decode the instruction and latch its register operands.".into(),
        Phase::Execute => {
            if let Some(md) = m
                .datapath
                .as_ref()
                .and_then(|d| d.alu.as_ref())
                .and_then(|a| a.muldiv.as_ref())
            {
                format!(
                    "{} clock {}/34: {}.",
                    if md.kind == "multiply" {
                        "Multiply"
                    } else {
                        "Divide"
                    },
                    md.execute_cycle + 1,
                    if md.iteration == 32 {
                        "apply signs and select the result"
                    } else {
                        "process the next bit"
                    }
                )
            } else {
                "Execute using the latched operands.".into()
            }
        }
        Phase::Memory => {
            if let Some(address) = m.effects.address {
                format!(
                    "Start the {} at {}.",
                    if m.effects.store.is_some() {
                        "store"
                    } else {
                        "load"
                    },
                    hex(address)
                )
            } else {
                "Pass through Memory; this instruction has no data access.".into()
            }
        }
        Phase::Writeback => {
            if let Some(rd) = m.effects.rd.filter(|r| *r != 0) {
                format!(
                    "Commit {} to {}; finish this instruction.",
                    number(m.effects.result),
                    reg(rd)
                )
            } else {
                "Finish this instruction and update the program counter.".into()
            }
        }
    }
}

pub(crate) fn describe(m: &Machine, b: &Before) -> Detail {
    if m.cycle == 0 {
        return Detail::initial(m.pc);
    }
    let mut d = Detail::initial(b.pc);
    d.before = Some(b.state.clone());
    d.after = Some(ClockState::capture(m));
    d.phase = Some(b.phase);
    d.inputs.clear();
    d.reason.clear();
    d.title.clear();
    d.instruction_cycle = m.cycle - m.history.back().map_or(m.cycle, |c| c.cycle);
    d.instruction = m
        .instruction
        .as_ref()
        .filter(|i| i.pc == b.pc)
        .map(|i| i.text.clone())
        .unwrap_or_else(|| format!("Fetching instruction at {}", hex(b.pc)));
    d.held = "PC and integer registers held; register write enable = 0.".into();
    d.next = next(m);
    d.components.clear();
    if let Some(fault) = &m.fault {
        d.title = "Execution stopped on this clock".into();
        d.reason = fault.clone();
        return d;
    }
    match b.phase {
        Phase::Fetch => {
            d.components.push("pc".into());
            memory(m, b, &mut d);
        }
        Phase::Memory if m.effects.address.is_some() => {
            d.components.push("memory".into());
            memory(m, b, &mut d);
        }
        Phase::Decode => {
            let p = m.datapath.as_ref().unwrap();
            d.title = "Decoded instruction; operands latched".into();
            d.reason=format!("The decoder selects {} and reads the source registers. These operand values stay latched while the instruction executes.",p.operation);
            d.components
                .extend(["decoder", "registers", "operand-a", "operand-b"].map(String::from));
            d.inspector = "decode".into();
            for (port, read) in [("Read port A", &p.read_a), ("Read port B", &p.read_b)] {
                if let Some(r) = read {
                    d.input(reg(r.register), number(r.value));
                    d.change(port, "not latched for this instruction", number(r.value));
                }
            }
            d.input(
                "ALU input A",
                format!("{} = {}", p.left_source, number(p.left)),
            );
            d.input(
                "ALU input B",
                format!("{} = {}", p.right_source, number(p.right)),
            );
            d.change(
                "Latched operands A / B",
                "not latched for this instruction",
                format!("{} / {}", hex(p.left), hex(p.right)),
            );
        }
        Phase::Execute => {
            let p = m.datapath.as_ref().unwrap();
            d.input(format!("A: {}", p.left_source), number(p.left));
            d.input(format!("B: {}", p.right_source), number(p.right));
            d.inspector = "alu".into();
            d.components
                .extend(["operand-a", "operand-b"].map(String::from));
            if let Some(md) = p.alu.as_ref().and_then(|a| a.muldiv.as_ref()) {
                d.components.push("muldiv".into());
                d.inspector = "muldiv".into();
                let name = if md.kind == "multiply" {
                    "Multiply"
                } else {
                    "Divide"
                };
                d.title = match md.stage.as_str() {
                    "prepare" => format!("{name}: prepare working registers"),
                    "finish" => format!("{name}: result ready for writeback"),
                    _ => format!("{name}: process bit {} of 32", md.iteration),
                };
                d.reason = md.description.clone();
                d.progress = Some(Progress {
                    name: format!("{name} execution"),
                    elapsed: md.execute_cycle,
                    total: 34,
                });
                for r in &md.registers {
                    d.input(&r.name, format!("0x{}", r.before));
                    d.change(&r.name, format!("0x{}", r.before), format!("0x{}", r.after));
                }
                if let Some(result) = md.result {
                    d.change("Writeback input", "not ready", number(result));
                }
            } else {
                d.components.push("alu".into());
                d.title = if p.memory_address.is_some() {
                    "Effective address computed".into()
                } else if p.branch_taken.is_some() {
                    format!(
                        "Branch {}",
                        if p.branch_taken == Some(true) {
                            "taken"
                        } else {
                            "not taken"
                        }
                    )
                } else if p.alu.is_some() {
                    "ALU result computed".into()
                } else {
                    "Control instruction prepared".into()
                };
                d.reason = m.effects.description.clone();
                if let Some(address) = p.memory_address {
                    d.change("Effective address", "not ready", hex(address));
                } else if let Some(value) = p.write_value {
                    d.change("Writeback input", "not ready", number(value));
                }
                if p.branch_taken.is_some() {
                    d.change(
                        "Next PC (pending)",
                        hex(b.pc.wrapping_add(4)),
                        hex(m.effects.next_pc),
                    );
                }
            }
            if let Some(rd) = p.rd.filter(|r| *r != 0) {
                d.held = format!(
                    "{} still holds {}. It changes only at Writeback.",
                    reg(rd),
                    number(m.regs[rd])
                );
            }
        }
        Phase::Memory => {
            d.title = "Memory phase: no data access".into();
            d.reason="This instruction does not load or store. The multicycle core still spends one clock in Memory before Writeback.".into();
            d.components.push("memory".into());
            d.inspector = "alu".into();
            if let Some(rd) = m.effects.rd.filter(|r| *r != 0) {
                d.input(
                    "Result waiting to commit",
                    format!("{} → {}", number(m.effects.result), reg(rd)),
                );
            }
        }
        Phase::Writeback => {
            d.title = if m.halted {
                format!("Program exited with code {}", m.exit_code.unwrap_or(0))
            } else {
                "Instruction completed".into()
            };
            d.reason="The clock edge commits the destination, if any, and advances the program counter. The next instruction can now begin.".into();
            d.components
                .extend(["pc", "write", "registers"].map(String::from));
            d.inspector = "decode".into();
            if m.events & EVENT_REGISTER != 0 {
                let rd = m.last_written.unwrap();
                d.title = format!("Wrote {} to {}", m.regs[rd], reg(rd));
                d.input("Write enable", "1");
                d.input("Write port destination", reg(rd));
                d.input("Write port data", number(m.effects.result));
                d.input("Previously stored value", number(b.regs[rd]));
                d.change(reg(rd), number(b.regs[rd]), number(m.regs[rd]));
                d.held = "Other integer registers retain their values.".into();
            } else {
                d.input("Write enable", "0");
                d.held = "No integer register write on this clock.".into();
            }
            d.input("Next PC", hex(m.pc));
            d.change("PC", hex(b.pc), hex(m.pc));
        }
    }
    d
}

fn memory(m: &Machine, b: &Before, d: &mut Detail) {
    let Some(a) = m.access.as_ref() else { return };
    let (stage, remaining) = b.stage.unwrap_or_else(|| {
        (
            if a.path.first().is_some_and(|p| p == "UART") {
                MemoryStage::Device
            } else {
                MemoryStage::Lookup(if b.phase == Phase::Fetch { 0 } else { 1 })
            },
            1,
        )
    });
    let (name, total, component, kind) = match stage {
        MemoryStage::Lookup(l) => (
            format!("{} lookup", m.caches[l].name),
            m.caches[l].latency,
            m.caches[l].name.clone(),
            "lookup",
        ),
        MemoryStage::RamRead => ("RAM read".into(), 60, "RAM".into(), "read"),
        MemoryStage::RamWrite => ("RAM write-through".into(), 60, "RAM".into(), "write"),
        MemoryStage::Fill(l) => (
            format!("{} refill", m.caches[l].name),
            1,
            m.caches[l].name.clone(),
            "fill",
        ),
        MemoryStage::Device => ("UART access".into(), 1, "UART".into(), "device"),
    };
    d.memory_clock = Some(MemoryClock {
        component: component.clone(),
        stage: name.clone(),
        stage_kind: kind.into(),
        remaining_before: remaining,
        remaining_after: remaining.saturating_sub(1),
        latency: total,
    });
    d.components.push(component);
    d.inspector = "cache".into();
    d.input(
        if b.phase == Phase::Fetch {
            "Instruction address"
        } else {
            "Data address"
        },
        format!("{} · {} bytes", hex(a.address), a.size),
    );
    if let Some(payload) = m.effects.store.filter(|_| a.kind == "store") {
        d.input("Store data", number(payload));
    }
    d.progress = Some(Progress {
        name: name.clone(),
        elapsed: total - remaining + 1,
        total,
    });
    if remaining > 1 {
        d.waiting = true;
        d.title = format!(
            "Waiting for {name}: {} clock{} remaining",
            remaining - 1,
            if remaining == 2 { "" } else { "s" }
        );
        d.reason=match stage {
            MemoryStage::RamRead=>"RAM is servicing the 64-byte line request. This clock only advances its modeled latency; no bytes return yet.".into(),
            MemoryStage::RamWrite=>"The store is waiting for RAM. RAM and resident cache bytes change together when the write-through finishes.".into(),
            _=>"This clock only advances the cache's modeled lookup latency. No tag-comparison or data-array transition is simulated on this clock; the hit or miss is reported when the countdown reaches zero.".into(),
        };
        return;
    }
    match stage {
        MemoryStage::Lookup(level) => {
            let Some(p) = a.probes.iter().rev().find(|p| p.level == level) else {
                return;
            };
            d.title = format!(
                "{} {} in set {}",
                p.cache,
                if p.hit { "hit" } else { "miss" },
                p.index
            );
            d.input("Requested tag", hex(p.tag));
            d.input(
                "Stored tag / valid",
                format!(
                    "{} / V={}",
                    p.stored_tag.map(hex).unwrap_or_else(|| "none".into()),
                    u8::from(p.valid)
                ),
            );
            d.reason = if p.hit {
                format!("The valid line has the requested tag. The selected {} bytes are available from {}.",a.size,p.cache)
            } else if !p.valid {
                "The indexed line is invalid. The request continues to the next memory level; the instruction remains stalled.".into()
            } else {
                "The stored tag differs from the requested tag. The request continues to the next memory level; the instruction remains stalled.".into()
            };
            let count = if p.hit {
                m.caches[level].hits
            } else {
                m.caches[level].misses
            };
            d.change(
                format!("{} {}", p.cache, if p.hit { "hits" } else { "misses" }),
                count.saturating_sub(1).to_string(),
                count.to_string(),
            );
        }
        MemoryStage::RamRead => {
            d.title = "RAM returned a 64-byte line".into();
            d.reason="The line is now in the refill buffer. Following clocks copy it into the caches, one level at a time.".into();
            d.change(
                "Refill buffer",
                "empty",
                format!("64 bytes from {}", hex(a.line)),
            );
        }
        MemoryStage::Fill(level) => {
            let c = &m.caches[level];
            d.title = format!("{} filled set {}", c.name, c.index(a.address));
            d.reason =
                "The cache's valid bit, tag, and all 64 data bytes are updated on this clock."
                    .into();
            d.change(
                format!("{} set {}", c.name, c.index(a.address)),
                b.old_line
                    .map(|a| format!("valid · {}", hex(a)))
                    .unwrap_or_else(|| "invalid".into()),
                format!("valid · {} · 64 bytes", hex(a.line)),
            );
        }
        MemoryStage::RamWrite => {
            d.title = "Store committed to RAM and resident caches".into();
            d.reason =
                "The write-through finished. Every resident copy now contains the new bytes."
                    .into();
            d.change(
                format!("Bytes at {}", hex(a.address)),
                bytes(&b.old_bytes),
                bytes(&m.memory(a.address, a.size)),
            );
        }
        MemoryStage::Device => {
            d.inspector = "ram".into();
            if a.kind == "store" {
                d.title = "UART transmitted one byte".into();
                d.reason="The program wrote the terminal device. Its output buffer gains this byte on this clock.".into();
                d.change("Terminal output", "", format!("0x{:02x}", a.value as u8));
            } else {
                d.title = "UART read completed".into();
                d.reason = "The device returned its input byte or input-ready flag.".into();
                d.change("Device result", "pending", number(a.value));
            }
        }
    }
    if a.complete && a.kind == "fetch" {
        d.components.push("instruction".into());
        d.change(
            "Instruction register",
            b.raw.map(hex).unwrap_or_else(|| "empty".into()),
            hex(a.value),
        );
        d.reason
            .push_str(" The instruction word is latched; Decode runs next.");
    } else if a.complete && a.kind == "load" {
        d.change(
            "Load result (pending writeback)",
            "not ready",
            number(m.effects.result),
        );
        d.reason
            .push_str(" The destination register still holds its old value until Writeback.");
    }
}

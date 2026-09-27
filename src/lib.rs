pub mod alu;
pub mod cache;
pub mod cycle;
pub mod datapath;
pub mod elf;
pub mod isa;
pub mod muldiv;
pub mod provenance;
pub mod timeline;

use cache::{Cache, Probe, LINE_SIZE};
use datapath::Datapath;
use elf::Symbol;
use isa::{decode, Instruction, Op};
use serde::Serialize;
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

pub const RAM_BASE: u32 = 0x8000_0000;
pub const RAM_SIZE: usize = 16 * 1024 * 1024;
pub const UART: u32 = 0x1000_0000;
pub const EVENT_REGISTER: u32 = 1;
pub const EVENT_MISS: u32 = 2;
pub const EVENT_FILL: u32 = 4;
pub const EVENT_ALU: u32 = 8;
pub const EVENT_CACHE_STAGE: u32 = 16;
pub const EVENT_DATA_STAGE: u32 = 32;
pub const EVENT_MULDIV: u32 = 64;
pub const EVENT_WATCH: u32 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Phase {
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}

#[derive(Clone, Serialize)]
pub struct MemoryStep {
    pub cycle: u64,
    pub level: Option<usize>,
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Serialize)]
pub struct Access {
    pub cycle: u64,
    pub pc: u32,
    pub instruction: String,
    pub address: u32,
    pub size: usize,
    pub kind: String,
    pub value: u32,
    pub path: Vec<String>,
    pub latency: u32,
    pub remaining: u32,
    pub line: u32,
    pub complete: bool,
    pub probes: Vec<Probe>,
    pub filled: Vec<usize>,
    pub finished_cycle: Option<u64>,
    pub stage: String,
    pub stage_kind: String,
    pub stage_level: Option<usize>,
    pub stage_latency: u32,
    pub pending_probe: Option<Probe>,
    pub steps: Vec<MemoryStep>,
    pub data_ready: bool,
    pub read_value: Option<u32>,
}

#[derive(Clone, Copy)]
enum MemoryStage {
    Lookup(usize),
    RamRead,
    Fill(usize),
    RamWrite,
    Device,
}

#[derive(Clone)]
struct Transaction {
    access: Access,
    source: Vec<u8>,
    top: usize,
    write: bool,
    stage: MemoryStage,
    returns: VecDeque<usize>,
}

#[derive(Clone, Default, Serialize)]
pub struct Effects {
    pub rd: Option<usize>,
    pub result: u32,
    pub address: Option<u32>,
    pub size: usize,
    pub store: Option<u32>,
    pub next_pc: u32,
    pub branch_taken: bool,
    pub left: u32,
    pub right: u32,
    pub alu_result: u32,
    pub description: String,
}

#[derive(Clone, Serialize)]
pub struct Log {
    pub cycle: u64,
    pub pc: u32,
    pub text: String,
    pub kind: String,
}

#[derive(Clone)]
struct Checkpoint {
    regs: [u32; 32],
    pc: u32,
    cycle: u64,
    retired: u64,
    caches: Vec<Cache>,
    console_len: usize,
    input: VecDeque<u8>,
    log: VecDeque<Log>,
    access: Option<Access>,
    instruction: Option<Instruction>,
    effects: Effects,
    last_written: Option<usize>,
    last_data: Option<u32>,
    undo: Vec<(usize, Vec<u8>)>,
    datapath: Option<Datapath>,
    cache_accesses: Vec<Option<Access>>,
    provenance: provenance::Checkpoint,
    input_cursor: usize,
}

#[derive(Serialize)]
pub struct Machine {
    pub regs: [u32; 32],
    pub pc: u32,
    pub cycle: u64,
    pub retired: u64,
    pub phase: Phase,
    pub instruction: Option<Instruction>,
    pub effects: Effects,
    pub caches: Vec<Cache>,
    pub access: Option<Access>,
    pub last_written: Option<usize>,
    pub last_data: Option<u32>,
    pub halted: bool,
    pub exit_code: Option<u32>,
    pub fault: Option<String>,
    pub symbols: Vec<Symbol>,
    pub log: VecDeque<Log>,
    pub datapath: Option<Datapath>,
    pub cache_accesses: Vec<Option<Access>>,
    #[serde(skip)]
    pub events: u32,
    #[serde(skip)]
    pub ram: Vec<u8>,
    #[serde(skip)]
    pub executable: Vec<(u32, u32)>,
    #[serde(skip)]
    pub console: Vec<u8>,
    #[serde(skip)]
    input: VecDeque<u8>,
    #[serde(skip)]
    transaction: Option<Transaction>,
    #[serde(skip)]
    history: VecDeque<Checkpoint>,
    #[serde(skip)]
    pub provenance: provenance::Provenance,
    #[serde(skip)]
    timeline: VecDeque<timeline::Sample>,
    #[serde(skip)]
    input_tape: Vec<timeline::InputEvent>,
    #[serde(skip)]
    input_cursor: usize,
}

impl Machine {
    pub fn empty() -> Self {
        Self {
            regs: [0; 32],
            pc: RAM_BASE,
            cycle: 0,
            retired: 0,
            phase: Phase::Fetch,
            instruction: None,
            effects: Effects::default(),
            access: None,
            last_written: None,
            last_data: None,
            halted: false,
            exit_code: None,
            fault: None,
            symbols: vec![],
            log: VecDeque::new(),
            datapath: None,
            cache_accesses: vec![None; 4],
            events: 0,
            ram: vec![0; RAM_SIZE],
            executable: vec![],
            console: vec![],
            input: VecDeque::new(),
            transaction: None,
            history: VecDeque::new(),
            provenance: provenance::Provenance::default(),
            timeline: VecDeque::new(),
            input_tape: vec![],
            input_cursor: 0,
            caches: vec![
                Cache::new("L1I", 8, 1),
                Cache::new("L1D", 8, 1),
                Cache::new("L2", 32, 6),
                Cache::new("L3", 128, 18),
            ],
        }
    }
    pub fn from_elf(bytes: &[u8]) -> Result<Self, String> {
        elf::load(bytes)
    }
    fn record(&mut self, kind: &str, text: String) {
        if self.log.len() == 128 {
            self.log.pop_front();
        }
        self.log.push_back(Log {
            cycle: self.cycle,
            pc: self.pc,
            kind: kind.into(),
            text,
        });
    }
    fn range(&self, address: u32, size: usize) -> Result<usize, String> {
        let offset = address
            .checked_sub(RAM_BASE)
            .ok_or_else(|| format!("Unmapped address 0x{address:08x}"))?
            as usize;
        if size > RAM_SIZE || offset > RAM_SIZE - size {
            return Err(format!("Access outside RAM: 0x{address:08x}"));
        }
        Ok(offset)
    }
    fn save(&mut self) {
        if self.history.len() == 128 {
            self.history.pop_front();
        }
        self.history.push_back(Checkpoint {
            regs: self.regs,
            pc: self.pc,
            cycle: self.cycle,
            retired: self.retired,
            caches: self.caches.clone(),
            console_len: self.console.len(),
            input: self.input.clone(),
            log: self.log.clone(),
            access: self.access.clone(),
            instruction: self.instruction.clone(),
            effects: self.effects.clone(),
            last_written: self.last_written,
            last_data: self.last_data,
            undo: vec![],
            datapath: self.datapath.clone(),
            cache_accesses: self.cache_accesses.clone(),
            provenance: self.provenance.checkpoint(),
            input_cursor: self.input_cursor,
        });
    }
    pub fn back(&mut self) -> bool {
        if !self.can_back() {
            return false;
        }
        self.restore_instruction()
    }
    fn can_back(&self) -> bool {
        self.history
            .back()
            .is_some_and(|c| c.cycle >= self.timeline_start())
    }
    fn restore_instruction(&mut self) -> bool {
        let Some(checkpoint) = self.history.pop_back() else {
            return false;
        };
        for (index, bytes) in checkpoint.undo.into_iter().rev() {
            self.ram[index..index + bytes.len()].copy_from_slice(&bytes);
        }
        self.regs = checkpoint.regs;
        self.pc = checkpoint.pc;
        self.cycle = checkpoint.cycle;
        self.retired = checkpoint.retired;
        self.caches = checkpoint.caches;
        self.console.truncate(checkpoint.console_len);
        self.input = checkpoint.input;
        self.log = checkpoint.log;
        self.access = checkpoint.access;
        self.instruction = checkpoint.instruction;
        self.effects = checkpoint.effects;
        self.last_written = checkpoint.last_written;
        self.last_data = checkpoint.last_data;
        self.datapath = checkpoint.datapath;
        self.cache_accesses = checkpoint.cache_accesses;
        self.provenance.restore(checkpoint.provenance);
        self.input_cursor = checkpoint.input_cursor;
        self.events = 0;
        self.phase = Phase::Fetch;
        self.transaction = None;
        self.halted = false;
        self.exit_code = None;
        self.fault = None;
        true
    }
    fn publish_access(&mut self, access: Access) {
        for probe in &access.probes {
            self.cache_accesses[probe.level] = Some(access.clone());
        }
        if let Some(probe) = &access.pending_probe {
            self.cache_accesses[probe.level] = Some(access.clone());
        }
        self.access = Some(access);
    }
    fn set_memory_stage(&self, tx: &mut Transaction, stage: MemoryStage) {
        tx.stage = stage;
        let (name, kind, level, duration) = match stage {
            MemoryStage::Lookup(level) => (
                format!("{} lookup", self.caches[level].name),
                "lookup",
                Some(level),
                self.caches[level].latency,
            ),
            MemoryStage::RamRead => ("RAM read".into(), "read", None, 60),
            MemoryStage::Fill(level) => (
                format!("{} refill", self.caches[level].name),
                "fill",
                Some(level),
                1,
            ),
            MemoryStage::RamWrite => ("RAM write-through".into(), "write", None, 60),
            MemoryStage::Device => ("UART access".into(), "device", None, 1),
        };
        tx.access.stage = name;
        tx.access.stage_kind = kind.into();
        tx.access.stage_level = level;
        tx.access.stage_latency = duration;
        tx.access.remaining = duration;
        tx.access.pending_probe = if let MemoryStage::Lookup(level) = stage {
            tx.access.path.push(self.caches[level].name.clone());
            Some(self.caches[level].inspect_probe(tx.access.address, level))
        } else {
            None
        };
        if matches!(stage, MemoryStage::RamRead) {
            tx.access.path.push("RAM".into());
        }
    }
    fn memory_event(
        &mut self,
        tx: &mut Transaction,
        kind: &str,
        level: Option<usize>,
        text: String,
    ) {
        self.events |= EVENT_CACHE_STAGE;
        if tx.access.kind != "fetch" {
            self.events |= EVENT_DATA_STAGE;
        }
        self.record(kind, text.clone());
        tx.access.steps.push(MemoryStep {
            cycle: self.cycle,
            level,
            kind: kind.into(),
            text,
        });
    }
    fn schedule_return(&self, tx: &mut Transaction) -> bool {
        if let Some(level) = tx.returns.pop_front() {
            self.set_memory_stage(tx, MemoryStage::Fill(level));
            true
        } else if tx.write {
            self.set_memory_stage(tx, MemoryStage::RamWrite);
            true
        } else {
            false
        }
    }
    fn read_transaction_value(tx: &mut Transaction) {
        let offset = (tx.access.address - tx.access.line) as usize;
        let mut value = 0;
        for i in 0..tx.access.size {
            value |= (tx.source[offset + i] as u32) << (i * 8);
        }
        tx.access.read_value = Some(value);
        if !tx.write {
            tx.access.value = value;
        }
        tx.access.data_ready = true;
    }
    fn start_access(
        &mut self,
        address: u32,
        size: usize,
        kind: &str,
        write: Option<u32>,
    ) -> Result<(), String> {
        if !address.is_multiple_of(size as u32) {
            return Err(format!(
                "Misaligned {kind} at 0x{address:08x} ({size} bytes)"
            ));
        }
        let device = (UART..UART + 12).contains(&address);
        if device {
            if kind == "fetch"
                || !matches!(
                    (address - UART, size, write.is_some()),
                    (0, 1, true) | (4, 1, false) | (8, 4, false)
                )
            {
                return Err(format!("Unsupported device access at 0x{address:08x}"));
            }
        } else {
            self.range(address, size)?;
            if kind == "fetch"
                && !self
                    .executable
                    .iter()
                    .any(|&(a, b)| address >= a && address + size as u32 <= b)
            {
                return Err(format!(
                    "Instruction fetch outside executable segment: 0x{address:08x}"
                ));
            }
        }
        let top = if kind == "fetch" { 0 } else { 1 };
        let access = Access {
            cycle: self.cycle,
            pc: self.pc,
            instruction: if kind == "fetch" {
                "instruction fetch".into()
            } else {
                self.instruction
                    .as_ref()
                    .map(|i| i.text.clone())
                    .unwrap_or_default()
            },
            address,
            size,
            kind: kind.into(),
            value: write.unwrap_or(0) & (u32::MAX >> ((4 - size) * 8)),
            path: if device { vec!["UART".into()] } else { vec![] },
            latency: 0,
            remaining: 0,
            line: address & !(LINE_SIZE as u32 - 1),
            complete: false,
            probes: vec![],
            filled: vec![],
            finished_cycle: None,
            stage: String::new(),
            stage_kind: String::new(),
            stage_level: None,
            stage_latency: 0,
            pending_probe: None,
            steps: vec![],
            data_ready: false,
            read_value: None,
        };
        let mut tx = Transaction {
            access,
            source: vec![],
            top,
            write: write.is_some(),
            stage: MemoryStage::Device,
            returns: VecDeque::new(),
        };
        self.set_memory_stage(
            &mut tx,
            if device {
                MemoryStage::Device
            } else {
                MemoryStage::Lookup(top)
            },
        );
        self.publish_access(tx.access.clone());
        self.transaction = Some(tx);
        Ok(())
    }
    fn advance_access(&mut self) -> Result<Option<u32>, String> {
        let mut tx = self
            .transaction
            .take()
            .ok_or("Missing memory transaction")?;
        tx.access.latency += 1;
        tx.access.remaining -= 1;
        if tx.access.remaining != 0 {
            self.publish_access(tx.access.clone());
            self.transaction = Some(tx);
            return Ok(None);
        }
        let address = tx.access.address;
        let mut continuing = false;
        match tx.stage {
            MemoryStage::Lookup(level) => {
                let mut probe = self.caches[level].probe(address, level);
                probe.cycle = Some(self.cycle);
                tx.access.pending_probe = None;
                let hit = probe.hit;
                let outcome = if hit { "hit" } else { "miss" };
                let message = format!(
                    "{} {} {outcome} · 0x{address:08x} · set {} · tag {:x} · stored {}",
                    probe.cache,
                    tx.access.kind,
                    probe.index,
                    probe.tag,
                    probe
                        .stored_tag
                        .map(|tag| format!("{tag:x}"))
                        .unwrap_or_else(|| "invalid".into())
                );
                tx.access.probes.push(probe);
                if !hit {
                    self.events |= EVENT_MISS;
                }
                self.memory_event(
                    &mut tx,
                    if hit { "cache_hit" } else { "cache_miss" },
                    Some(level),
                    message,
                );
                if hit {
                    let cache = &self.caches[level];
                    tx.source = cache.lines[cache.index(address)].data.clone();
                    Self::read_transaction_value(&mut tx);
                    tx.returns = tx
                        .access
                        .probes
                        .iter()
                        .rev()
                        .filter(|p| !p.hit)
                        .map(|p| p.level)
                        .collect();
                    continuing = self.schedule_return(&mut tx);
                } else {
                    let next = if level == 3 {
                        MemoryStage::RamRead
                    } else {
                        MemoryStage::Lookup(if level == tx.top { 2 } else { 3 })
                    };
                    self.set_memory_stage(&mut tx, next);
                    continuing = true;
                }
            }
            MemoryStage::RamRead => {
                let at = self.range(tx.access.line, LINE_SIZE)?;
                tx.source = self.ram[at..at + LINE_SIZE].to_vec();
                Self::read_transaction_value(&mut tx);
                let message = format!(
                    "RAM read · line 0x{:08x} · 64 bytes returned",
                    tx.access.line
                );
                self.memory_event(&mut tx, "ram_read", None, message);
                tx.returns = tx.access.probes.iter().rev().map(|p| p.level).collect();
                continuing = self.schedule_return(&mut tx);
            }
            MemoryStage::Fill(level) => {
                let cache = &self.caches[level];
                let old = &cache.lines[cache.index(address)];
                let replaced = old.valid.then_some(old.address);
                self.caches[level].fill(address, &tx.source);
                tx.access.filled.push(level);
                self.events |= EVENT_FILL;
                let message = format!(
                    "{} fill · line 0x{:08x}{}",
                    self.caches[level].name,
                    tx.access.line,
                    replaced
                        .map(|old| format!(" · evicted 0x{old:08x}"))
                        .unwrap_or_default()
                );
                self.memory_event(&mut tx, "cache_fill", Some(level), message);
                continuing = self.schedule_return(&mut tx);
            }
            MemoryStage::RamWrite => {
                let at = self.range(address, tx.access.size)?;
                if let Some(cp) = self.history.back_mut() {
                    cp.undo
                        .push((at, self.ram[at..at + tx.access.size].to_vec()));
                }
                let bytes = tx.access.value.to_le_bytes();
                self.ram[at..at + tx.access.size].copy_from_slice(&bytes[..tx.access.size]);
                for cache in &mut self.caches {
                    cache.update(address, &bytes[..tx.access.size]);
                }
                let message = format!(
                    "RAM write · 0x{address:08x} ← {:08x} · {} bytes",
                    tx.access.value, tx.access.size
                );
                self.memory_event(&mut tx, "ram_write", None, message);
            }
            MemoryStage::Device => {
                if tx.write {
                    if self.console.len() >= 65536 {
                        return Err(
                            "Console limit reached (64 KiB); reset to start a new session".into(),
                        );
                    }
                    self.console.push(tx.access.value as u8);
                    self.record(
                        "output",
                        format!("UART transmitted 0x{:02x}", tx.access.value),
                    );
                } else {
                    tx.access.value = if address == UART + 8 {
                        u32::from(!self.input.is_empty())
                    } else {
                        self.input.pop_front().unwrap_or(0) as u32
                    };
                }
                tx.access.data_ready = true;
            }
        }
        if continuing {
            self.publish_access(tx.access.clone());
            self.transaction = Some(tx);
            Ok(None)
        } else {
            tx.access.complete = true;
            tx.access.finished_cycle = Some(self.cycle);
            tx.access.remaining = 0;
            tx.access.stage = "Complete".into();
            tx.access.stage_kind = "complete".into();
            tx.access.stage_level = None;
            let value = tx.access.value;
            if tx.write {
                self.trace_store(&tx.access, matches!(tx.stage, MemoryStage::Device));
            }
            self.publish_access(tx.access);
            Ok(Some(value))
        }
    }
    pub fn tick(&mut self) {
        self.events = 0;
        if self.halted {
            return;
        }
        self.replay_inputs();
        // Input older than every retained checkpoint is no longer replayable.
        let old = self
            .history
            .front()
            .map_or(self.input_cursor, |c| c.input_cursor);
        if old > 0 {
            self.input_tape.drain(..old);
            self.input_cursor -= old;
            for checkpoint in &mut self.history {
                checkpoint.input_cursor -= old;
            }
        }
        let before = cycle::Before::capture(self);
        if self.timeline.is_empty() {
            self.sample_cycle(&before);
        }
        if self.phase == Phase::Fetch && self.transaction.is_none() {
            self.save();
        }
        self.cycle += 1;
        if let Err(error) = self.tick_inner() {
            self.record("fault", error.clone());
            self.fault = Some(error);
            self.halted = true;
        }
        self.regs[0] = 0;
        self.sample_cycle(&before);
    }
    fn tick_inner(&mut self) -> Result<(), String> {
        match self.phase {
            Phase::Fetch => {
                if self.transaction.is_none() {
                    self.last_written = None;
                    self.start_access(self.pc, 4, "fetch", None)?;
                }
                if let Some(raw) = self.advance_access()? {
                    self.instruction = Some(decode(self.pc, raw)?);
                    self.phase = Phase::Decode;
                }
            }
            Phase::Decode => {
                self.effects = Effects::default();
                let mut d = Datapath::decode(
                    self.instruction.as_ref().ok_or("Missing instruction")?,
                    &self.regs,
                );
                d.source_a = d.read_a.as_ref().map(|p| {
                    self.provenance
                        .register_source(p.register, p.value, self.cycle)
                });
                d.source_b = d.read_b.as_ref().map(|p| {
                    self.provenance
                        .register_source(p.register, p.value, self.cycle)
                });
                self.datapath = Some(d);
                self.phase = Phase::Execute;
            }
            Phase::Execute => {
                if self.execute()? {
                    self.phase = Phase::Memory;
                }
            }
            Phase::Memory => {
                if let Some(address) = self.effects.address {
                    if self.transaction.is_none() {
                        self.last_data = Some(address);
                        self.start_access(
                            address,
                            self.effects.size,
                            if self.effects.store.is_some() {
                                "store"
                            } else {
                                "load"
                            },
                            self.effects.store,
                        )?;
                    }
                    if let Some(value) = self.advance_access()? {
                        use Op::*;
                        self.effects.result = match self.instruction.as_ref().unwrap().op {
                            Lb => (value as u8 as i8 as i32) as u32,
                            Lh => (value as u16 as i16 as i32) as u32,
                            _ => value,
                        };
                        if let Some(d) = &mut self.datapath {
                            if d.write_source == "memory" {
                                d.write_value = Some(self.effects.result);
                            }
                        }
                        self.phase = Phase::Writeback;
                    }
                } else {
                    self.phase = Phase::Writeback;
                }
            }
            Phase::Writeback => {
                if let Some(rd) = self.effects.rd {
                    if rd != 0 {
                        let before = self.regs[rd];
                        self.trace_register_write(rd, self.effects.result);
                        self.regs[rd] = self.effects.result;
                        self.last_written = Some(rd);
                        self.events |= EVENT_REGISTER;
                        self.record(
                            "register_write",
                            format!(
                                "x{rd} / {} · {before:08x} → {:08x}",
                                isa::REG_NAMES[rd],
                                self.effects.result
                            ),
                        );
                    }
                }
                if let Some(d) = &mut self.datapath {
                    d.committed = true;
                }
                let op = self.instruction.as_ref().unwrap().op;
                if op == Op::Ecall {
                    if self.regs[17] != 93 {
                        return Err(format!(
                            "Unsupported environment call {} (exit is 93)",
                            self.regs[17]
                        ));
                    }
                    self.exit_code = Some(self.regs[10]);
                    self.halted = true;
                    self.record(
                        "exit",
                        format!("Program exited with code {}", self.regs[10]),
                    );
                }
                if op == Op::Ebreak {
                    return Err("Guest breakpoint instruction (EBREAK)".into());
                }
                self.pc = self.effects.next_pc;
                self.retired += 1;
                self.phase = Phase::Fetch;
            }
        }
        Ok(())
    }
    fn execute(&mut self) -> Result<bool, String> {
        use Op::*;
        {
            let d = self.datapath.as_mut().ok_or("Missing decoded datapath")?;
            if let Some(trace) = &mut d.alu {
                trace.advance();
            } else {
                d.alu = d
                    .function
                    .map(|function| alu::start(function, d.left, d.right));
            }
        }
        let trace = self.datapath.as_ref().and_then(|d| d.alu.as_ref());
        let ready = trace.is_none_or(|a| a.ready);
        if let Some(m) = trace.and_then(|a| a.muldiv.as_ref()) {
            let description = format!(
                "{:?} · execute {}/34 · {}",
                m.operation, m.execute_cycle, m.description
            );
            self.events |= EVENT_MULDIV;
            self.record("muldiv_step", description);
        }
        if !ready {
            return Ok(false);
        }
        let i = self.instruction.as_ref().ok_or("Missing instruction")?;
        let d = self.datapath.as_mut().ok_or("Missing decoded datapath")?;
        let result = d.alu.as_ref().map_or(0, |trace| trace.result);
        let mut e = Effects {
            rd: d.rd,
            result,
            left: d.left,
            right: d.right,
            alu_result: result,
            next_pc: self.pc.wrapping_add(4),
            ..Effects::default()
        };
        match i.op {
            Jal | Jalr => {
                e.next_pc = if i.op == Jalr { result & !1 } else { result };
                e.branch_taken = true;
                e.result = self.pc.wrapping_add(4);
                d.branch_taken = Some(true);
            }
            Beq | Bne | Blt | Bge | Bltu | Bgeu => {
                e.branch_taken = result != 0;
                d.branch_taken = Some(e.branch_taken);
                if e.branch_taken {
                    e.next_pc = self.pc.wrapping_add(i.imm as u32);
                }
            }
            Lb | Lh | Lw | Lbu | Lhu | Sb | Sh | Sw => {
                e.address = Some(result);
                e.size = match i.op {
                    Lb | Lbu | Sb => 1,
                    Lh | Lhu | Sh => 2,
                    _ => 4,
                };
                e.store = d.store_data;
                d.memory_address = e.address;
            }
            _ => {}
        }
        d.next_pc = Some(e.next_pc);
        if d.rd.is_some() && d.write_source != "memory" {
            d.write_value = Some(e.result);
        }
        e.description = if let Some(address) = e.address {
            format!(
                "{} address · {:08x} + {} → {address:08x}",
                if e.store.is_some() { "store" } else { "load" },
                d.left,
                i.imm
            )
        } else if let Some(taken) = d.branch_taken {
            format!(
                "{} · {} · next {:08x}",
                i.text,
                if taken { "taken" } else { "not taken" },
                e.next_pc
            )
        } else if d.alu.is_some() {
            format!(
                "ALU {} · 0x{:08x}, 0x{:08x} → 0x{:08x}",
                d.operation, d.left, d.right, result
            )
        } else {
            i.text.clone()
        };
        self.effects = e;
        if self.effects.next_pc & 3 != 0 {
            return Err(format!(
                "Misaligned instruction target 0x{:08x}",
                self.effects.next_pc
            ));
        }
        if d.alu.is_some() {
            self.events |= EVENT_ALU;
        }
        Ok(true)
    }
    pub fn step(&mut self) {
        let before = self.retired;
        while !self.halted && self.retired == before {
            self.tick();
        }
    }
    pub fn input(&mut self, bytes: &[u8]) {
        // New external input forks execution at the selected cycle. Recorded
        // input is replayed automatically when simply moving through history.
        self.timeline.retain(|s| s.cycle <= self.cycle);
        self.input_tape.truncate(self.input_cursor);
        self.input_tape.push(timeline::InputEvent {
            cycle: self.cycle,
            bytes: bytes
                .iter()
                .copied()
                .take(4096usize.saturating_sub(self.input.len()))
                .collect(),
        });
        self.replay_inputs();
    }
    pub fn snapshot(&self) -> String {
        let mut value = serde_json::to_value(self).unwrap();
        value["console"] = String::from_utf8_lossy(&self.console).into_owned().into();
        value["history_depth"] = self.history.len().into();
        value["can_back"] = self.can_back().into();
        value["input_pending"] = self.input.len().into();
        value["timeline_start"] = self.timeline_start().into();
        value["timeline_end"] = self.timeline_end().into();
        value["register_versions"] = serde_json::to_value(self.provenance.registers).unwrap();
        value["cycle_detail"] = serde_json::to_value(
            self.timeline
                .iter()
                .rev()
                .find(|s| s.cycle == self.cycle)
                .map(|s| s.detail.clone())
                .unwrap_or_else(|| cycle::Detail::initial(self.pc)),
        )
        .unwrap();
        serde_json::to_string(&value).unwrap()
    }
    pub fn memory(&self, address: u32, size: usize) -> Vec<u8> {
        self.range(address, size.min(4096))
            .map(|i| self.ram[i..i + size.min(4096)].to_vec())
            .unwrap_or_default()
    }
    pub fn program(&self) -> String {
        let mut rows = vec![];
        for &(start, end) in &self.executable {
            for address in (start..end.saturating_sub(3))
                .step_by(4)
                .take(8192usize.saturating_sub(rows.len()))
            {
                let at = (address - RAM_BASE) as usize;
                let raw = u32::from_le_bytes(self.ram[at..at + 4].try_into().unwrap());
                let text = decode(address, raw)
                    .map(|i| i.text)
                    .unwrap_or_else(|_| format!(".word 0x{raw:08x}"));
                let symbol = self
                    .symbols
                    .iter()
                    .find(|s| s.address == address && s.kind == 2)
                    .map(|s| s.name.as_str())
                    .unwrap_or("");
                rows.push(
                    serde_json::json!({"address":address,"raw":raw,"text":text,"symbol":symbol}),
                );
            }
        }
        serde_json::to_string(&rows).unwrap()
    }
}

#[wasm_bindgen]
pub struct Simulator {
    machine: Machine,
    binary: Vec<u8>,
    stop_reason: String,
    watch_address: Option<u32>,
    watch_size: u32,
    watch_roots: Vec<u64>,
}

#[wasm_bindgen]
impl Simulator {
    #[wasm_bindgen(constructor)]
    pub fn new(binary: &[u8]) -> Result<Simulator, String> {
        Ok(Self {
            machine: Machine::from_elf(binary)?,
            binary: binary.to_vec(),
            stop_reason: String::new(),
            watch_address: None,
            watch_size: 4,
            watch_roots: vec![],
        })
    }
    pub fn tick(&mut self, count: u32) {
        for _ in 0..count.min(100_000) {
            self.machine.tick();
            if self.machine.halted {
                break;
            }
        }
    }
    pub fn run(&mut self, count: u32, breakpoints: &[u32], ignore_first: bool) -> u32 {
        self.run_until(count, breakpoints, ignore_first, 0)
    }
    pub fn run_until(
        &mut self,
        count: u32,
        breakpoints: &[u32],
        ignore_first: bool,
        stop_mask: u32,
    ) -> u32 {
        self.stop_reason.clear();
        let mut ran = 0;
        for i in 0..count.min(100_000) {
            if self.machine.halted {
                self.stop_reason = if self.machine.fault.is_some() {
                    "fault"
                } else {
                    "exit"
                }
                .into();
                break;
            }
            if self.machine.phase == Phase::Fetch
                && self.machine.transaction.is_none()
                && breakpoints.contains(&self.machine.pc)
                && !(i == 0 && ignore_first)
            {
                self.stop_reason = "breakpoint".into();
                break;
            }
            self.machine.tick();
            ran += 1;
            let mut events = self.machine.events & stop_mask;
            if stop_mask & EVENT_WATCH != 0 && self.watched_event() {
                events |= EVENT_WATCH;
            }
            if events != 0 {
                self.stop_reason = if events & EVENT_REGISTER != 0 {
                    "register write"
                } else if events & EVENT_MISS != 0 {
                    "cache miss"
                } else if events & EVENT_FILL != 0 {
                    "cache fill"
                } else if events & EVENT_ALU != 0 {
                    "ALU result"
                } else if events & EVENT_MULDIV != 0 {
                    "multiply/divide cycle"
                } else if events & EVENT_WATCH != 0 {
                    "watched value"
                } else {
                    "cache stage"
                }
                .into();
                break;
            }
        }
        ran
    }
    pub fn stop_reason(&self) -> String {
        self.stop_reason.clone()
    }
    pub fn step(&mut self) {
        self.machine.step();
    }
    pub fn back(&mut self) -> bool {
        self.machine.back()
    }
    pub fn reset(&mut self) -> Result<(), String> {
        self.machine = Machine::from_elf(&self.binary)?;
        self.watch_roots.clear();
        Ok(())
    }
    pub fn input(&mut self, text: &str) {
        self.machine.input(text.as_bytes());
        self.watch_roots.clear();
    }
    pub fn snapshot(&self) -> String {
        self.machine.snapshot()
    }
    pub fn memory(&self, address: u32, size: u32) -> Vec<u8> {
        self.machine.memory(address, size as usize)
    }
    pub fn output_bytes(&self) -> Vec<u8> {
        self.machine.console.clone()
    }
    pub fn program(&self) -> String {
        self.machine.program()
    }
    pub fn back_cycle(&mut self) -> bool {
        self.machine.back_cycle()
    }
    pub fn seek(&mut self, cycle: f64) -> bool {
        cycle.is_finite()
            && cycle >= 0.0
            && cycle.fract() == 0.0
            && self.machine.seek_cycle(cycle as u64)
    }
    pub fn waves(&self, register: u32, span: u32) -> String {
        self.machine.wave_window(register as usize, span)
    }
    pub fn trace(&self, kind: &str, target: u32, size: u32) -> String {
        self.machine.provenance.graph(kind, target, size)
    }
    pub fn watch(&mut self, address: u32, size: u32) {
        self.watch_address = Some(address);
        self.watch_size = size.clamp(1, 4);
        self.watch_roots.clear();
    }
}

impl Simulator {
    fn watched_event(&mut self) -> bool {
        let Some(address) = self.watch_address else {
            return false;
        };
        let m = &self.machine;
        let touches = |a: u32, size: usize| {
            u64::from(a) < u64::from(address) + u64::from(self.watch_size)
                && u64::from(address) < u64::from(a) + size as u64
        };
        let access = m.access.as_ref().filter(|a| a.kind != "fetch");
        if self.watch_roots.is_empty() {
            self.watch_roots = m.provenance.memory_roots(address, self.watch_size);
        }
        let roots = &self.watch_roots;
        if access.is_some_and(|a| touches(a.address, a.size)) && m.events & EVENT_DATA_STAGE != 0 {
            return true;
        }
        let Some(d) = &m.datapath else {
            return false;
        };
        let related = d.memory_address.is_some_and(|a| touches(a, m.effects.size))
            || m.provenance.depends_on(d.source_a, roots)
            || m.provenance.depends_on(d.source_b, roots);
        related && m.events & (EVENT_REGISTER | EVENT_ALU | EVENT_MULDIV | EVENT_DATA_STAGE) != 0
    }
}

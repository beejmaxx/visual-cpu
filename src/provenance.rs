//! Versioned data dependencies. Values are linked by writes, never by equality.
use crate::{Access, Machine, RAM_BASE, RAM_SIZE};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const NODE_LIMIT: usize = 8192;
const MEMORY_LIMIT: usize = 16384;

#[derive(Clone, Serialize)]
pub struct Parent {
    pub id: u64,
    pub role: String,
}

#[derive(Clone, Serialize)]
pub struct Node {
    pub id: u64,
    pub cycle: u64,
    pub pc: u32,
    pub kind: String,
    pub label: String,
    pub value: u32,
    pub register: Option<usize>,
    pub address: Option<u32>,
    pub size: usize,
    pub parents: Vec<Parent>,
    pub access: Option<Access>,
}

#[derive(Clone)]
pub struct Checkpoint {
    next_id: u64,
    registers: [Option<u64>; 32],
    output_len: usize,
    overflow: bool,
    pub undo: Vec<(u32, Option<u64>)>,
}

pub struct Provenance {
    pub registers: [Option<u64>; 32],
    pub nodes: VecDeque<Node>,
    pub outputs: Vec<u64>,
    memory: BTreeMap<u32, u64>,
    next_id: u64,
    overflow: bool,
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            registers: [None; 32],
            nodes: VecDeque::new(),
            outputs: vec![],
            memory: BTreeMap::new(),
            next_id: 1,
            overflow: false,
        }
    }
}

impl Provenance {
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            next_id: self.next_id,
            registers: self.registers,
            output_len: self.outputs.len(),
            overflow: self.overflow,
            undo: vec![],
        }
    }
    pub fn restore(&mut self, checkpoint: Checkpoint) {
        for (address, previous) in checkpoint.undo.into_iter().rev() {
            if let Some(id) = previous {
                self.memory.insert(address, id);
            } else {
                self.memory.remove(&address);
            }
        }
        self.nodes.retain(|n| n.id < checkpoint.next_id);
        self.next_id = checkpoint.next_id;
        self.registers = checkpoint.registers;
        self.outputs.truncate(checkpoint.output_len);
        self.overflow = checkpoint.overflow;
    }
    fn push(&mut self, mut node: Node) -> u64 {
        node.id = self.next_id;
        self.next_id += 1;
        let id = node.id;
        self.nodes.push_back(node);
        if self.nodes.len() > NODE_LIMIT {
            self.nodes.pop_front();
        }
        id
    }
    pub fn register_source(&mut self, register: usize, value: u32, cycle: u64) -> u64 {
        if let Some(id) = self.registers[register] {
            return id;
        }
        let id = self.push(Node {
            id: 0,
            cycle,
            pc: 0,
            kind: "initial_register".into(),
            label: format!("x{register} initial value"),
            value,
            register: Some(register),
            address: None,
            size: 4,
            parents: vec![],
            access: None,
        });
        self.registers[register] = Some(id);
        id
    }
    fn put_memory(&mut self, address: u32, id: u64, undo: &mut Vec<(u32, Option<u64>)>) {
        if self.memory.len() >= MEMORY_LIMIT && !self.memory.contains_key(&address) {
            self.overflow = true;
            return;
        }
        undo.push((address, self.memory.insert(address, id)));
    }
    pub fn memory_roots(&self, address: u32, size: u32) -> Vec<u64> {
        let mut roots = BTreeSet::new();
        for offset in 0..size.clamp(1, 4) {
            if let Some(id) = self.memory.get(&address.saturating_add(offset)) {
                roots.insert(*id);
            }
        }
        roots.into_iter().collect()
    }
    pub fn depends_on(&self, id: Option<u64>, roots: &[u64]) -> bool {
        let mut pending: Vec<_> = id.into_iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if roots.contains(&id) {
                return true;
            }
            if !seen.insert(id) {
                continue;
            }
            if let Some(node) = self.nodes.iter().rev().find(|n| n.id == id) {
                pending.extend(
                    node.parents
                        .iter()
                        .filter(|p| p.role != "address")
                        .map(|p| p.id),
                );
            }
        }
        false
    }
    pub fn graph(&self, kind: &str, target: u32, size: u32) -> String {
        let roots: Vec<u64> = match kind {
            "register" => self
                .registers
                .get(target as usize)
                .copied()
                .flatten()
                .into_iter()
                .collect(),
            "output" => self
                .outputs
                .get(target as usize)
                .copied()
                .into_iter()
                .collect(),
            "node" => vec![u64::from(target)],
            _ => self.memory_roots(target, size),
        };
        let mut descendants: BTreeSet<_> = roots.iter().copied().collect();
        for node in &self.nodes {
            if descendants.len() >= 96 {
                break;
            }
            if node
                .parents
                .iter()
                .any(|p| p.role != "address" && descendants.contains(&p.id))
            {
                descendants.insert(node.id);
            }
        }
        let mut picked = descendants.clone();
        let mut pending: Vec<_> = picked.iter().copied().collect();
        while let Some(id) = pending.pop() {
            if picked.len() >= 128 {
                break;
            }
            if let Some(node) = self.nodes.iter().rev().find(|n| n.id == id) {
                for p in &node.parents {
                    if picked.insert(p.id) {
                        pending.push(p.id);
                    }
                }
            }
        }
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| picked.contains(&n.id))
            .collect();
        let registers: Vec<_> = self
            .registers
            .iter()
            .enumerate()
            .filter_map(|(i, id)| id.filter(|id| descendants.contains(id)).map(|_| i))
            .collect();
        serde_json::json!({"roots":roots,"nodes":nodes,"descendants":descendants,"registers":registers,"missing":picked.len().saturating_sub(nodes.len()),"memory_limit_reached":self.overflow}).to_string()
    }
}

fn parent(id: Option<u64>, role: &str) -> Vec<Parent> {
    id.map(|id| Parent {
        id,
        role: role.into(),
    })
    .into_iter()
    .collect()
}

impl Machine {
    fn memory_sources(&mut self, address: u32, size: usize, value: u32) -> Vec<Parent> {
        let mut result = vec![];
        let mut offset = 0;
        while offset < size {
            let at = address + offset as u32;
            if let Some(id) = self.provenance.memory.get(&at).copied() {
                result.push(Parent {
                    id,
                    role: format!("memory byte +{offset}"),
                });
                offset += 1;
                continue;
            }
            let start = offset;
            offset += 1;
            while offset < size
                && !self
                    .provenance
                    .memory
                    .contains_key(&(address + offset as u32))
            {
                offset += 1;
            }
            let bytes = offset - start;
            let data = (value >> (start * 8)) & (u32::MAX >> ((4 - bytes) * 8));
            let id = self.provenance.push(Node {
                id: 0,
                cycle: self.cycle,
                pc: 0,
                kind: if self.provenance.overflow {
                    "untracked_memory"
                } else {
                    "initial_memory"
                }
                .into(),
                label: if self.provenance.overflow {
                    "RAM · earlier source not retained"
                } else {
                    "RAM · initial program data"
                }
                .into(),
                value: data,
                register: None,
                address: Some(at),
                size: bytes,
                parents: vec![],
                access: None,
            });
            if let Some(cp) = self.history.back_mut() {
                for index in start..offset {
                    self.provenance
                        .put_memory(address + index as u32, id, &mut cp.provenance.undo);
                }
            }
            result.push(Parent {
                id,
                role: format!("memory bytes +{start}..+{}", offset - 1),
            });
        }
        result.dedup_by_key(|p| p.id);
        result
    }
    pub(crate) fn trace_register_write(&mut self, register: usize, value: u32) {
        let d = self.datapath.as_ref().unwrap();
        let load = d.write_source == "memory";
        let address = d.memory_address;
        let label = d.instruction.clone();
        let mut parents = parent(d.source_a, if load { "address" } else { "operand A" });
        parents.extend(parent(d.source_b, "operand B"));
        let access = if load { self.access.clone() } else { None };
        if let Some(a) = &access {
            if a.address >= RAM_BASE && a.address < RAM_BASE + RAM_SIZE as u32 {
                parents.extend(self.memory_sources(a.address, a.size, a.value));
            }
        }
        let id = self.provenance.push(Node {
            id: 0,
            cycle: self.cycle,
            pc: self.pc,
            kind: if load { "load" } else { "register" }.into(),
            label,
            value,
            register: Some(register),
            address,
            size: access.as_ref().map_or(4, |a| a.size),
            parents,
            access,
        });
        self.provenance.registers[register] = Some(id);
    }
    pub(crate) fn trace_store(&mut self, access: &Access, output: bool) {
        let d = self.datapath.as_ref().unwrap();
        let mut parents = parent(d.source_b, "stored value");
        parents.extend(parent(d.source_a, "address"));
        let id = self.provenance.push(Node {
            id: 0,
            cycle: self.cycle,
            pc: self.pc,
            kind: if output { "output" } else { "store" }.into(),
            label: d.instruction.clone(),
            value: access.value & (u32::MAX >> ((4 - access.size) * 8)),
            register: None,
            address: Some(access.address),
            size: access.size,
            parents,
            access: Some(access.clone()),
        });
        if output {
            self.provenance.outputs.push(id);
        } else if let Some(cp) = self.history.back_mut() {
            for offset in 0..access.size {
                self.provenance.put_memory(
                    access.address + offset as u32,
                    id,
                    &mut cp.provenance.undo,
                );
            }
        }
    }
}

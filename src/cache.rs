use serde::Serialize;

pub const LINE_SIZE: usize = 64;

#[derive(Clone, Serialize)]
pub struct Probe {
    pub cycle: Option<u64>,
    pub level: usize,
    pub cache: String,
    pub index: usize,
    pub index_bits: u32,
    pub tag: u32,
    pub offset: u32,
    pub valid: bool,
    pub stored_tag: Option<u32>,
    pub stored_address: Option<u32>,
    pub hit: bool,
}

#[derive(Clone, Serialize)]
pub struct Line {
    pub valid: bool,
    pub address: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Serialize)]
pub struct Cache {
    pub name: String,
    pub latency: u32,
    pub hits: u64,
    pub misses: u64,
    pub fills: u64,
    pub evictions: u64,
    pub lines: Vec<Line>,
}

impl Cache {
    pub fn new(name: &str, lines: usize, latency: u32) -> Self {
        Self {
            name: name.into(),
            latency,
            hits: 0,
            misses: 0,
            fills: 0,
            evictions: 0,
            lines: vec![
                Line {
                    valid: false,
                    address: 0,
                    data: vec![0; LINE_SIZE]
                };
                lines
            ],
        }
    }
    pub fn index(&self, address: u32) -> usize {
        address as usize / LINE_SIZE % self.lines.len()
    }
    pub fn contains(&self, address: u32) -> bool {
        let line = &self.lines[self.index(address)];
        line.valid && line.address == address & !(LINE_SIZE as u32 - 1)
    }
    pub fn lookup(&mut self, address: u32) -> bool {
        let hit = self.contains(address);
        if hit {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        hit
    }
    pub fn inspect_probe(&self, address: u32, level: usize) -> Probe {
        let index = self.index(address);
        let index_bits = self.lines.len().ilog2();
        let line = &self.lines[index];
        let valid = line.valid;
        let stored_tag = valid.then_some(line.address >> (6 + index_bits));
        let stored_address = valid.then_some(line.address);
        let hit = self.contains(address);
        Probe {
            cycle: None,
            level,
            cache: self.name.clone(),
            index,
            index_bits,
            tag: address >> (6 + index_bits),
            offset: address & 63,
            valid,
            stored_tag,
            stored_address,
            hit,
        }
    }
    pub fn probe(&mut self, address: u32, level: usize) -> Probe {
        let probe = self.inspect_probe(address, level);
        self.lookup(address);
        probe
    }
    pub fn fill(&mut self, address: u32, bytes: &[u8]) {
        if self.contains(address) {
            return;
        }
        let index = self.index(address);
        if self.lines[index].valid {
            self.evictions += 1;
        }
        self.lines[index] = Line {
            valid: true,
            address: address & !(LINE_SIZE as u32 - 1),
            data: bytes.to_vec(),
        };
        self.fills += 1;
    }
    pub fn update(&mut self, address: u32, bytes: &[u8]) {
        if self.contains(address) {
            let index = self.index(address);
            let offset = address as usize % LINE_SIZE;
            self.lines[index].data[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
    }
}

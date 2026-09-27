//! Compact clock samples plus instruction checkpoints for exact cycle replay.
use crate::{Machine, Phase, EVENT_REGISTER};
use serde::Serialize;

pub const SAMPLE_LIMIT: usize = 4096;

#[derive(Clone)]
pub struct InputEvent {
    pub cycle: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Serialize)]
pub struct Sample {
    pub cycle: u64,
    pub pc: u32,
    pub phase: Phase,
    pub retired: u64,
    pub write_register: Option<usize>,
    pub d_register: Option<usize>,
    pub d: Option<u32>,
    pub alu_busy: bool,
    pub memory_request: bool,
    pub memory_ready: bool,
    pub memory_address: Option<u32>,
    pub memory_stage: String,
    pub events: u32,
    #[serde(skip)]
    pub regs: [u32; 32],
    #[serde(skip)]
    pub detail: crate::cycle::Detail,
}

impl Machine {
    pub(crate) fn sample_cycle(&mut self, before: &crate::cycle::Before) {
        // Replaying recorded cycles does not duplicate the retained waveform.
        if self.timeline.back().is_some_and(|s| s.cycle >= self.cycle) {
            return;
        }
        let d = self.datapath.as_ref();
        let written = if self.events & EVENT_REGISTER != 0 {
            self.last_written
        } else {
            None
        };
        let valid_d = d.filter(|d| d.write_enable && (!d.committed || written.is_some()));
        let access = self.access.as_ref();
        let sample = Sample {
            cycle: self.cycle,
            pc: before.pc,
            phase: before.phase,
            retired: self.retired,
            write_register: written,
            d_register: valid_d.and_then(|d| d.rd),
            d: valid_d.and_then(|d| d.write_value),
            alu_busy: d.and_then(|d| d.alu.as_ref()).is_some_and(|a| !a.ready),
            memory_request: access.is_some_and(|a| !a.complete),
            memory_ready: access.is_some_and(|a| a.finished_cycle == Some(self.cycle)),
            memory_address: access
                .filter(|a| !a.complete || a.finished_cycle == Some(self.cycle))
                .map(|a| a.address),
            memory_stage: access
                .filter(|a| !a.complete)
                .map(|a| a.stage.clone())
                .unwrap_or_default(),
            events: self.events,
            regs: self.regs,
            detail: crate::cycle::describe(self, before),
        };
        self.timeline.push_back(sample);
        if self.timeline.len() > SAMPLE_LIMIT {
            self.timeline.pop_front();
        }
    }

    pub fn timeline_start(&self) -> u64 {
        let checkpoint = self.history.front().map_or(self.cycle, |c| c.cycle);
        checkpoint.max(self.timeline.front().map_or(0, |s| s.cycle))
    }

    pub fn timeline_end(&self) -> u64 {
        self.timeline.back().map_or(self.cycle, |s| s.cycle)
    }

    pub fn seek_cycle(&mut self, target: u64) -> bool {
        if target < self.timeline_start() || target > self.timeline_end() {
            return false;
        }
        while self.cycle > target {
            if !self.restore_instruction() {
                return false;
            }
        }
        while self.cycle < target && !self.halted {
            self.tick();
        }
        self.replay_inputs();
        self.cycle == target
    }

    pub fn back_cycle(&mut self) -> bool {
        self.cycle
            .checked_sub(1)
            .is_some_and(|target| self.seek_cycle(target))
    }

    pub(crate) fn replay_inputs(&mut self) {
        while let Some(event) = self.input_tape.get(self.input_cursor) {
            if event.cycle > self.cycle {
                break;
            }
            self.input.extend(
                event
                    .bytes
                    .iter()
                    .copied()
                    .take(4096usize.saturating_sub(self.input.len())),
            );
            self.input_cursor += 1;
        }
    }

    pub fn wave_window(&self, register: usize, span: u32) -> String {
        let span = u64::from(span.clamp(8, 128));
        let min = self.timeline_start();
        let max = self.timeline_end();
        let end = max.min(self.cycle.saturating_add(span / 2));
        let start = min.max(end.saturating_sub(span - 1));
        let register = register.min(31);
        let samples: Vec<_> = self
            .timeline
            .iter()
            .filter(|s| s.cycle >= start && s.cycle <= end)
            .map(|s| {
                let mut value = serde_json::to_value(s).unwrap();
                value["q"] = s.regs[register].into();
                value["selected_d"] = if s.d_register == Some(register) {
                    s.d.map(Into::into).unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                };
                value["we"] = (s.write_register == Some(register)).into();
                value
            })
            .collect();
        serde_json::json!({"start":start,"end":end,"cursor":self.cycle,"register":register,"samples":samples}).to_string()
    }
}

//! WM-Ring — prefrontal working-memory buffer (Miller 7±2 slots).
//!
//! Holds the current conversational turn before hippocampal encoding. High-salience
//! or end-of-turn slots flush via [`WmRing::flush`] into durable engrams.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

pub const DEFAULT_WM_CAPACITY: usize = 7;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WmSlot {
    pub content: String,
    pub context: String,
    pub salience: f32,
    pub tick: u64,
    #[serde(default)]
    pub semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub source_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WmRing {
    pub capacity: usize,
    pub turn_id: u64,
    slots: VecDeque<WmSlot>,
}

impl Default for WmRing {
    fn default() -> Self {
        Self::new(DEFAULT_WM_CAPACITY)
    }
}

impl WmRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.clamp(3, 9),
            turn_id: 0,
            slots: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn slots(&self) -> Vec<WmSlot> {
        self.slots.iter().cloned().collect()
    }

    pub fn slots_mut(&mut self) -> impl Iterator<Item = &mut WmSlot> {
        self.slots.iter_mut()
    }

    /// Push into WM; evict oldest when at capacity.
    pub fn push(&mut self, slot: WmSlot) {
        if self.slots.len() >= self.capacity {
            self.slots.pop_front();
        }
        self.slots.push_back(slot);
    }

    pub fn begin_turn(&mut self) {
        self.turn_id = self.turn_id.wrapping_add(1);
    }

    /// Drain all slots for hippocampal commit.
    pub fn drain(&mut self) -> Vec<WmSlot> {
        self.slots.drain(..).collect()
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WmFlushReport {
    pub committed: u32,
    pub turn_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_evicts_oldest_at_capacity() {
        let mut ring = WmRing::new(3);
        for i in 0..5 {
            ring.push(WmSlot {
                content: format!("m{i}"),
                context: "t".into(),
                salience: 0.5,
                tick: i,
                semantic_vector: None,
                tool_name: None,
                source_uri: None,
            });
        }
        assert_eq!(ring.len(), 3);
        let drained = ring.drain();
        assert_eq!(drained[0].content, "m2");
        assert_eq!(drained[2].content, "m4");
    }
}

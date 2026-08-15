//! Homeostasis — organ health metrics (measurement only; never changes recall ranking).
//!
//! Tracks durability cadence, generation hygiene, and agent-prompt token estimates so
//! the living-memory loop can be proven without touching CHORUS / activate scoring.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::env;
use std::path::Path;

const PROMPT_TOKEN_RING: usize = 32;

/// Runtime homeostasis counters (not persisted; rebuilt each process).
#[derive(Debug, Clone, Default)]
pub struct HomeostasisState {
    pub systems_seals_total: u64,
    pub agent_prompt_calls: u64,
    pub last_prompt_tokens_est: u64,
    prompt_token_ring: VecDeque<u64>,
}

impl HomeostasisState {
    pub fn note_systems_seal(&mut self) {
        self.systems_seals_total = self.systems_seals_total.saturating_add(1);
    }

    pub fn note_agent_prompt_tokens(&mut self, tokens: u64) {
        self.agent_prompt_calls = self.agent_prompt_calls.saturating_add(1);
        self.last_prompt_tokens_est = tokens;
        if self.prompt_token_ring.len() >= PROMPT_TOKEN_RING {
            self.prompt_token_ring.pop_front();
        }
        self.prompt_token_ring.push_back(tokens);
    }

    pub fn median_prompt_tokens_est(&self) -> Option<u64> {
        if self.prompt_token_ring.is_empty() {
            return None;
        }
        let mut v: Vec<u64> = self.prompt_token_ring.iter().copied().collect();
        v.sort_unstable();
        Some(v[v.len() / 2])
    }
}

/// Snapshot for `/status` and soak harnesses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeostasisReport {
    pub somnus_enabled: bool,
    pub somnus_keep: usize,
    pub somnus_seal_every_ticks: u64,
    pub systems_seals_total: u64,
    pub ticks_since_systems_seal: u64,
    pub generation_dirs: Option<usize>,
    /// True when generation dir count ≤ keep (when countable).
    pub generation_count_ok: Option<bool>,
    pub agent_prompt_calls: u64,
    pub last_prompt_tokens_est: u64,
    pub median_prompt_tokens_est: Option<u64>,
    pub agent_prompt_token_budget: usize,
    pub agent_prompt_max_engrams: usize,
    /// True when last prompt estimate ≤ token budget (or no calls yet).
    pub tokens_within_budget: bool,
}

impl Default for HomeostasisReport {
    fn default() -> Self {
        Self {
            somnus_enabled: true,
            somnus_keep: 3,
            somnus_seal_every_ticks: 360,
            systems_seals_total: 0,
            ticks_since_systems_seal: 0,
            generation_dirs: None,
            generation_count_ok: None,
            agent_prompt_calls: 0,
            last_prompt_tokens_est: 0,
            median_prompt_tokens_est: None,
            agent_prompt_token_budget: agent_prompt_token_budget(),
            agent_prompt_max_engrams: agent_prompt_max_engrams(),
            tokens_within_budget: true,
        }
    }
}

/// Rough token estimate for agent prompt packing.
/// Uses max(whitespace words, ceil(chars/4)) — conservative so we rarely overrun budgets.
pub fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().filter(|t| !t.is_empty()).count();
    let chars = text.chars().count();
    let by_chars = chars.div_ceil(4);
    words.max(by_chars).max(1)
}

pub fn count_generation_dirs(brain_path: &Path) -> Option<usize> {
    let gens = brain_path.join("generations");
    let rd = std::fs::read_dir(gens).ok()?;
    Some(rd.filter(|e| e.as_ref().map(|x| x.path().is_dir()).unwrap_or(false)).count())
}

pub fn agent_prompt_max_engrams() -> usize {
    env::var("FLUCTLIGHT_AGENT_ACTIVATE_MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8)
        .clamp(1, 64)
}

pub fn agent_prompt_token_budget() -> usize {
    env::var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512)
        .clamp(64, 8192)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_ring() {
        let mut h = HomeostasisState::default();
        h.note_agent_prompt_tokens(10);
        h.note_agent_prompt_tokens(30);
        h.note_agent_prompt_tokens(20);
        assert_eq!(h.median_prompt_tokens_est(), Some(20));
    }

    #[test]
    fn estimate_tokens_splits_words() {
        // max(word_count, ceil(chars/4)) — short words can make chars/4 dominate.
        assert_eq!(estimate_tokens("one two three"), 4);
        assert_eq!(estimate_tokens("abcdefghij klmnopqrst"), 6);
    }
}

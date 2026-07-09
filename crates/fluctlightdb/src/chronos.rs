//! Chronos — temporal & causal indexing as a first-class memory axis.
//!
//! # Why this exists
//! Agents must reason over *when* and *why*, not just *what*: "what did I do before the payment
//! failed?", "what caused the outage?". Inferring this from text every time is slow and brittle.
//! The hippocampus has dedicated **time cells** (Eichenbaum 2014) that tile elapsed time, and the
//! brain builds explicit causal models. Chronos gives FluctlightDB both: multi-scale time buckets
//! (mapping onto the lattice Time axis) for temporal proximity, and an explicit causal DAG for
//! before/after/because queries.
//!
//! Time buckets are multi-scale (like grid cells for time) so "around the same hour" and "around
//! the same day" are both cheap lookups. The causal graph is a DAG; `caused_by` links are directed,
//! and ancestry answers "what led to this?" without re-reading transcripts.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

/// An indexed event on the timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub tick: u64,
}

/// Temporal + causal index.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Chronos {
    events: HashMap<String, u64>,
    order: Vec<Event>,
    /// effect_id -> set of direct cause_ids.
    causes: HashMap<String, HashSet<String>>,
}

impl Chronos {
    /// Record an event at `tick`. Keeps `order` sorted by time for range queries.
    pub fn add_event(&mut self, id: impl Into<String>, tick: u64) {
        let id = id.into();
        self.events.insert(id.clone(), tick);
        let ev = Event { id, tick };
        let pos = self.order.partition_point(|e| e.tick <= tick);
        self.order.insert(pos, ev);
    }

    /// Declare that `cause` contributed to `effect` (directed causal edge).
    pub fn link_cause(&mut self, cause: impl Into<String>, effect: impl Into<String>) {
        self.causes
            .entry(effect.into())
            .or_default()
            .insert(cause.into());
    }

    /// Strict temporal order: did `a` happen before `b`?
    pub fn before(&self, a: &str, b: &str) -> Option<bool> {
        Some(self.events.get(a)? < self.events.get(b)?)
    }

    /// Multi-scale time bucket (grid-cell-for-time): `tick / scale`. Nearby events share buckets.
    pub fn bucket(&self, id: &str, scale: u64) -> Option<u64> {
        let scale = scale.max(1);
        self.events.get(id).map(|t| t / scale)
    }

    /// Total events recorded.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The most recent `n` events, newest last.
    pub fn recent(&self, n: usize) -> Vec<Event> {
        let start = self.order.len().saturating_sub(n);
        self.order[start..].to_vec()
    }

    /// Events within `[from, to]` inclusive, in temporal order.
    pub fn in_range(&self, from: u64, to: u64) -> Vec<Event> {
        self.order
            .iter()
            .filter(|e| e.tick >= from && e.tick <= to)
            .cloned()
            .collect()
    }

    /// The `n` events immediately preceding `id` (episodic "what happened just before").
    pub fn preceding(&self, id: &str, n: usize) -> Vec<Event> {
        let Some(&t) = self.events.get(id) else {
            return Vec::new();
        };
        self.order
            .iter()
            .filter(|e| e.tick < t || (e.tick == t && e.id != id))
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// All transitive causes of `effect` (causal ancestry — "what led to this?").
    pub fn causal_ancestors(&self, effect: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut q: VecDeque<String> = VecDeque::new();
        if let Some(direct) = self.causes.get(effect) {
            for c in direct {
                q.push_back(c.clone());
            }
        }
        while let Some(node) = q.pop_front() {
            if !seen.insert(node.clone()) {
                continue;
            }
            out.push(node.clone());
            if let Some(preds) = self.causes.get(&node) {
                for p in preds {
                    q.push_back(p.clone());
                }
            }
        }
        out
    }

    /// Temporal-consistency guard: a cause must not occur strictly after its effect.
    pub fn causally_consistent(&self) -> bool {
        for (effect, causes) in &self.causes {
            let Some(&et) = self.events.get(effect) else {
                continue;
            };
            for c in causes {
                if let Some(&ct) = self.events.get(c) {
                    if ct > et {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Chronos {
        let mut c = Chronos::default();
        c.add_event("login", 100);
        c.add_event("browse", 200);
        c.add_event("checkout", 300);
        c.add_event("payment_fail", 400);
        c
    }

    #[test]
    fn temporal_order_and_range() {
        let c = seed();
        assert_eq!(c.before("login", "checkout"), Some(true));
        assert_eq!(c.before("payment_fail", "login"), Some(false));
        let mid = c.in_range(150, 350);
        assert_eq!(
            mid.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["browse", "checkout"]
        );
    }

    #[test]
    fn preceding_events_are_episodic_context() {
        let c = seed();
        let before_fail = c.preceding("payment_fail", 2);
        assert_eq!(
            before_fail
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            vec!["browse", "checkout"]
        );
    }

    #[test]
    fn multiscale_buckets_group_nearby_events() {
        let c = seed();
        // At coarse scale, login+browse share a bucket; checkout+fail another.
        assert_eq!(c.bucket("login", 250), c.bucket("browse", 250));
        assert_ne!(c.bucket("login", 250), c.bucket("payment_fail", 250));
    }

    #[test]
    fn causal_ancestry_traverses_transitively() {
        let mut c = seed();
        c.link_cause("checkout", "payment_fail");
        c.link_cause("browse", "checkout");
        c.link_cause("login", "browse");
        let mut anc = c.causal_ancestors("payment_fail");
        anc.sort();
        assert_eq!(anc, vec!["browse", "checkout", "login"]);
    }

    #[test]
    fn detects_causal_time_violation() {
        let mut c = seed();
        c.link_cause("checkout", "login"); // effect (login@100) before cause (checkout@300) → invalid
        assert!(!c.causally_consistent());
    }

    #[test]
    fn consistent_causes_pass_guard() {
        let mut c = seed();
        c.link_cause("login", "payment_fail");
        assert!(c.causally_consistent());
    }
}

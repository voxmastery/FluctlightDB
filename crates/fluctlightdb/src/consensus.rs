//! Multi-Agent Consensus — shared memory with conflict resolution and access scoping.
//!
//! # Why this exists
//! When many agents write to one brain, they will disagree: two agents record different values for
//! the same key ("the customer's plan is Pro" vs "…is Free"), at different times, from sources of
//! different trust. A shared substrate for AGI needs a principled way to (a) detect the conflict,
//! (b) resolve it into a current belief, and (c) enforce who may read what. This is the social
//! analogue of memory reconsolidation: competing traces are arbitrated, not blindly overwritten.
//!
//! Resolution is confidence-weighted (see [`crate::confidence`]) with a recency tiebreak, so a
//! fresh high-trust claim wins, corroborating claims reinforce, and stale low-trust claims lose
//! without being deleted (they remain as minority evidence, auditable).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One agent's claim about a key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub agent_id: String,
    pub value: String,
    pub confidence: f32,
    pub tick: u64,
    /// Agents/roles allowed to read this claim (empty = public).
    pub scope: Vec<String>,
}

impl Claim {
    pub fn public(agent_id: impl Into<String>, value: impl Into<String>, confidence: f32, tick: u64) -> Self {
        Self {
            agent_id: agent_id.into(),
            value: value.into(),
            confidence: confidence.clamp(0.0, 1.0),
            tick,
            scope: Vec::new(),
        }
    }

    pub fn scoped(mut self, scope: Vec<String>) -> Self {
        self.scope = scope;
        self
    }

    fn readable_by(&self, viewer: Option<&str>) -> bool {
        if self.scope.is_empty() {
            return true;
        }
        match viewer {
            Some(v) => self.scope.iter().any(|s| s == v),
            None => false,
        }
    }
}

/// The resolved belief for a key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consensus {
    pub value: String,
    pub support: f32,
    pub contested: bool,
}

/// Shared memory of claims keyed by fact identity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedMemory {
    keys: HashMap<String, Vec<Claim>>,
}

impl SharedMemory {
    pub fn assert(&mut self, key: impl Into<String>, claim: Claim) {
        self.keys.entry(key.into()).or_default().push(claim);
    }

    /// Is there genuine disagreement (≥2 distinct values) on this key, among readable claims?
    pub fn is_contested(&self, key: &str, viewer: Option<&str>) -> bool {
        let Some(claims) = self.keys.get(key) else {
            return false;
        };
        let mut values: Vec<&str> = claims
            .iter()
            .filter(|c| c.readable_by(viewer))
            .map(|c| c.value.as_str())
            .collect();
        values.sort_unstable();
        values.dedup();
        values.len() >= 2
    }

    /// Resolve the current belief: sum confidence per value, recency breaks ties.
    pub fn resolve(&self, key: &str, viewer: Option<&str>) -> Option<Consensus> {
        let claims = self.keys.get(key)?;
        // value -> (total_support, latest_tick)
        let mut tally: HashMap<&str, (f32, u64)> = HashMap::new();
        let mut total = 0.0f32;
        for c in claims.iter().filter(|c| c.readable_by(viewer)) {
            let e = tally.entry(c.value.as_str()).or_insert((0.0, 0));
            e.0 += c.confidence;
            e.1 = e.1.max(c.tick);
            total += c.confidence;
        }
        if tally.is_empty() {
            return None;
        }
        let (value, (support, _)) = tally
            .iter()
            .max_by(|a, b| {
                a.1 .0
                    .partial_cmp(&b.1 .0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1 .1.cmp(&b.1 .1)) // recency tiebreak
            })
            .map(|(v, s)| (v.to_string(), *s))?;
        let contested = tally.len() >= 2;
        Some(Consensus {
            value,
            support: if total > 0.0 { support / total } else { 0.0 },
            contested,
        })
    }

    /// Claims visible to a viewer (access scoping).
    pub fn readable_claims(&self, key: &str, viewer: Option<&str>) -> Vec<&Claim> {
        self.keys
            .get(key)
            .map(|cs| cs.iter().filter(|c| c.readable_by(viewer)).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_confidence_claim_wins() {
        let mut m = SharedMemory::default();
        m.assert("plan", Claim::public("a1", "Free", 0.5, 10));
        m.assert("plan", Claim::public("a2", "Pro", 0.9, 12));
        let c = m.resolve("plan", None).unwrap();
        assert_eq!(c.value, "Pro");
        assert!(c.contested);
    }

    #[test]
    fn corroborating_claims_reinforce() {
        let mut m = SharedMemory::default();
        m.assert("plan", Claim::public("a1", "Pro", 0.5, 10));
        m.assert("plan", Claim::public("a2", "Pro", 0.5, 11));
        m.assert("plan", Claim::public("a3", "Free", 0.8, 12));
        // Two Pro (0.5+0.5=1.0) outweigh one stronger Free (0.8).
        let c = m.resolve("plan", None).unwrap();
        assert_eq!(c.value, "Pro");
    }

    #[test]
    fn recency_breaks_ties() {
        let mut m = SharedMemory::default();
        m.assert("status", Claim::public("a1", "open", 0.6, 5));
        m.assert("status", Claim::public("a2", "closed", 0.6, 99));
        let c = m.resolve("status", None).unwrap();
        assert_eq!(c.value, "closed", "equal support → newer wins");
    }

    #[test]
    fn uncontested_key_is_not_contested() {
        let mut m = SharedMemory::default();
        m.assert("name", Claim::public("a1", "Ada", 0.7, 1));
        m.assert("name", Claim::public("a2", "Ada", 0.7, 2));
        assert!(!m.is_contested("name", None));
        assert!(!m.resolve("name", None).unwrap().contested);
    }

    #[test]
    fn access_scoping_hides_private_claims() {
        let mut m = SharedMemory::default();
        m.assert("salary", Claim::public("hr", "100k", 0.9, 1).scoped(vec!["hr".into()]));
        // Public viewer can't see the scoped claim.
        assert!(m.readable_claims("salary", None).is_empty());
        assert!(m.resolve("salary", None).is_none());
        // Authorized viewer can.
        assert_eq!(m.resolve("salary", Some("hr")).unwrap().value, "100k");
    }

    #[test]
    fn contested_detection_respects_scope() {
        let mut m = SharedMemory::default();
        m.assert("plan", Claim::public("a1", "Pro", 0.7, 1));
        m.assert("plan", Claim::public("a2", "Free", 0.7, 2).scoped(vec!["admin".into()]));
        // Public sees only one value → not contested.
        assert!(!m.is_contested("plan", None));
        // Admin sees both → contested.
        assert!(m.is_contested("plan", Some("admin")));
    }
}

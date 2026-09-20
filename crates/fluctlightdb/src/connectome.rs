//! Connectome metadata + cue projection (FlyWire substrate).
//!
//! Metadata about imported biological neurons lives here, beside the graph, in its own
//! additive segment — `Region` is serde-by-name with no fallback, so it is never extended.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::id::NeuronId;

/// Dominant neurotransmitter of a presynaptic neuron (FlyWire `nt_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Nt {
    Ach,
    Gaba,
    Glut,
    Da,
    Ser,
    Oct,
    #[default]
    Unknown,
}

impl Nt {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "ACH" => Nt::Ach,
            "GABA" => Nt::Gaba,
            "GLUT" => Nt::Glut,
            "DA" => Nt::Da,
            "SER" => Nt::Ser,
            "OCT" => Nt::Oct,
            _ => Nt::Unknown,
        }
    }

    /// GABA and glutamate are inhibitory in the fly central brain (Liu & Wilson 2013).
    pub fn sign(self) -> f32 {
        match self {
            Nt::Gaba | Nt::Glut => -1.0,
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NeuronMeta {
    pub nt: Nt,
    pub super_class: String,
    pub class: String,
    pub side: String,
    /// Neuropil of the neuron's strongest presynaptic site set (first seen at import).
    pub neuropil: String,
}

/// Provenance + lookup tables for an imported connectome. Persisted as segment `connectome`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConnectomeMeta {
    pub source: String,
    pub imported_at: u64,
    pub p99_syn: f32,
    pub fanout: u8,
    pub entry_class: String,
    /// Sorted neuron ids of `entry_class` (Kenyon cells) — the cue entry layer.
    pub entry_set: Vec<NeuronId>,
    pub neurons: HashMap<NeuronId, NeuronMeta>,
}

impl ConnectomeMeta {
    /// Token → Kenyon-cell random sparse projection (antennal lobe → mushroom body analogue).
    /// Deterministic, codec-independent, `fanout` cells per token, duplicates allowed.
    pub fn project_tokens(&self, tokens: &[String]) -> Vec<NeuronId> {
        if self.entry_set.is_empty() {
            return Vec::new();
        }
        let n = self.entry_set.len() as u64;
        let mut out = Vec::with_capacity(tokens.len() * self.fanout as usize);
        for t in tokens {
            for k in 0..self.fanout {
                out.push(self.entry_set[(fnv1a64(t, k) % n) as usize]);
            }
        }
        out
    }

    pub fn is_inhibitory(&self, pre: NeuronId) -> bool {
        self.neurons
            .get(&pre)
            .map(|m| m.nt.sign() < 0.0)
            .unwrap_or(false)
    }

    pub fn summary(&self) -> serde_json::Value {
        let inhibitory = self.neurons.values().filter(|m| m.nt.sign() < 0.0).count();
        serde_json::json!({
            "present": true,
            "source": self.source,
            "imported_at": self.imported_at,
            "p99_syn": self.p99_syn,
            "fanout": self.fanout,
            "entry_class": self.entry_class,
            "entry_set_len": self.entry_set.len(),
            "neurons": self.neurons.len(),
            "inhibitory_neurons": inhibitory,
        })
    }
}

/// Synapse weight from summed synapse count: `sign × min(1, ln(1+Σ) / ln(1+p99))`.
/// p99 rather than max keeps the median fly edge near 0.5 so activation survives 3 hops
/// (spec D4) and ≤1 % of edges sit at the 1.0 clamp (`graph.rs:203`).
pub fn weight_for(sum_syn: u32, p99_syn: f32, inhibitory: bool) -> f32 {
    let denom = (1.0 + p99_syn.max(1.0)).ln();
    let w = ((1.0 + sum_syn as f32).ln() / denom).min(1.0);
    if inhibitory {
        -w
    } else {
        w
    }
}

/// FNV-1a 64 over `token` bytes then `k`. Plain hash on purpose: must not depend on
/// `neuron_codec` or `life_id`, so the projection is stable across brains and codecs.
pub fn fnv1a64(token: &str, k: u8) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for b in token.bytes().chain(std::iter::once(k)) {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::NeuronId;

    fn meta() -> ConnectomeMeta {
        let mut m = ConnectomeMeta {
            source: "test".into(),
            imported_at: 0,
            p99_syn: 78.0,
            fanout: 7,
            entry_class: "Kenyon_Cell".into(),
            entry_set: vec![
                NeuronId(1001),
                NeuronId(1002),
                NeuronId(1003),
                NeuronId(1004),
                NeuronId(1005),
            ],
            neurons: Default::default(),
        };
        m.neurons.insert(
            NeuronId(4001),
            NeuronMeta {
                nt: Nt::Gaba,
                super_class: "central".into(),
                class: "APL".into(),
                side: "right".into(),
                neuropil: "MB_CA_R".into(),
            },
        );
        m.neurons.insert(
            NeuronId(2001),
            NeuronMeta {
                nt: Nt::Ach,
                super_class: "central".into(),
                class: "ALPN".into(),
                side: "right".into(),
                neuropil: "AL_R".into(),
            },
        );
        m
    }

    #[test]
    fn nt_sign_is_negative_only_for_gaba_and_glut() {
        assert_eq!(Nt::Gaba.sign(), -1.0);
        assert_eq!(Nt::Glut.sign(), -1.0);
        for nt in [Nt::Ach, Nt::Da, Nt::Ser, Nt::Oct, Nt::Unknown] {
            assert_eq!(nt.sign(), 1.0);
        }
    }

    #[test]
    fn nt_parses_flywire_labels_case_insensitively() {
        assert_eq!(Nt::parse("GABA"), Nt::Gaba);
        assert_eq!(Nt::parse("ach"), Nt::Ach);
        assert_eq!(Nt::parse("GLUT"), Nt::Glut);
        assert_eq!(Nt::parse(""), Nt::Unknown);
        assert_eq!(Nt::parse("weird"), Nt::Unknown);
    }

    #[test]
    fn weight_is_monotone_signed_and_clamped() {
        assert!(weight_for(1, 78.0, false) < weight_for(8, 78.0, false));
        assert!(weight_for(8, 78.0, false) < weight_for(78, 78.0, false));
        assert_eq!(weight_for(78, 78.0, false), 1.0);
        assert_eq!(weight_for(2405, 78.0, false), 1.0);
        assert_eq!(weight_for(8, 78.0, true), -weight_for(8, 78.0, false));
        assert!((weight_for(8, 78.0, false) - 0.503).abs() < 0.01);
    }

    #[test]
    fn projection_is_deterministic_and_in_range() {
        let m = meta();
        let a = m.project_tokens(&["cue".to_string(), "word".to_string()]);
        let b = m.project_tokens(&["cue".to_string(), "word".to_string()]);
        assert_eq!(a, b);
        assert_eq!(a.len(), 2 * 7);
        for n in &a {
            assert!(m.entry_set.contains(n), "{n:?} not in entry set");
        }
    }

    #[test]
    fn projection_of_no_tokens_or_empty_entry_set_is_empty() {
        let mut m = meta();
        assert!(m.project_tokens(&[]).is_empty());
        m.entry_set.clear();
        assert!(m.project_tokens(&["cue".to_string()]).is_empty());
    }

    #[test]
    fn fnv_differs_across_k_and_token() {
        assert_ne!(fnv1a64("cue", 0), fnv1a64("cue", 1));
        assert_ne!(fnv1a64("cue", 0), fnv1a64("cub", 0));
    }

    #[test]
    fn is_inhibitory_reads_pre_neuron_nt() {
        let m = meta();
        assert!(m.is_inhibitory(NeuronId(4001)));
        assert!(!m.is_inhibitory(NeuronId(2001)));
        assert!(!m.is_inhibitory(NeuronId(9999)));
    }

    #[test]
    fn summary_reports_counts() {
        let m = meta();
        let s = m.summary();
        assert_eq!(s["present"], true);
        assert_eq!(s["neurons"], 2);
        assert_eq!(s["entry_set_len"], 5);
        assert_eq!(s["inhibitory_neurons"], 1);
        assert_eq!(s["source"], "test");
    }
}

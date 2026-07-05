//! Recall Confidence — provenance, recency, and corroboration fused into a trust score.
//!
//! # Why this exists
//! An agent acting autonomously must know *how much to trust* a memory before it acts on it.
//! A verified tool result, a corroborated fact repeated across sessions, and an offhand unverified
//! remark should not carry equal weight. The brain does this with source monitoring (prefrontal
//! reality checking) and confidence signals. This module fuses the evidence FluctlightDB already
//! tracks — provenance kind, verification, recency (via the forgetting curve), and independent
//! corroboration — into a single calibrated confidence in `[0,1]`.
//!
//! Corroboration uses a noisy-OR combination: independent sources each reduce the probability of
//! error, so many weak agreeing sources can exceed one strong source, but no single unverified
//! claim is treated as certain.

use serde::{Deserialize, Serialize};

/// Where a piece of evidence came from — determines its base reliability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    /// Ledger / tool output / file read — ground truth.
    Verified,
    /// User-stated fact.
    UserStated,
    /// Model inference / summary.
    Inferred,
    /// Unattributed or low-trust.
    Unknown,
}

impl SourceKind {
    /// Base reliability prior in [0,1].
    pub fn base_reliability(self) -> f32 {
        match self {
            SourceKind::Verified => 0.95,
            SourceKind::UserStated => 0.75,
            SourceKind::Inferred => 0.5,
            SourceKind::Unknown => 0.3,
        }
    }
}

/// One independent piece of supporting evidence for a claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: SourceKind,
    /// Retention/recency in (0,1] (e.g. from [`crate::forgetting::MemoryTrace::retention`]).
    pub recency: f32,
}

impl Evidence {
    pub fn new(kind: SourceKind, recency: f32) -> Self {
        Self {
            kind,
            recency: recency.clamp(0.0, 1.0),
        }
    }

    /// This source's standalone reliability, discounted by recency.
    fn reliability(&self) -> f32 {
        // Recency discounts, but never below a floor for verified ground truth.
        let floor = if self.kind == SourceKind::Verified { 0.6 } else { 0.0 };
        (self.kind.base_reliability() * (0.4 + 0.6 * self.recency)).max(floor * self.recency.max(0.5))
    }
}

/// Fuse multiple independent evidences into one confidence via noisy-OR.
/// `conf = 1 - Π(1 - r_i)` — corroboration compounds, but stays in [0,1].
pub fn recall_confidence(evidence: &[Evidence]) -> f32 {
    if evidence.is_empty() {
        return 0.0;
    }
    let prod_err: f32 = evidence.iter().map(|e| 1.0 - e.reliability().clamp(0.0, 0.999)).product();
    (1.0 - prod_err).clamp(0.0, 1.0)
}

/// Confidence-weighted activation multiplier for the recall path (centered near 1.0):
/// trusted memories are boosted, low-trust memories damped, without inverting rank order.
pub fn activation_multiplier(confidence: f32) -> f32 {
    0.6 + 0.8 * confidence.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_beats_unverified() {
        let v = recall_confidence(&[Evidence::new(SourceKind::Verified, 1.0)]);
        let u = recall_confidence(&[Evidence::new(SourceKind::Unknown, 1.0)]);
        assert!(v > u, "verified {v} should beat unknown {u}");
        assert!(v > 0.9);
    }

    #[test]
    fn corroboration_compounds() {
        let single = recall_confidence(&[Evidence::new(SourceKind::UserStated, 1.0)]);
        let triple = recall_confidence(&[
            Evidence::new(SourceKind::UserStated, 1.0),
            Evidence::new(SourceKind::UserStated, 1.0),
            Evidence::new(SourceKind::UserStated, 1.0),
        ]);
        assert!(triple > single, "corroboration should raise confidence: {single} -> {triple}");
        assert!(triple <= 1.0);
    }

    #[test]
    fn recency_discounts_confidence() {
        let fresh = recall_confidence(&[Evidence::new(SourceKind::Inferred, 1.0)]);
        let stale = recall_confidence(&[Evidence::new(SourceKind::Inferred, 0.1)]);
        assert!(fresh > stale, "fresh {fresh} should beat stale {stale}");
    }

    #[test]
    fn verified_stays_trusted_even_when_stale() {
        let stale_verified = recall_confidence(&[Evidence::new(SourceKind::Verified, 0.2)]);
        let fresh_unknown = recall_confidence(&[Evidence::new(SourceKind::Unknown, 1.0)]);
        assert!(stale_verified > fresh_unknown, "ground truth should resist decay");
    }

    #[test]
    fn empty_evidence_is_zero_confidence() {
        assert_eq!(recall_confidence(&[]), 0.0);
    }

    #[test]
    fn multiplier_is_centered_and_monotone() {
        assert!(activation_multiplier(0.0) < 1.0);
        assert!(activation_multiplier(1.0) > 1.0);
        assert!(activation_multiplier(0.9) > activation_multiplier(0.1));
    }
}

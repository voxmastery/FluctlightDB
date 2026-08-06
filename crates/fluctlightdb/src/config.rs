//! Per-brain recall tuning, resolved once when the brain is opened.
//!
//! # The defect this addresses
//!
//! Recall behaviour is driven by ~58 `FLUCTLIGHT_*` environment variables. Two of them were
//! read with `std::env::var` **on every `activate_scoped` call** — `FLUCTLIGHT_CORTEX_WEIGHT`
//! was parsed from a string into an `f32` per recall. That is a lookup and a parse in the
//! hottest path in the engine, for a value that cannot meaningfully change between two
//! recalls on the same brain.
//!
//! It is also shared mutable state. The Python SDK sets `os.environ[...]` inside its
//! `connect_*()` helpers, so a second `connect_chorus()` silently reconfigured every brain
//! already open in the process.
//!
//! # The scope of this fix, stated honestly
//!
//! [`RecallTuning`] is resolved **once per brain, at construction** — which is exactly after
//! `connect_*()` has set the environment — and then read from the brain handle. That:
//!
//! - removes the per-recall lookup and float parse, and
//! - makes these values genuinely per-brain, so two brains opened under different settings
//!   keep their own.
//!
//! What it deliberately does **not** do is memoize the mode flags (`FLUCTLIGHT_VECTOR_FAST`,
//! `FLUCTLIGHT_FAST_INGEST`, `FLUCTLIGHT_AGENT_FAST`). Those are read on demand in
//! `activation.rs` because the SDK sets them at runtime and expects the very next
//! `experience()` to observe the change — memoizing them process-wide silently breaks
//! documented mode switching, which is a worse defect than the one being fixed.
//!
//! Bringing the remaining knobs onto the brain handle is the same shape of change repeated,
//! and is worth doing; it is left out here so this lands without touching ingest semantics.
//!
//! Deployment knobs (`FLUCTLIGHT_MAX_CONNECTIONS`, `FLUCTLIGHT_API_KEYS`, `FLUCTLIGHT_SHARD_*`)
//! are correctly ambient — they describe the process, not a brain — and stay where they are.

use serde::{Deserialize, Serialize};

/// Recall constants belonging to one brain.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RecallTuning {
    /// Weight applied to the cortical fact/semantic prior when scoring recalls.
    pub cortex_weight: f32,
}

impl Default for RecallTuning {
    fn default() -> Self {
        // The literal the hot path used to fall back to when the variable was unset.
        Self { cortex_weight: 0.1 }
    }
}

impl RecallTuning {
    /// Resolve from the environment. Called once per brain, at construction.
    pub fn from_env() -> Self {
        Self {
            cortex_weight: std::env::var("FLUCTLIGHT_CORTEX_WEIGHT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(Self::default().cortex_weight),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::EnvGuard;

    /// Pins the default against the literal the pre-refactor hot path used. A transcription
    /// slip here would silently move every published recall number.
    #[test]
    fn default_matches_the_previous_inline_literal() {
        assert_eq!(RecallTuning::default().cortex_weight, 0.1);
    }

    #[test]
    fn resolves_from_env_at_construction() {
        let env = EnvGuard::acquire(&["FLUCTLIGHT_CORTEX_WEIGHT"]);
        env.set("FLUCTLIGHT_CORTEX_WEIGHT", "0.42");
        assert_eq!(RecallTuning::from_env().cortex_weight, 0.42);
        env.remove("FLUCTLIGHT_CORTEX_WEIGHT");
        assert_eq!(RecallTuning::from_env().cortex_weight, 0.1);
    }

    /// A malformed value must fall back to the default rather than panic or read as zero —
    /// a silent zero would disable the cortical prior entirely.
    #[test]
    fn malformed_value_falls_back_to_default() {
        let env = EnvGuard::acquire(&["FLUCTLIGHT_CORTEX_WEIGHT"]);
        env.set("FLUCTLIGHT_CORTEX_WEIGHT", "not-a-number");
        assert_eq!(RecallTuning::from_env().cortex_weight, 0.1);
    }
}

#[cfg(test)]
mod brain_tests {
    use crate::test_env::EnvGuard;
    use crate::FluctlightBrain;

    /// Two brains opened under different settings must keep their own tuning.
    ///
    /// Before this change the value was re-read from the process environment on every
    /// recall, so whichever `connect_*()` ran last silently retuned every brain already
    /// open — including ones the caller had configured deliberately.
    #[test]
    fn two_brains_do_not_share_recall_tuning() {
        let env = EnvGuard::acquire(&["FLUCTLIGHT_CORTEX_WEIGHT"]);
        env.set("FLUCTLIGHT_CORTEX_WEIGHT", "0.9");
        let hot = FluctlightBrain::new();
        env.set("FLUCTLIGHT_CORTEX_WEIGHT", "0.01");
        let cold = FluctlightBrain::new();

        assert_eq!(hot.tuning.cortex_weight, 0.9);
        assert_eq!(
            cold.tuning.cortex_weight, 0.01,
            "each brain keeps the tuning it was opened with"
        );
        // And the first brain is unaffected by the second's arrival.
        assert_eq!(
            hot.tuning.cortex_weight, 0.9,
            "opening a second brain must not retune the first"
        );
    }
}

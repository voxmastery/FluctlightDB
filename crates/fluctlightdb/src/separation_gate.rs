//! Separation-gated ingest — DG confusion risk blocks near-duplicate chat claims.

use std::collections::HashSet;

use crate::hippocampus::Hippocampus;
use crate::types::Episode;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SeparationGateResult {
    pub allowed: bool,
    pub confusion_risk: f32,
    pub separation_index: f32,
    pub max_overlap: f32,
    pub reason: Option<String>,
}

pub fn gate_enabled() -> bool {
    std::env::var("FLUCTLIGHT_SEPARATION_GATE")
        .map(|v| v != "0" && v.to_lowercase() != "false")
        .unwrap_or(true)
}

pub fn min_separation_threshold() -> f32 {
    std::env::var("FLUCTLIGHT_MIN_SEPARATION")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.35)
}

pub fn overlap_window() -> usize {
    std::env::var("FLUCTLIGHT_SEPARATION_OVERLAP_WINDOW")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512)
}

pub fn assess(hippocampus: &Hippocampus, episode: &Episode, life_id: uuid::Uuid) -> SeparationGateResult {
    let probe: HashSet<String> = episode
        .content
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect();
    if probe.is_empty() {
        return SeparationGateResult {
            allowed: true,
            confusion_risk: 0.0,
            separation_index: 1.0,
            max_overlap: 0.0,
            reason: None,
        };
    }
    let window = overlap_window();
    let peers = hippocampus.tail_for_life(life_id, window);
    let mut best_overlap = 0.0f32;
    let mut best_sep = 1.0f32;
    for e in peers {
        let existing: HashSet<String> = e
            .episode
            .content
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();
        if existing.is_empty() {
            continue;
        }
        let inter = probe.intersection(&existing).count() as f32;
        let union = probe.union(&existing).count() as f32;
        let jaccard = if union > 0.0 { inter / union } else { 0.0 };
        if jaccard > best_overlap {
            best_overlap = jaccard;
            best_sep = e.separation_index.max(1.0 - jaccard);
        }
    }
    let threshold = min_separation_threshold();
    let allowed = if best_overlap >= 0.85 {
        false
    } else {
        best_overlap < 0.72 || best_sep >= threshold
    };
    SeparationGateResult {
        allowed,
        confusion_risk: best_overlap,
        separation_index: best_sep,
        max_overlap: best_overlap,
        reason: if allowed {
            None
        } else {
            Some(format!(
                "separation gate: overlap={best_overlap:.2} sep={best_sep:.2} < {threshold:.2}"
            ))
        },
    }
}

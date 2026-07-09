//! Conflict lattice — π-inhibition style fact arbitration for agents.
//!
//! Competing engrams about the same key are ranked by provenance trust + recency + salience.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::hippocampus::Hippocampus;
use crate::types::{ProvenanceKind, RecallResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedFact {
    pub key: String,
    pub value: String,
    pub winner_engram_id: Option<Uuid>,
    pub contested: bool,
    pub confidence: f32,
    pub trust_note: String,
}

fn provenance_weight(kind: &ProvenanceKind, verified: bool) -> f32 {
    let base = match kind {
        ProvenanceKind::LedgerVerified => 1.0,
        ProvenanceKind::ToolGrounded => 0.88,
        ProvenanceKind::FileObservation => 0.82,
        ProvenanceKind::UserExplicit => 0.78,
        ProvenanceKind::ChatAssertion => 0.45,
    };
    if verified {
        base + 0.08
    } else {
        base
    }
}

/// Score a recall candidate for conflict resolution.
pub fn score_candidate(hit: &RecallResult) -> f32 {
    let prov = hit.episode.provenance.as_ref();
    let kind = prov
        .map(|p| &p.kind)
        .unwrap_or(&ProvenanceKind::ChatAssertion);
    let verified = prov.map(|p| p.verified).unwrap_or(false);
    let conf = prov.map(|p| p.confidence).unwrap_or(0.4);
    let trust = provenance_weight(kind, verified);
    hit.activation * 0.35 + trust * 0.4 + conf * 0.15 + hit.episode.salience_hint * 0.1
}

/// Pick the winning fact from activation results; detect contested keys.
pub fn resolve_from_recalls(cue: &str, recalls: &[RecallResult]) -> ResolvedFact {
    if recalls.is_empty() {
        return ResolvedFact {
            key: cue.to_string(),
            value: String::new(),
            winner_engram_id: None,
            contested: false,
            confidence: 0.0,
            trust_note: "no matching memories".into(),
        };
    }

    let mut ranked: Vec<(f32, &RecallResult)> =
        recalls.iter().map(|r| (score_candidate(r), r)).collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let (top_score, winner) = ranked[0];
    let second = ranked.get(1).map(|(s, _)| *s).unwrap_or(0.0);
    let contested = ranked.len() > 1 && (top_score - second) < 0.12;

    let prov = winner.episode.provenance.as_ref();
    let trust_note = if winner.verified {
        "verified ground truth".into()
    } else if contested {
        "contested — multiple traces disagree".into()
    } else {
        format!(
            "best match via {:?}",
            prov.map(|p| &p.kind)
                .unwrap_or(&ProvenanceKind::ChatAssertion)
        )
    };

    ResolvedFact {
        key: cue.to_string(),
        value: winner.episode.content.clone(),
        winner_engram_id: Some(winner.engram_id),
        contested,
        confidence: top_score.clamp(0.0, 1.0),
        trust_note,
    }
}

/// Lexical key extraction for hippocampal engrams (simple slot:value or noun phrases).
pub fn fact_key_from_cue(cue: &str) -> String {
    cue.split_whitespace()
        .filter(|w| w.len() > 2)
        .take(6)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Find engrams that might conflict on the same topic as `cue`.
pub fn related_engram_ids(hippocampus: &Hippocampus, cue: &str, limit: usize) -> Vec<Uuid> {
    let key_tokens: Vec<String> = cue
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|s| s.to_string())
        .collect();
    if key_tokens.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(f32, Uuid)> = hippocampus
        .engrams
        .iter()
        .map(|e| {
            let body = format!("{} {}", e.episode.content, e.episode.context).to_lowercase();
            let hits = key_tokens
                .iter()
                .filter(|t| body.contains(t.as_str()))
                .count() as f32;
            let score = hits / key_tokens.len() as f32;
            (score, e.id)
        })
        .filter(|(s, _)| *s > 0.2)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(limit).map(|(_, id)| id).collect()
}

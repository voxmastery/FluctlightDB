//! Reality monitoring — verified facts for prompt injection (source monitoring analog).

use serde::{Deserialize, Serialize};

use crate::brain::FluctlightBrain;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifiedFact {
    pub engram_id: uuid::Uuid,
    pub content: String,
    pub source_uri: Option<String>,
    pub confidence: f32,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifiedContext {
    pub facts: Vec<VerifiedFact>,
    pub unverified_warnings: Vec<String>,
}

pub fn verified_context(brain: &FluctlightBrain, limit: usize) -> VerifiedContext {
    let mut facts = Vec::new();
    for e in brain.hippocampus.engrams_for_life(brain.life.life_id) {
        let Some(ref p) = e.episode.provenance else {
            continue;
        };
        if !p.verified {
            continue;
        }
        facts.push(VerifiedFact {
            engram_id: e.id,
            content: e.episode.content.clone(),
            source_uri: p.source_uri.clone(),
            confidence: p.confidence,
            kind: format!("{:?}", p.kind),
        });
    }
    facts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    facts.truncate(limit);

    let mut unverified_warnings = Vec::new();
    for e in brain.hippocampus.engrams_for_life(brain.life.life_id) {
        let c = e.episode.content.to_lowercase();
        let verified = e
            .episode
            .provenance
            .as_ref()
            .map(|p| p.verified)
            .unwrap_or(false);
        if !verified && (c.contains('$') || c.contains("balance")) {
            unverified_warnings.push(format!(
                "unverified: {}",
                e.episode.content.chars().take(100).collect::<String>()
            ));
            if unverified_warnings.len() >= 5 {
                break;
            }
        }
    }

    VerifiedContext {
        facts,
        unverified_warnings,
    }
}

pub fn format_for_prompt(ctx: &VerifiedContext) -> String {
    if ctx.facts.is_empty() && ctx.unverified_warnings.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n## Verified ground truth (Fluctlight ledger/file)\n");
    for f in &ctx.facts {
        out.push_str(&format!(
            "- [verified {:.0}%] {}\n",
            f.confidence * 100.0,
            f.content
        ));
    }
    if !ctx.unverified_warnings.is_empty() {
        out.push_str("\n## Unverified chat claims (do NOT treat as fact)\n");
        for w in &ctx.unverified_warnings {
            out.push_str(&format!("- {w}\n"));
        }
    }
    out
}

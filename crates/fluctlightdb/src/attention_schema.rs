//! Attention Schema (Graziano AST) — a simplified internal model OF attention.
//!
//! This is **not**:
//! - engram `salience` (encoding priority)
//! - prefrontal goals (top-down bias of what *should* matter)
//! - `fovea` (saccadic file intake)
//!
//! Graziano's Attention Schema Theory: the brain builds a coarse model of its own
//! attentional state ("what am I attending to, how strongly, who owns it?"). That
//! model is used for (1) control — redirecting the spotlight — and (2) an
//! introspective report that can read like subjective awareness.
//!
//! FluctlightDB implements the *engineering* substrate of that idea:
//! observe attentional effects → maintain a schema → bias the next `activate()`.

use crate::tokenize::tokenize;
use crate::types::RecallResult;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const MAX_TRAJECTORY: usize = 16;
const MAX_SPOTLIGHT_BOOST: f32 = 0.45;
const DEFAULT_CONTROL_GAIN: f32 = 0.85;
const INTENSITY_DECAY: f32 = 0.04;
const CONFIDENCE_DECAY: f32 = 0.02;

/// Who the schema attributes the current spotlight to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AttentionOwner {
    /// "I am attending" — self-directed / endogenous control.
    #[default]
    SelfOwned,
    /// Spotlight driven by an external cue or agent.
    External,
    Ambiguous,
}

/// How the current spotlight was established.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpotlightSource {
    #[default]
    Cue,
    WorkingMemory,
    /// Voluntary redirect via the schema's control path.
    Redirect,
    Observed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpotlightTarget {
    pub summary: String,
    pub tokens: Vec<String>,
    #[serde(default)]
    pub engram_ids: Vec<String>,
    pub source: SpotlightSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttentionSnapshot {
    pub tick: u64,
    pub summary: String,
    pub intensity: f32,
    pub owner: AttentionOwner,
}

/// Introspective readout of the attention schema ("I am attending to…").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AttentionSchemaReport {
    pub attending: bool,
    pub subject: String,
    pub intensity: f32,
    pub depth: f32,
    pub owner: AttentionOwner,
    pub model_confidence: f32,
    pub source: Option<SpotlightSource>,
    /// Plain-language schema narration (control + awareness proxy).
    pub narration: String,
}

/// Simplified model of the brain's own attentional state (AST).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttentionSchema {
    /// Schema's estimate that attention is currently active.
    pub attending: bool,
    pub owner: AttentionOwner,
    /// Estimated spotlight strength in \[0, 1\].
    pub intensity: f32,
    /// How confident the schema is in its own model \[0, 1\].
    pub model_confidence: f32,
    /// Estimated processing depth (shallow scan → deep engagement).
    pub depth: f32,
    pub spotlight: Option<SpotlightTarget>,
    #[serde(default)]
    pub trajectory: VecDeque<AttentionSnapshot>,
    pub tick: u64,
    /// How strongly the schema's spotlight biases the next activate.
    pub control_gain: f32,
}

impl Default for AttentionSchema {
    fn default() -> Self {
        Self {
            attending: false,
            owner: AttentionOwner::SelfOwned,
            intensity: 0.0,
            model_confidence: 0.0,
            depth: 0.0,
            spotlight: None,
            trajectory: VecDeque::new(),
            tick: 0,
            control_gain: DEFAULT_CONTROL_GAIN,
        }
    }
}

impl AttentionSchema {
    /// Voluntary control: set the spotlight using the schema (endogenous attention).
    pub fn redirect(&mut self, target: &str, tick: u64) {
        let summary = target.chars().take(240).collect::<String>();
        if summary.trim().is_empty() {
            self.release(tick);
            return;
        }
        let tokens = tokenize(&summary);
        self.attending = true;
        self.owner = AttentionOwner::SelfOwned;
        self.intensity = 0.9;
        self.depth = 0.75;
        self.model_confidence = 0.85;
        self.tick = tick;
        self.spotlight = Some(SpotlightTarget {
            summary: summary.clone(),
            tokens,
            engram_ids: Vec::new(),
            source: SpotlightSource::Redirect,
        });
        self.push_trajectory(summary, tick);
    }

    /// Clear the modeled spotlight (attention released).
    pub fn release(&mut self, tick: u64) {
        self.attending = false;
        self.intensity = 0.0;
        self.depth = 0.0;
        self.model_confidence = (self.model_confidence * 0.5).max(0.1);
        self.spotlight = None;
        self.tick = tick;
        self.owner = AttentionOwner::SelfOwned;
    }

    /// Update the schema from observed activation (bottom-up model of what attention did).
    pub fn observe_activation(&mut self, cue: &str, recalls: &[RecallResult], tick: u64) {
        self.tick = tick;
        if recalls.is_empty() {
            // Cue tried to pull attention but nothing engaged — lower confidence.
            self.model_confidence = (self.model_confidence * 0.7).max(0.05);
            if let Some(spot) = self.spotlight.as_ref() {
                if token_overlap(&tokenize(cue), &spot.tokens) < 0.15 {
                    self.intensity = (self.intensity * 0.6).max(0.05);
                }
            }
            return;
        }

        let top = &recalls[0];
        let top_n = recalls.len().min(3);
        let mean_act: f32 =
            recalls[..top_n].iter().map(|r| r.activation).sum::<f32>() / top_n as f32;
        let intensity = (mean_act / 2.5).clamp(0.15, 1.0);
        let depth = if top_n >= 3 && mean_act > 1.0 {
            0.85
        } else if mean_act > 0.6 {
            0.55
        } else {
            0.3
        };

        let mut engram_ids = Vec::new();
        for r in recalls.iter().take(5) {
            engram_ids.push(r.engram_id.to_string());
        }

        let summary: String = if cue.trim().is_empty() {
            top.episode.content.chars().take(160).collect()
        } else {
            cue.chars().take(160).collect()
        };
        let tokens = tokenize(&summary);

        // If redirect already owns a close spotlight, keep SelfOwned; else External cue.
        let owner = match &self.spotlight {
            Some(spot)
                if spot.source == SpotlightSource::Redirect
                    && token_overlap(&tokens, &spot.tokens) >= 0.25 =>
            {
                AttentionOwner::SelfOwned
            }
            Some(_) if token_overlap(&tokens, &tokenize(cue)) < 0.2 => AttentionOwner::Ambiguous,
            _ => AttentionOwner::External,
        };

        self.attending = true;
        self.owner = owner;
        self.intensity = intensity;
        self.depth = depth;
        self.model_confidence = (0.45 + 0.4 * intensity).clamp(0.2, 0.95);
        self.spotlight = Some(SpotlightTarget {
            summary: summary.clone(),
            tokens,
            engram_ids,
            source: SpotlightSource::Observed,
        });
        self.push_trajectory(summary, tick);
    }

    /// Control signal: boost content that matches the schema's current spotlight.
    pub fn spotlight_boost(&self, content: &str) -> f32 {
        let Some(spot) = self.spotlight.as_ref() else {
            return 0.0;
        };
        if !self.attending || self.intensity < 0.05 {
            return 0.0;
        }
        let overlap = token_overlap(&tokenize(content), &spot.tokens);
        if overlap <= 0.0 {
            return 0.0;
        }
        (overlap * self.intensity * self.control_gain * MAX_SPOTLIGHT_BOOST)
            .clamp(0.0, MAX_SPOTLIGHT_BOOST)
    }

    /// Introspective schema report (awareness proxy — not claimed phenomenology).
    pub fn report(&self) -> AttentionSchemaReport {
        let subject = self
            .spotlight
            .as_ref()
            .map(|s| s.summary.clone())
            .unwrap_or_default();
        let source = self.spotlight.as_ref().map(|s| s.source);
        let narration = if !self.attending || subject.is_empty() {
            "Attention schema idle: no active spotlight.".to_string()
        } else {
            let who = match self.owner {
                AttentionOwner::SelfOwned => "I am",
                AttentionOwner::External => "Attention is externally",
                AttentionOwner::Ambiguous => "Attention is ambiguously",
            };
            format!(
                "{who} attending to «{subject}» (intensity={:.2}, depth={:.2}, confidence={:.2})",
                self.intensity, self.depth, self.model_confidence
            )
        };
        AttentionSchemaReport {
            attending: self.attending,
            subject,
            intensity: self.intensity,
            depth: self.depth,
            owner: self.owner,
            model_confidence: self.model_confidence,
            source,
            narration,
        }
    }

    /// Per-tick fade of unmaintained attention (schema maintenance cost).
    pub fn tick_decay(&mut self, tick: u64) {
        self.tick = tick;
        if !self.attending {
            return;
        }
        self.intensity = (self.intensity - INTENSITY_DECAY).max(0.0);
        self.model_confidence = (self.model_confidence - CONFIDENCE_DECAY).max(0.05);
        self.depth = (self.depth * 0.97).max(0.0);
        if self.intensity < 0.08 {
            self.release(tick);
        }
    }

    fn push_trajectory(&mut self, summary: String, tick: u64) {
        self.trajectory.push_back(AttentionSnapshot {
            tick,
            summary,
            intensity: self.intensity,
            owner: self.owner,
        });
        while self.trajectory.len() > MAX_TRAJECTORY {
            self.trajectory.pop_front();
        }
    }
}

fn token_overlap(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let bset: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let hits = a.iter().filter(|t| bset.contains(t.as_str())).count();
    hits as f32 / a.len().max(b.len()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;
    use uuid::Uuid;

    fn recall(content: &str, activation: f32) -> RecallResult {
        RecallResult {
            engram_id: Uuid::new_v4(),
            activation,
            episode: Episode {
                content: content.into(),
                context: "test".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            },
            completion_strength: 0.5,
            separation_index: 0.0,
            verified: false,
            trust_note: None,
        }
    }

    #[test]
    fn redirect_sets_self_owned_spotlight() {
        let mut schema = AttentionSchema::default();
        schema.redirect("attention schema architecture", 1);
        assert!(schema.attending);
        assert_eq!(schema.owner, AttentionOwner::SelfOwned);
        let report = schema.report();
        assert!(report.narration.contains("attending"));
        assert!(report.subject.contains("attention schema"));
    }

    #[test]
    fn spotlight_boosts_matching_content() {
        let mut schema = AttentionSchema::default();
        schema.redirect("locomo evidence recall honesty", 2);
        let hit = schema.spotlight_boost("honest locomo evidence recall protocol");
        let miss = schema.spotlight_boost("unrelated cooking recipe pasta");
        assert!(hit > 0.05, "expected boost, got {hit}");
        assert!(miss < hit);
    }

    #[test]
    fn observe_updates_schema_from_activation() {
        let mut schema = AttentionSchema::default();
        let recalls = vec![
            recall("tool call failed timeout", 1.8),
            recall("retry succeeded after timeout", 1.1),
        ];
        schema.observe_activation("tool timeout", &recalls, 3);
        assert!(schema.attending);
        assert!(schema.intensity > 0.2);
        assert_eq!(
            schema.spotlight.as_ref().unwrap().source,
            SpotlightSource::Observed
        );
        assert!(!schema.report().narration.is_empty());
    }

    #[test]
    fn tick_decay_releases_weak_spotlight() {
        let mut schema = AttentionSchema::default();
        schema.redirect("temporary focus", 1);
        schema.intensity = 0.1;
        schema.tick_decay(2);
        assert!(!schema.attending);
        assert!(schema.spotlight.is_none());
    }
}

//! Agent ergonomics runtime — WM-Ring, tool observe, unified recall, retention, idle consolidate.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::chorus::ChorusRecallOpts;
use crate::chorus_runtime::{chorus_enabled, chorus_fast_enabled, chorus_float_rerank_enabled};
use crate::conflict_lattice::{resolve_from_recalls, ResolvedFact};
use crate::error::Result;
use crate::recall_router::{
    choose_mode, filter_hits_by_tick, lanes_from_activation, merge_hits, temporal_filter_from_cue,
    RecallMode, TemporalFilter, UnifiedRecallHit, UnifiedRecallResult,
};
use crate::retention_policy::{RetentionPolicy, RetentionReport, RetentionState};
use crate::types::{Episode, Provenance, ProvenanceKind};
use crate::wm_ring::{WmFlushReport, WmRing, WmSlot};

pub fn agent_ergonomics_enabled() -> bool {
    std::env::var("FLUCTLIGHT_AGENT_ERGONOMICS")
        .map(|v| !matches!(v.to_lowercase().as_str(), "0" | "false" | "no" | "off"))
        .unwrap_or(true)
}

/// Persisted agent-facing state (WM, retention, activity clock).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentState {
    pub wm: WmRing,
    pub retention: RetentionState,
    pub ticks_since_activity: u64,
    pub idle_consolidations: u64,
    #[serde(default = "default_auto_consolidate")]
    pub auto_consolidate: bool,
    #[serde(default = "default_idle_ticks")]
    pub idle_ticks_before_consolidate: u64,
}

fn default_auto_consolidate() -> bool {
    true
}

impl Default for AgentState {
    fn default() -> Self {
        Self {
            wm: WmRing::default(),
            retention: RetentionState::default(),
            ticks_since_activity: 0,
            idle_consolidations: 0,
            auto_consolidate: true,
            idle_ticks_before_consolidate: default_idle_ticks(),
        }
    }
}

fn default_idle_ticks() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsolidateReport {
    pub hippocampal: Option<crate::types::SleepReport>,
    pub chorus: Option<crate::chorus::ChorusSleepReport>,
    pub retention: RetentionReport,
    pub wm_flushed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolObserveInput {
    pub tool_name: String,
    pub result: String,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub salience: f32,
    #[serde(default)]
    pub semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    pub to_working_memory: bool,
}

fn wm_lexical_hits(cue: &str, slots: &[WmSlot], k: usize) -> Vec<UnifiedRecallHit> {
    use std::collections::HashSet;

    let cue_toks: HashSet<String> = crate::tokenize::tokenize(cue).into_iter().collect();
    if cue_toks.is_empty() {
        return Vec::new();
    }
    let cue_len = cue_toks.len() as f32;
    let mut scored: Vec<UnifiedRecallHit> = slots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let text = format!("{} {}", s.content, s.context);
            let toks = crate::tokenize::tokenize(&text);
            let overlap = toks.iter().filter(|t| cue_toks.contains(*t)).count();
            if overlap == 0 {
                return None;
            }
            Some(UnifiedRecallHit {
                memory_id: format!("wm:{i}"),
                score: overlap as f32 / cue_len,
                lane: "working_memory".into(),
                content: s.content.clone(),
                context: s.context.clone(),
                verified: false,
                engram_id: None,
                snippet: None,
            })
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    scored
}

impl FluctlightBrain {
    pub fn agent_state(&self) -> &AgentState {
        &self.agent
    }

    pub fn wm_slots(&self) -> Vec<WmSlot> {
        self.agent.wm.slots()
    }

    pub fn wm_len(&self) -> usize {
        self.agent.wm.len()
    }

    /// Start a new agent turn (increments turn counter; WM persists until flush).
    pub fn turn_begin(&mut self) {
        self.agent.wm.begin_turn();
        self.touch_activity();
    }

    /// Push content into working memory (7±2 ring).
    pub fn wm_push(
        &mut self,
        content: impl Into<String>,
        context: impl Into<String>,
        salience: f32,
        semantic_vector: Option<Vec<f32>>,
    ) {
        let tick = self.autonomic.total_ticks;
        self.agent.wm.push(WmSlot {
            content: content.into(),
            context: context.into(),
            salience,
            tick,
            semantic_vector,
            tool_name: None,
            source_uri: None,
        });
        self.touch_activity();
    }

    /// Commit WM slots to hippocampus (+ CHORUS imprint when enabled).
    pub fn turn_end(&mut self, flush_wm: bool) -> Result<WmFlushReport> {
        if !flush_wm {
            return Ok(WmFlushReport {
                committed: 0,
                turn_id: self.agent.wm.turn_id,
            });
        }
        self.flush_wm_internal()
    }

    pub fn flush_wm(&mut self) -> Result<WmFlushReport> {
        self.flush_wm_internal()
    }

    fn flush_wm_internal(&mut self) -> Result<WmFlushReport> {
        let turn_id = self.agent.wm.turn_id;
        let slots = self.agent.wm.drain();
        let mut committed = 0u32;
        for slot in slots {
            let mut episode = Episode::new(slot.content, slot.context, slot.salience);
            episode.semantic_vector = slot.semantic_vector;
            if let Some(uri) = slot.source_uri {
                episode.provenance = Some(Provenance {
                    kind: if slot.tool_name.is_some() {
                        ProvenanceKind::ToolGrounded
                    } else {
                        ProvenanceKind::ChatAssertion
                    },
                    source_uri: Some(uri),
                    confidence: 0.85,
                    verified: false,
                });
            }
            let report = self.experience_internal(episode, false)?;
            self.agent_on_experience(report.engram_id);
            committed += 1;
        }
        self.touch_activity();
        Ok(WmFlushReport { committed, turn_id })
    }

    /// Observe a tool/MCP result with provenance sheath pre-filled.
    pub fn observe_tool(&mut self, input: &ToolObserveInput) -> Result<serde_json::Value> {
        let ctx = input
            .context
            .clone()
            .unwrap_or_else(|| format!("tool:{}", input.tool_name));
        let salience = if input.salience > 0.0 {
            input.salience
        } else {
            0.72
        };
        if input.to_working_memory {
            self.agent.wm.push(WmSlot {
                content: input.result.clone(),
                context: ctx.clone(),
                salience,
                tick: self.autonomic.total_ticks,
                semantic_vector: input.semantic_vector.clone(),
                tool_name: Some(input.tool_name.clone()),
                source_uri: input.uri.clone(),
            });
            self.touch_activity();
            return Ok(serde_json::json!({
                "stored": "working_memory",
                "wm_len": self.agent.wm.len(),
            }));
        }

        let mut episode = Episode::new(
            format!("[{}] {}", input.tool_name, input.result),
            ctx,
            salience,
        );
        episode.semantic_vector = input.semantic_vector.clone();
        episode.provenance = Some(Provenance {
            kind: ProvenanceKind::ToolGrounded,
            source_uri: input.uri.clone(),
            confidence: 0.88,
            verified: false,
        });
        let report = self.experience_internal(episode, true)?;
        if chorus_enabled() {
            let _ = self.chorus_imprint(&crate::chorus::ChorusImprintInput {
                memory_id: report.engram_id.to_string(),
                content: input.result.clone(),
                context: input.tool_name.clone(),
                semantic_vector: input.semantic_vector.clone(),
                salience,
                sheath: crate::chorus::ProvenanceSheath {
                    agent_id: None,
                    verified: false,
                    provenance_kind: 2,
                    source_uri: input.uri.clone(),
                },
            });
        }
        self.touch_activity();
        Ok(serde_json::json!({
            "stored": "hippocampus",
            "engram_id": report.engram_id.to_string(),
        }))
    }

    pub fn set_retention_policy(&mut self, policy: RetentionPolicy) {
        self.agent.retention.set_policy(policy);
    }

    pub fn retention_policy(&self) -> &RetentionPolicy {
        &self.agent.retention.policy
    }

    /// Apply retention DSL — prune aged / low-salience engrams and CHORUS traces.
    pub fn apply_retention(&mut self) -> Result<RetentionReport> {
        let now = self.autonomic.total_ticks;
        let mut report = RetentionReport::default();
        let before = self.hippocampus.engrams.len();
        self.hippocampus.engrams.retain(|e| {
            let verified = e
                .episode
                .provenance
                .as_ref()
                .map(|p| p.verified)
                .unwrap_or(false);
            !self
                .agent
                .retention
                .should_prune_engram(e.id, now, e.salience, verified)
        });
        report.pruned_engrams = before.saturating_sub(self.hippocampus.engrams.len()) as u32;
        for id in self
            .agent
            .retention
            .engram_ticks
            .keys()
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.hippocampus.engrams.iter().any(|e| e.id == id) {
                self.agent.retention.engram_ticks.remove(&id);
            }
        }
        if chorus_enabled() && self.agent.retention.policy.min_salience > 0.0 {
            report.pruned_chorus = self.chorus.decay_untagged(0);
        }
        Ok(report)
    }

    /// Unified recall — auto-routes episodic / CHORUS / Muon / hybrid; falls back to WM lexically.
    pub fn recall_unified(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        mode: RecallMode,
        k: usize,
        temporal: Option<TemporalFilter>,
    ) -> UnifiedRecallResult {
        let mode = choose_mode(
            cue,
            self.hippocampus.engrams.len(),
            self.chorus_len(),
            self.muon.len(),
            mode,
        );
        let mut temporal = temporal;
        if temporal.is_none() {
            temporal = temporal_filter_from_cue(cue, self.autonomic.total_ticks);
        }
        let mut lanes_used = Vec::new();
        let mut lane_vecs: Vec<Vec<UnifiedRecallHit>> = Vec::new();

        match mode {
            RecallMode::Episodic | RecallMode::Hybrid | RecallMode::Auto => {
                let act = self.activate_scoped(cue, cue_vector, None, k.saturating_mul(2));
                if !act.recalls.is_empty() {
                    lanes_used.push("episodic".into());
                    lane_vecs.push(lanes_from_activation(&act));
                }
            }
            _ => {}
        }

        if matches!(mode, RecallMode::Corpus | RecallMode::Hybrid) && chorus_enabled() {
            let opts = ChorusRecallOpts {
                fast: chorus_fast_enabled(),
                float_rerank: chorus_float_rerank_enabled(),
            };
            let hits = self.chorus_recall_with_opts(cue, k, cue_vector, opts);
            if !hits.is_empty() {
                lanes_used.push("chorus".into());
                lane_vecs.push(hits.iter().map(UnifiedRecallHit::from_chorus).collect());
            }
        }

        if matches!(mode, RecallMode::Session | RecallMode::Hybrid) {
            if self.muon.len() > 0 {
                let hits = self.muon_recall(cue, k);
                if !hits.is_empty() {
                    lanes_used.push("muon".into());
                    lane_vecs.push(hits.iter().map(UnifiedRecallHit::from_muon).collect());
                }
            }
            if self.tau_shard_len() > 0 {
                let hits = self.tau_recall(cue, k);
                if !hits.is_empty() {
                    lanes_used.push("tau".into());
                    lane_vecs.push(hits.iter().map(UnifiedRecallHit::from_tau).collect());
                }
            }
        }

        let mut hits = if lane_vecs.len() > 1 {
            merge_hits(lane_vecs, k.saturating_mul(2))
        } else {
            lane_vecs.into_iter().next().unwrap_or_default()
        };

        // Embedded agents often recall before turn_end flush — search WM lexically.
        if hits.is_empty() && !self.agent.wm.is_empty() {
            let wm_hits = wm_lexical_hits(cue, &self.agent.wm.slots(), k);
            if !wm_hits.is_empty() {
                lanes_used.push("working_memory".into());
                hits = wm_hits;
            }
        }

        if let Some(ref filt) = temporal {
            let tick_map: std::collections::HashMap<String, u64> = self
                .hippocampus
                .engrams
                .iter()
                .map(|e| (e.id.to_string(), e.encoded_at_tick))
                .collect();
            hits = filter_hits_by_tick(hits, &tick_map, filt);
            hits.truncate(k);
            if filt.from_tick.is_some() || filt.to_tick.is_some() {
                lanes_used.push("chronos".into());
            }
        } else {
            hits.truncate(k);
        }

        UnifiedRecallResult {
            mode,
            hits,
            lanes_used,
        }
    }

    /// Conflict lattice — pick the trusted fact for a cue.
    pub fn resolve(&self, cue: &str, cue_vector: Option<&[f32]>) -> ResolvedFact {
        let act = self.activate_scoped(cue, cue_vector, None, 12);
        resolve_from_recalls(cue, &act.recalls)
    }

    /// Idle consolidation — WM flush + CHORUS sleep + hippocampal sleep + retention.
    pub fn consolidate(&mut self) -> Result<ConsolidateReport> {
        let mut report = ConsolidateReport::default();
        let wm = self.flush_wm_internal()?;
        report.wm_flushed = wm.committed;

        if chorus_enabled() {
            report.chorus = Some(self.chorus_sleep()?);
        }
        report.hippocampal =
            Some(self.sleep_internal(false, crate::sleep_trigger::SleepTrigger::Manual)?);
        report.retention = self.apply_retention()?;
        self.agent.idle_consolidations += 1;
        self.agent.ticks_since_activity = 0;
        Ok(report)
    }

    pub(crate) fn touch_activity(&mut self) {
        self.agent.ticks_since_activity = 0;
    }

    pub(crate) fn agent_on_tick(&mut self) -> Result<Option<ConsolidateReport>> {
        if !self.agent.auto_consolidate || !agent_ergonomics_enabled() {
            return Ok(None);
        }
        self.agent.ticks_since_activity = self.agent.ticks_since_activity.saturating_add(1);
        if self.agent.ticks_since_activity < self.agent.idle_ticks_before_consolidate {
            return Ok(None);
        }
        // Light consolidate: chorus sleep + retention; full sleep via autonomic separately.
        let mut report = ConsolidateReport::default();
        if chorus_enabled() {
            report.chorus = Some(self.chorus_sleep()?);
        }
        report.retention = self.apply_retention()?;
        self.agent.idle_consolidations += 1;
        self.agent.ticks_since_activity = 0;
        Ok(Some(report))
    }

    pub(crate) fn agent_on_experience(&mut self, engram_id: Uuid) {
        self.agent
            .retention
            .record_engram(engram_id, self.autonomic.total_ticks);
        self.touch_activity();
    }
}

/// Apply agent-friendly env defaults (called from Python connect_agent).
pub fn enable_agent_env() {
    std::env::set_var("FLUCTLIGHT_AGENT_ERGONOMICS", "1");
    std::env::set_var("FLUCTLIGHT_CHORUS", "1");
    std::env::set_var("FLUCTLIGHT_CHORUS_FAST", "1");
    std::env::set_var("FLUCTLIGHT_FAST_INGEST", "1");
    std::env::remove_var("FLUCTLIGHT_VECTOR_FAST");
    std::env::set_var("FLUCTLIGHT_AGENT_FAST", "1");
    std::env::set_var("FLUCTLIGHT_CANDIDATE_CAP", "512");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recall_router::RecallMode;
    use crate::types::Episode;

    #[test]
    fn wm_flush_commits_slots() {
        let mut brain = FluctlightBrain::new();
        brain.wm_push("user likes dark mode", "settings", 0.8, None);
        let report = brain.flush_wm().unwrap();
        assert_eq!(report.committed, 1);
        assert!(brain.hippocampus.engrams.len() >= 1);
    }

    #[test]
    fn unified_recall_auto_episodic() {
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode::new("wallet balance is $42", "ledger:wallet", 0.9))
            .unwrap();
        let out = brain.recall_unified("wallet balance", None, RecallMode::Auto, 4, None);
        assert!(!out.hits.is_empty());
    }

    #[test]
    fn recall_unified_searches_wm_before_flush() {
        let mut brain = FluctlightBrain::new();
        brain.turn_begin();
        brain.wm_push("User prefers dark mode", "settings", 0.8, None);
        let out = brain.recall_unified("dark mode", None, RecallMode::Auto, 4, None);
        assert!(
            !out.hits.is_empty(),
            "expected WM lexical hit before turn_end flush: {:?}",
            out
        );
        assert!(
            out.lanes_used.iter().any(|l| l == "working_memory"),
            "expected working_memory lane: {:?}",
            out.lanes_used
        );
    }

    #[test]
    fn agent_env_wm_recall_without_semantic_vector() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        let prior: std::collections::HashMap<String, Option<String>> = [
            "FLUCTLIGHT_AGENT_ERGONOMICS",
            "FLUCTLIGHT_CHORUS",
            "FLUCTLIGHT_CHORUS_FAST",
            "FLUCTLIGHT_FAST_INGEST",
            "FLUCTLIGHT_VECTOR_FAST",
            "FLUCTLIGHT_AGENT_FAST",
            "FLUCTLIGHT_CANDIDATE_CAP",
        ]
        .into_iter()
        .map(|k| (k.to_string(), std::env::var(k).ok()))
        .collect();
        enable_agent_env();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.brain.flct");
        let mut brain = FluctlightBrain::open(&path).unwrap();
        brain.turn_begin();
        brain.wm_push("User prefers dark mode", "settings", 0.8, None);
        brain.turn_end(true).unwrap();
        let status = brain.status();
        assert!(
            status.synapses > 0,
            "expected graph synapses after wm_push without vector, got {:?}",
            status
        );
        let out = brain.recall_unified("dark mode", None, RecallMode::Auto, 8, None);
        assert!(
            !out.hits.is_empty(),
            "connect_agent-style env should recall lexical wm content: {:?}",
            out
        );
        for (k, v) in prior {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

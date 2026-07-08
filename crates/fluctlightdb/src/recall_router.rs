//! Unified recall router — one API, auto-picks episodic / corpus / session lanes.

use serde::{Deserialize, Serialize};

use crate::chorus::ChorusHit;
use crate::muon::MuonHit;
use crate::tau::TauHit;
use crate::types::{ActivationResult, RecallResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
    #[default]
    Auto,
    Episodic,
    Corpus,
    Session,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedRecallHit {
    pub memory_id: String,
    pub score: f32,
    pub lane: String,
    pub content: String,
    pub context: String,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engram_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnifiedRecallResult {
    pub mode: RecallMode,
    pub hits: Vec<UnifiedRecallHit>,
    pub lanes_used: Vec<String>,
}

impl UnifiedRecallHit {
    pub fn from_episodic(r: &RecallResult) -> Self {
        Self {
            memory_id: r.engram_id.to_string(),
            score: r.activation,
            lane: "episodic".into(),
            content: r.episode.content.clone(),
            context: r.episode.context.clone(),
            verified: r.verified,
            engram_id: Some(r.engram_id.to_string()),
            snippet: None,
        }
    }

    pub fn from_chorus(h: &ChorusHit) -> Self {
        Self {
            memory_id: h.memory_id.clone(),
            score: h.score,
            lane: h.lane.clone(),
            content: h.snippet.clone(),
            context: String::new(),
            verified: false,
            engram_id: None,
            snippet: Some(h.snippet.clone()),
        }
    }

    pub fn from_muon(h: &MuonHit) -> Self {
        Self {
            memory_id: h.session_id.clone(),
            score: h.score,
            lane: "muon".into(),
            content: h.snippet.clone(),
            context: h.session_id.clone(),
            verified: false,
            engram_id: None,
            snippet: Some(h.snippet.clone()),
        }
    }

    pub fn from_tau(h: &TauHit) -> Self {
        Self {
            memory_id: h.shard_id.clone(),
            score: h.score,
            lane: "tau".into(),
            content: h.content.clone(),
            context: h.session_id.clone(),
            verified: false,
            engram_id: None,
            snippet: Some(h.content.chars().take(280).collect()),
        }
    }
}

/// Merge and rerank hits from multiple lanes (RRF-lite).
pub fn merge_hits(mut lanes: Vec<Vec<UnifiedRecallHit>>, k: usize) -> Vec<UnifiedRecallHit> {
    let mut scores: std::collections::HashMap<String, (UnifiedRecallHit, f32)> =
        std::collections::HashMap::new();
    for lane_hits in lanes {
        for (rank, hit) in lane_hits.into_iter().enumerate() {
            let rrf = 1.0 / (60.0 + rank as f32);
            let combined = hit.score * 0.7 + rrf * 0.3;
            scores
                .entry(hit.memory_id.clone())
                .and_modify(|(prev, s)| {
                    if combined > *s {
                        *prev = hit.clone();
                        *s = combined;
                    }
                })
                .or_insert((hit, combined));
        }
    }
    let mut out: Vec<(f32, UnifiedRecallHit)> = scores
        .into_values()
        .map(|(h, s)| (s, h))
        .collect();
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter().take(k).map(|(_, h)| h).collect()
}

pub fn lanes_from_activation(act: &ActivationResult) -> Vec<UnifiedRecallHit> {
    act.recalls.iter().map(UnifiedRecallHit::from_episodic).collect()
}

/// Heuristic: pick recall mode from cue shape and brain fill levels.
pub fn choose_mode(
    cue: &str,
    engram_count: usize,
    chorus_len: usize,
    muon_len: usize,
    explicit: RecallMode,
) -> RecallMode {
    if explicit != RecallMode::Auto {
        return explicit;
    }
    let sessionish = cue.contains("session") || cue.len() < 24;
    if muon_len > 0 && sessionish {
        return RecallMode::Session;
    }
    if chorus_len > engram_count.saturating_mul(2) && chorus_len >= 64 {
        return RecallMode::Corpus;
    }
    if engram_count == 0 && chorus_len > 0 {
        return RecallMode::Corpus;
    }
    if engram_count > 0 && chorus_len > 0 {
        return RecallMode::Hybrid;
    }
    RecallMode::Episodic
}

/// Optional tick window for Chronos temporal gate on recall.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TemporalFilter {
    pub from_tick: Option<u64>,
    pub to_tick: Option<u64>,
}

/// Parse lightweight temporal hints from natural-language cues (LongMemEval / agent queries).
pub fn temporal_filter_from_cue(cue: &str, now_tick: u64) -> Option<TemporalFilter> {
    let low = cue.to_lowercase();
    let day = 86_400u64;
    if low.contains("yesterday") {
        return Some(TemporalFilter {
            from_tick: Some(now_tick.saturating_sub(day * 2)),
            to_tick: Some(now_tick.saturating_sub(day / 2)),
        });
    }
    if low.contains("last week") || low.contains("past week") {
        return Some(TemporalFilter {
            from_tick: Some(now_tick.saturating_sub(day * 8)),
            to_tick: Some(now_tick),
        });
    }
    if low.contains("last month") || low.contains("past month") {
        return Some(TemporalFilter {
            from_tick: Some(now_tick.saturating_sub(day * 35)),
            to_tick: Some(now_tick),
        });
    }
    if low.contains("recent") || low.contains("lately") || low.contains("just now") {
        return Some(TemporalFilter {
            from_tick: Some(now_tick.saturating_sub(day * 3)),
            to_tick: Some(now_tick),
        });
    }
    None
}

/// Filter unified hits by engram tick (Chronos gate).
pub fn filter_hits_by_tick(
    hits: Vec<UnifiedRecallHit>,
    tick_lookup: &std::collections::HashMap<String, u64>,
    filter: &TemporalFilter,
) -> Vec<UnifiedRecallHit> {
    if filter.from_tick.is_none() && filter.to_tick.is_none() {
        return hits;
    }
    hits.into_iter()
        .filter(|h| {
            let key = h.engram_id.as_deref().unwrap_or(&h.memory_id);
            let Some(tick) = tick_lookup.get(key) else {
                return true;
            };
            filter.from_tick.map(|f| *tick >= f).unwrap_or(true)
                && filter.to_tick.map(|to| *tick <= to).unwrap_or(true)
        })
        .collect()
}

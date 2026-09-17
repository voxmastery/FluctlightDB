//! Autonomous worldview agent — first-principles loop on Fluctlight primitives.
//!
//! # Research anchors (implementation inspiration, not claimed equivalence)
//! - **FluctlightDB** ([arXiv:2608.12365](https://arxiv.org/abs/2608.12365)):
//!   memory contract = `experience()` write + `activate()` cue-driven read.
//! - **Predictive processing / active inference** (Friston-style free-energy idea):
//!   keep a generative model of the world; on prediction error, **revise beliefs**
//!   (here: confidence-weighted claims), not only store raw text.
//! - **World models**: simulate futures then update latents — we use graph
//!   `preplay` / `PredictiveLoop::dream_step` as the simulator.
//! - **Global Workspace** (Baars; Blum & Blum CTM [arXiv:2011.09850](https://arxiv.org/abs/2011.09850)):
//!   broadcast a single winning content into a shared workspace each cycle.
//!
//! # What “fully autonomous worldview agent” means here
//! On autonomic tick, with **no user query**, the agent:
//! 1. Uses dream/predictive outcomes + cue-driven `activate` as perception
//! 2. Upserts **clean episodic beliefs** only (rejects meta/bookkeeping text)
//! 3. Broadcasts a single winning *world* claim into a GWT-style workspace
//! 4. Emits **open questions** (next cues) for subsequent cycles
//! 5. Persists worldview state in the `worldview` segment (not as hippocampal noise)
//!
//! Not claimed: literal consciousness or phenomenology.

use crate::attention_schema::AttentionSchema;
use crate::predictive_loop::{DreamReport, PredictiveLoop};
use crate::tokenize::tokenize;
use crate::types::{ActivationResult, Episode};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

const MAX_BELIEFS: usize = 64;
const MAX_QUESTIONS: usize = 24;
const MAX_WORKSPACE_HISTORY: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldBelief {
    pub claim: String,
    pub tokens: Vec<String>,
    /// 0..1 — raised by confirmations / consistent recalls; lowered by contradictions.
    pub confidence: f32,
    pub support_count: u32,
    pub contradict_count: u32,
    pub created_tick: u64,
    pub updated_tick: u64,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceBroadcast {
    pub tick: u64,
    pub content: String,
    pub kind: BroadcastKind,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastKind {
    Belief,
    Surprise,
    Question,
    Interpretation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldviewStepReport {
    pub tick: u64,
    pub perceived_cue: Option<String>,
    pub belief_upserts: u32,
    pub broadcast: Option<WorkspaceBroadcast>,
    pub new_questions: Vec<String>,
    pub belief_count: usize,
    pub narration: String,
}

/// Autonomous worldview state (persisted with the brain).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldviewAgent {
    pub enabled: bool,
    pub autonomous: bool,
    /// Run a worldview step every N autonomic ticks.
    pub every_n_ticks: u64,
    #[serde(default)]
    pub beliefs: VecDeque<WorldBelief>,
    #[serde(default)]
    pub open_questions: VecDeque<String>,
    pub workspace: Option<WorkspaceBroadcast>,
    #[serde(default)]
    pub workspace_history: VecDeque<WorkspaceBroadcast>,
    pub cycle_count: u64,
    pub belief_updates: u64,
    pub broadcasts: u64,
    #[serde(default)]
    ticks_since_step: u64,
}

impl Default for WorldviewAgent {
    fn default() -> Self {
        Self {
            enabled: true,
            autonomous: true,
            every_n_ticks: 1,
            beliefs: VecDeque::new(),
            open_questions: VecDeque::new(),
            workspace: None,
            workspace_history: VecDeque::new(),
            cycle_count: 0,
            belief_updates: 0,
            broadcasts: 0,
            ticks_since_step: 0,
        }
    }
}

impl WorldviewAgent {
    /// Advance the autonomic cadence gate. Returns true when a step should run.
    pub fn advance_autonomic_gate(&mut self) -> bool {
        if !self.enabled || !self.autonomous {
            return false;
        }
        self.ticks_since_step = self.ticks_since_step.saturating_add(1);
        if self.ticks_since_step < self.every_n_ticks.max(1) {
            return false;
        }
        self.ticks_since_step = 0;
        true
    }

    /// Autonomic hook — returns a step report when a cycle fires.
    ///
    /// Caller supplies `perceive: Option<(cue, ActivationResult)>` from
    /// `activate(cue)` after `select_cue()`, keeping borrows on `FluctlightBrain` simple.
    pub fn on_autonomic_tick(
        &mut self,
        tick: u64,
        attention: &AttentionSchema,
        predictive: &PredictiveLoop,
        dream: Option<&DreamReport>,
        perceive: Option<(String, ActivationResult)>,
    ) -> Option<WorldviewStepReport> {
        if !self.advance_autonomic_gate() {
            return None;
        }
        Some(self.step(tick, attention, predictive, dream, perceive))
    }

    /// Choose the next proactive cue (no user query).
    pub fn select_cue(
        &mut self,
        attention: &AttentionSchema,
        predictive: &PredictiveLoop,
    ) -> String {
        self.next_cue(attention, predictive)
            .unwrap_or_else(|| "what is currently true in this memory world".into())
    }

    /// One full worldview cycle.
    pub fn step(
        &mut self,
        tick: u64,
        attention: &AttentionSchema,
        predictive: &PredictiveLoop,
        dream: Option<&DreamReport>,
        perceive: Option<(String, ActivationResult)>,
    ) -> WorldviewStepReport {
        self.cycle_count = self.cycle_count.saturating_add(1);

        let mut belief_upserts = 0u32;
        let mut new_questions = Vec::new();
        let mut perceived_cue = None;

        // 1–2) Perceive via Fluctlight activate() — only clean episodic claims become beliefs.
        if let Some((c, result)) = perceive {
            perceived_cue = Some(c.clone());
            let mut clean_hits = 0u32;
            for recall in result.recalls.iter().take(8) {
                let raw = &recall.episode.content;
                if crate::predictive_loop::is_meta_episode_content(raw) {
                    continue;
                }
                let claim: String = raw.chars().take(160).collect();
                let delta = (0.05 + 0.08 * recall.activation.min(2.0) / 2.0).min(0.14);
                if self.upsert_belief(&claim, delta, tick, raw) {
                    belief_upserts += 1;
                    clean_hits += 1;
                }
            }
            if clean_hits == 0 {
                new_questions.push(format!("unresolved: {}", trunc(&c, 100)));
            }
        } else if let Some(c) = self.next_cue(attention, predictive) {
            if !crate::predictive_loop::is_meta_episode_content(&c) {
                new_questions.push(format!("investigate: {}", trunc(&c, 100)));
            }
        }

        // 3) Active inference: revise world claims from dream — never store meta narration as belief.
        let mut broadcast: Option<WorkspaceBroadcast> = None;
        if let Some(d) = dream {
            if d.surprise {
                if let Some(sim) = d.moment.simulated.as_ref() {
                    if let Some(clean) = crate::predictive_loop::clean_world_claim(sim) {
                        if self.upsert_belief(&clean, 0.10, tick, &clean) {
                            belief_upserts += 1;
                        }
                        if let Some(exp) = d.expectation.as_ref() {
                            if let Some(failed) =
                                crate::predictive_loop::clean_world_claim(&exp.summary)
                            {
                                belief_upserts += self.contradict_similar(&failed, -0.14, tick);
                            }
                        }
                        new_questions.push(format!(
                            "how does «{}» relate to prior beliefs",
                            trunc(&clean, 72)
                        ));
                        broadcast = Some(WorkspaceBroadcast {
                            tick,
                            content: clean,
                            kind: BroadcastKind::Surprise,
                            confidence: d.error.as_ref().map(|e| e.surprisal).unwrap_or(0.6),
                        });
                    } else {
                        new_questions.push("seek a non-meta observation after surprise".into());
                    }
                }
            } else if let Some(exp) = d.expectation.as_ref() {
                // Confirmations gently reinforce the clean expected claim only.
                if let Some(clean) = crate::predictive_loop::clean_world_claim(&exp.summary) {
                    if self.upsert_belief(&clean, 0.04, tick, &clean) {
                        belief_upserts += 1;
                    }
                }
            }
            // Interpretations stay in predictive_loop state — not belief store, not workspace text.
        }

        // 4) GWT-style winner: highest-confidence *clean* belief.
        if broadcast.is_none() {
            if let Some(top) = self
                .beliefs
                .iter()
                .filter(|b| !crate::predictive_loop::is_meta_episode_content(&b.claim))
                .max_by(|a, b| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            {
                broadcast = Some(WorkspaceBroadcast {
                    tick,
                    content: top.claim.clone(),
                    kind: BroadcastKind::Belief,
                    confidence: top.confidence,
                });
            }
        }

        if let Some(b) = broadcast.clone() {
            self.publish(b);
        }

        for q in new_questions.iter().cloned() {
            self.enqueue_question(q);
        }
        // Curiosity: verify weak clean beliefs; also probe gaps between top beliefs.
        if let Some(weak) = self
            .beliefs
            .iter()
            .filter(|b| {
                b.confidence < 0.50 && !crate::predictive_loop::is_meta_episode_content(&b.claim)
            })
            .min_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            self.enqueue_question(format!("verify: {}", trunc(&weak.claim, 100)));
        }
        if self.beliefs.len() >= 2 && self.open_questions.len() < 4 {
            let tops: Vec<&WorldBelief> = self
                .top_beliefs(2)
                .into_iter()
                .filter(|b| !crate::predictive_loop::is_meta_episode_content(&b.claim))
                .collect();
            if tops.len() == 2 {
                self.enqueue_question(format!(
                    "relate: «{}» and «{}»",
                    trunc(&tops[0].claim, 48),
                    trunc(&tops[1].claim, 48)
                ));
            }
        }

        let narration = format!(
            "t={tick}: worldview cycle#{} upserts={} beliefs={} questions={} workspace={}",
            self.cycle_count,
            belief_upserts,
            self.beliefs.len(),
            self.open_questions.len(),
            self.workspace
                .as_ref()
                .map(|w| trunc(&w.content, 48))
                .unwrap_or_else(|| "∅".into())
        );

        WorldviewStepReport {
            tick,
            perceived_cue,
            belief_upserts,
            broadcast: self.workspace.clone(),
            new_questions,
            belief_count: self.beliefs.len(),
            narration,
        }
    }

    pub fn worldview_snapshot_episode(&self, tick: u64) -> Option<Episode> {
        let top: Vec<String> = self
            .beliefs
            .iter()
            .take(5)
            .map(|b| format!("({}) {}", format_conf(b.confidence), trunc(&b.claim, 80)))
            .collect();
        if top.is_empty() {
            return None;
        }
        let ws = self
            .workspace
            .as_ref()
            .map(|w| trunc(&w.content, 100))
            .unwrap_or_default();
        Some(Episode {
            content: format!(
                "[worldview] tick={tick}; workspace=«{ws}»; beliefs: {}",
                top.join(" || ")
            ),
            context: "worldview_agent".into(),
            outcome: Some(format!(
                "beliefs={}; questions={}; cycles={}",
                self.beliefs.len(),
                self.open_questions.len(),
                self.cycle_count
            )),
            salience_hint: 0.7,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
    }

    pub fn top_beliefs(&self, k: usize) -> Vec<&WorldBelief> {
        let mut v: Vec<&WorldBelief> = self.beliefs.iter().collect();
        v.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v.truncate(k);
        v
    }

    /// Human-readable worldview status for agents / debugging.
    pub fn report_narration(&self) -> String {
        let top: Vec<String> = self
            .top_beliefs(5)
            .into_iter()
            .map(|b| format!("{:.2}:{}", b.confidence, trunc(&b.claim, 60)))
            .collect();
        let qs: Vec<String> = self
            .open_questions
            .iter()
            .take(5)
            .map(|q| trunc(q, 50))
            .collect();
        let ws = self
            .workspace
            .as_ref()
            .map(|w| trunc(&w.content, 80))
            .unwrap_or_else(|| "∅".into());
        format!(
            "worldview cycles={} beliefs={} updates={} broadcasts={} workspace=«{}» top=[{}] questions=[{}]",
            self.cycle_count,
            self.beliefs.len(),
            self.belief_updates,
            self.broadcasts,
            ws,
            top.join(" | "),
            qs.join(" | ")
        )
    }

    fn next_cue(
        &mut self,
        attention: &AttentionSchema,
        predictive: &PredictiveLoop,
    ) -> Option<String> {
        while let Some(q) = self.open_questions.pop_front() {
            if !crate::predictive_loop::is_meta_episode_content(&q) {
                return Some(q);
            }
        }
        if attention.attending {
            if let Some(s) = attention.spotlight.as_ref() {
                if !crate::predictive_loop::is_meta_episode_content(&s.summary) {
                    return Some(s.summary.clone());
                }
            }
        }
        if let Some(exp) = predictive.expectation.as_ref() {
            if let Some(clean) = crate::predictive_loop::clean_world_claim(&exp.summary) {
                return Some(clean);
            }
        }
        self.beliefs
            .iter()
            .find(|b| !crate::predictive_loop::is_meta_episode_content(&b.claim))
            .map(|b| b.claim.clone())
            .or_else(|| Some("what is currently true in this memory world".into()))
    }

    /// External reinforce from global-workspace broadcast.
    pub fn reinforce_claim(&mut self, claim: &str, delta: f32, tick: u64) -> bool {
        self.upsert_belief(claim, delta, tick, claim)
    }

    fn upsert_belief(&mut self, claim: &str, delta: f32, tick: u64, evidence: &str) -> bool {
        let claim = claim.trim();
        if claim.is_empty() || crate::predictive_loop::is_meta_episode_content(claim) {
            return false;
        }
        let tokens = unique_tokens(claim);
        if tokens.is_empty() {
            return false;
        }
        if let Some(existing) = self.beliefs.iter_mut().find(|b| {
            jaccard(&b.tokens, &tokens) >= 0.55 || b.claim.eq_ignore_ascii_case(claim)
        }) {
            apply_confidence_delta(existing, delta);
            if delta >= 0.0 {
                existing.support_count = existing.support_count.saturating_add(1);
            } else {
                existing.contradict_count = existing.contradict_count.saturating_add(1);
            }
            existing.updated_tick = tick;
            let ev: String = evidence.chars().take(120).collect();
            if !ev.is_empty() && !crate::predictive_loop::is_meta_episode_content(&ev) {
                existing.evidence.push_back_limited(ev, 6);
            }
            self.belief_updates = self.belief_updates.saturating_add(1);
            return true;
        }
        let mut belief = WorldBelief {
            claim: claim.chars().take(200).collect(),
            tokens,
            confidence: (0.32 + delta * 0.5).clamp(0.18, 0.58),
            support_count: if delta >= 0.0 { 1 } else { 0 },
            contradict_count: if delta < 0.0 { 1 } else { 0 },
            created_tick: tick,
            updated_tick: tick,
            evidence: Vec::new(),
        };
        let ev: String = evidence.chars().take(120).collect();
        if !ev.is_empty() && !crate::predictive_loop::is_meta_episode_content(&ev) {
            belief.evidence.push(ev);
        }
        self.beliefs.push_front(belief);
        while self.beliefs.len() > MAX_BELIEFS {
            self.beliefs.pop_back();
        }
        self.belief_updates = self.belief_updates.saturating_add(1);
        true
    }

    /// Lower confidence on beliefs similar to a failed expectation.
    fn contradict_similar(&mut self, failed_claim: &str, delta: f32, tick: u64) -> u32 {
        let tokens = unique_tokens(failed_claim);
        if tokens.is_empty() {
            return 0;
        }
        let mut n = 0u32;
        for b in self.beliefs.iter_mut() {
            if crate::predictive_loop::is_meta_episode_content(&b.claim) {
                continue;
            }
            if jaccard(&b.tokens, &tokens) >= 0.40 {
                apply_confidence_delta(b, delta);
                b.contradict_count = b.contradict_count.saturating_add(1);
                b.updated_tick = tick;
                self.belief_updates = self.belief_updates.saturating_add(1);
                n += 1;
            }
        }
        n
    }

    fn enqueue_question(&mut self, q: String) {
        let q = q.chars().take(160).collect::<String>();
        if q.is_empty() || crate::predictive_loop::is_meta_episode_content(&q) {
            return;
        }
        if self.open_questions.iter().any(|x| x == &q) {
            return;
        }
        self.open_questions.push_back(q);
        while self.open_questions.len() > MAX_QUESTIONS {
            self.open_questions.pop_front();
        }
    }

    fn publish(&mut self, b: WorkspaceBroadcast) {
        self.workspace = Some(b.clone());
        self.workspace_history.push_back(b);
        while self.workspace_history.len() > MAX_WORKSPACE_HISTORY {
            self.workspace_history.pop_front();
        }
        self.broadcasts = self.broadcasts.saturating_add(1);
    }
}

trait VecLimitedExt {
    fn push_back_limited(&mut self, item: String, max: usize);
}

impl VecLimitedExt for Vec<String> {
    fn push_back_limited(&mut self, item: String, max: usize) {
        self.push(item);
        while self.len() > max {
            self.remove(0);
        }
    }
}

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn apply_confidence_delta(belief: &mut WorldBelief, delta: f32) {
    let mut c = belief.confidence + delta;
    // Require repeated support before high certainty (coherence over saturation).
    if delta > 0.0 && belief.support_count < 3 {
        c = c.min(0.72);
    }
    if delta > 0.0 && belief.support_count < 5 {
        c = c.min(0.85);
    }
    belief.confidence = c.clamp(0.05, 0.92);
}

fn format_conf(c: f32) -> String {
    format!("{c:.2}")
}

fn unique_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for t in tokenize(text) {
        if seen.insert(t.clone()) {
            out.push(t);
        }
    }
    out
}

fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let aset: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let bset: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let inter = aset.intersection(&bset).count() as f32;
    let union = aset.union(&bset).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::FluctlightBrain;
    use crate::types::Episode;

    #[test]
    fn autonomous_step_updates_beliefs_without_user_query() {
        let mut brain = FluctlightBrain::new();
        for c in [
            "fluctlight stores episodic engrams",
            "prediction error raises salience",
            "attention schema models focus",
        ] {
            brain
                .experience(Episode {
                    content: c.into(),
                    context: "seed".into(),
                    outcome: None,
                    salience_hint: 0.85,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .unwrap();
        }
        let dream = brain.dream_step();
        let attn = brain.attention_schema.clone();
        let pred = brain.predictive_loop.clone();
        let mut agent = WorldviewAgent::default();
        let cue = agent.select_cue(&attn, &pred);
        let activation = brain.activate(&cue);
        let report = agent.step(
            1,
            &attn,
            &pred,
            Some(&dream),
            Some((cue, activation)),
        );
        assert!(report.belief_count >= 1 || report.belief_upserts >= 1);
        assert!(agent.workspace.is_some() || report.broadcast.is_some());
        assert!(agent.cycle_count >= 1);
    }
}

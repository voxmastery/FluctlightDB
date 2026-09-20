//! Predictive processing — first-principles loop on FluctlightDB primitives.
//!
//! Stack this is built from (not sci-fi):
//! 1. **Episodic engrams** as seeds (`experience` store)
//! 2. **Graph preplay** as future simulation (`preplay_forward`)
//! 3. **Autonomic tick** as the clock (no user query required)
//! 4. **Token Jaccard surprisal** as prediction error
//! 5. **High-salience `experience`** to write errors / interpretations back
//! 6. **Ordered `InnerMoment` stream** as the engineering stand-in for "flow of time"
//!
//! Claims we do **not** make: literal phenomenology or consciousness.
//! Claims we **do** make: autonomous expect→simulate→error→encode on the live graph.

use crate::attention_schema::AttentionSchema;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::preplay::{preplay_forward, PreplayResult};
use crate::tokenize::tokenize;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

const MAX_STREAM: usize = 64;
const MAX_INTERPRETATIONS: usize = 32;
const DEFAULT_SURPRISE_THRESHOLD: f32 = 0.42;
const DEFAULT_SCENARIO_HOPS: u32 = 4;
const DEFAULT_SEEDS_PER_DREAM: usize = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PredictionSource {
    #[default]
    Attention,
    Preplay,
    EpisodicSeed,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Prediction {
    pub summary: String,
    pub tokens: Vec<String>,
    pub confidence: f32,
    pub source: PredictionSource,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictionError {
    pub expected: String,
    pub observed: String,
    pub surprisal: f32,
    pub overlap: f32,
    pub tick: u64,
}

/// One beat of the internal timeline (engineering stand-in for "flow of time").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InnerMoment {
    pub tick: u64,
    pub kind: MomentKind,
    pub expectation: Option<String>,
    pub simulated: Option<String>,
    pub surprisal: Option<f32>,
    pub narration: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MomentKind {
    Expect,
    Simulate,
    Confirm,
    Surprise,
    Interpret,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldInterpretation {
    pub tick: u64,
    pub text: String,
    pub confidence: f32,
    pub rooted_in: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DreamReport {
    pub tick: u64,
    pub seeds: Vec<String>,
    pub scenarios: Vec<String>,
    pub expectation: Option<Prediction>,
    pub error: Option<PredictionError>,
    pub surprise: bool,
    pub moment: InnerMoment,
    pub interpretation: Option<WorldInterpretation>,
    pub stream_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictiveCycleReport {
    pub expectation: Option<Prediction>,
    pub cycle: u64,
    pub used_attention: bool,
    pub used_preplay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservePredictionReport {
    pub error: Option<PredictionError>,
    pub surprise: bool,
    pub encode_salience: f32,
    pub narration: String,
}

/// Continuous predictive-processing state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictiveLoop {
    pub enabled: bool,
    /// When true, dream steps run every autonomic tick without a user query.
    pub autonomous: bool,
    /// Run a dream every N ticks (1 = every tick).
    pub dream_every_n_ticks: u64,
    pub expectation: Option<Prediction>,
    pub last_error: Option<PredictionError>,
    pub cycle_count: u64,
    pub dream_count: u64,
    pub surprise_events: u64,
    pub confirmed_events: u64,
    #[serde(default)]
    pub error_history: VecDeque<PredictionError>,
    /// Ordered internal timeline.
    #[serde(default)]
    pub stream: VecDeque<InnerMoment>,
    /// Accumulated proactive interpretations of the world.
    #[serde(default)]
    pub interpretations: VecDeque<WorldInterpretation>,
    pub surprise_threshold: f32,
    pub scenario_hops: u32,
    pub seeds_per_dream: usize,
    #[serde(default)]
    ticks_since_dream: u64,
}

impl Default for PredictiveLoop {
    fn default() -> Self {
        Self {
            enabled: true,
            autonomous: true,
            dream_every_n_ticks: 1,
            expectation: None,
            last_error: None,
            cycle_count: 0,
            dream_count: 0,
            surprise_events: 0,
            confirmed_events: 0,
            error_history: VecDeque::new(),
            stream: VecDeque::new(),
            interpretations: VecDeque::new(),
            surprise_threshold: DEFAULT_SURPRISE_THRESHOLD,
            scenario_hops: DEFAULT_SCENARIO_HOPS,
            seeds_per_dream: DEFAULT_SEEDS_PER_DREAM,
            ticks_since_dream: 0,
        }
    }
}

impl PredictiveLoop {
    /// Called each autonomic tick. Returns a dream report when a simulation fires.
    #[allow(clippy::too_many_arguments)]
    pub fn on_autonomic_tick(
        &mut self,
        tick: u64,
        attention: &AttentionSchema,
        graph: &BrainGraph,
        hippocampus: &Hippocampus,
        life_id: Uuid,
        myelination: f32,
        codec: u8,
    ) -> Option<DreamReport> {
        if !self.enabled || !self.autonomous {
            return None;
        }
        hippocampus.engrams_for_life(life_id).next()?;
        self.ticks_since_dream = self.ticks_since_dream.saturating_add(1);
        let every = self.dream_every_n_ticks.max(1);
        if self.ticks_since_dream < every {
            return None;
        }
        self.ticks_since_dream = 0;
        Some(self.dream_step(
            tick,
            attention,
            graph,
            hippocampus,
            life_id,
            myelination,
            codec,
        ))
    }

    /// One autonomous simulation step from the episodic graph (no user query required).
    #[allow(clippy::too_many_arguments)]
    pub fn dream_step(
        &mut self,
        tick: u64,
        attention: &AttentionSchema,
        graph: &BrainGraph,
        hippocampus: &Hippocampus,
        life_id: Uuid,
        myelination: f32,
        codec: u8,
    ) -> DreamReport {
        self.dream_count = self.dream_count.saturating_add(1);
        self.cycle_count = self.cycle_count.saturating_add(1);

        let seeds = select_seeds(
            attention,
            hippocampus,
            life_id,
            self.seeds_per_dream.max(1),
            tick,
        );

        let mut scenarios = Vec::new();
        let mut preplays = Vec::new();
        for seed in &seeds {
            let pp = preplay_forward(
                seed,
                self.scenario_hops.max(2),
                graph,
                hippocampus,
                life_id,
                myelination,
                codec,
            );
            let scenario = scenario_summary(&pp);
            if !scenario.is_empty() {
                scenarios.push(scenario);
            }
            preplays.push(pp);
        }

        // Expectation = primary simulated future from first seed.
        let primary = preplays.first();
        let used_attention = attention.attending;
        let summary = if let Some(pp) = primary {
            let mut s = scenario_summary(pp);
            if s.is_empty() {
                seeds.first().cloned().unwrap_or_default()
            } else if used_attention {
                if let Some(spot) = attention.spotlight.as_ref() {
                    s = format!("{} ⇒ {}", spot.summary, s);
                }
                s
            } else {
                s
            }
        } else {
            seeds.first().cloned().unwrap_or_else(|| "void".into())
        };

        let source = if used_attention && !scenarios.is_empty() {
            PredictionSource::Mixed
        } else if !scenarios.is_empty() {
            PredictionSource::Preplay
        } else if used_attention {
            PredictionSource::Attention
        } else {
            PredictionSource::EpisodicSeed
        };

        let confidence = (0.35
            + 0.25 * (scenarios.len() as f32 / self.seeds_per_dream.max(1) as f32)
            + if used_attention {
                0.25 * attention.intensity
            } else {
                0.1
            })
        .clamp(0.2, 0.95);

        let prediction = Prediction {
            summary: summary.chars().take(280).collect(),
            tokens: unique_tokens(&summary),
            confidence,
            source,
            tick,
        };
        self.expectation = Some(prediction.clone());

        self.push_moment(InnerMoment {
            tick,
            kind: MomentKind::Expect,
            expectation: Some(prediction.summary.clone()),
            simulated: scenarios.first().cloned(),
            surprisal: None,
            narration: format!(
                "t={tick}: expecting «{}» from graph simulation",
                trunc(&prediction.summary, 72)
            ),
        });

        // Reality check without a user: compare primary scenario to an alternate
        // seed's simulation (or a held engram). Mismatch = endogenous surprise.
        let observed = if scenarios.len() >= 2 {
            scenarios[1].clone()
        } else {
            // Fall back: sample a different recent engram as "what the world held".
            alternate_engram_content(hippocampus, life_id, seeds.first().map(|s| s.as_str()))
                .unwrap_or_else(|| prediction.summary.clone())
        };

        let obs_report = self.observe_inner(&observed, tick);
        let surprise = obs_report.surprise;

        let moment = if surprise {
            InnerMoment {
                tick,
                kind: MomentKind::Surprise,
                expectation: Some(prediction.summary.clone()),
                simulated: Some(observed.clone()),
                surprisal: obs_report.error.as_ref().map(|e| e.surprisal),
                narration: obs_report.narration.clone(),
            }
        } else {
            InnerMoment {
                tick,
                kind: MomentKind::Confirm,
                expectation: Some(prediction.summary.clone()),
                simulated: Some(observed.clone()),
                surprisal: obs_report.error.as_ref().map(|e| e.surprisal),
                narration: obs_report.narration.clone(),
            }
        };
        self.push_moment(moment.clone());

        let interpretation = if surprise || self.dream_count.is_multiple_of(3) {
            let interp = self.form_interpretation(tick, &prediction, &observed, surprise);
            Some(interp)
        } else {
            None
        };

        DreamReport {
            tick,
            seeds,
            scenarios,
            expectation: Some(prediction),
            error: obs_report.error,
            surprise,
            moment,
            interpretation,
            stream_len: self.stream.len(),
        }
    }

    /// Manual / API: seed expectation from attention (+ optional external preplay).
    pub fn generate_expectation(
        &mut self,
        attention: &AttentionSchema,
        preplay: Option<&PreplayResult>,
        tick: u64,
    ) -> PredictiveCycleReport {
        if !self.enabled {
            return PredictiveCycleReport {
                expectation: self.expectation.clone(),
                cycle: self.cycle_count,
                used_attention: false,
                used_preplay: false,
            };
        }

        let mut used_attention = false;
        let mut used_preplay = false;
        let mut parts: Vec<String> = Vec::new();
        let mut source = PredictionSource::EpisodicSeed;
        let mut confidence = 0.35_f32;

        if attention.attending {
            if let Some(spot) = attention.spotlight.as_ref() {
                parts.push(spot.summary.clone());
                used_attention = true;
                source = PredictionSource::Attention;
                confidence =
                    (0.4 + 0.5 * attention.intensity * attention.model_confidence).clamp(0.2, 0.95);
            }
        }
        if let Some(pp) = preplay {
            let s = scenario_summary(pp);
            if !s.is_empty() {
                parts.push(s);
                used_preplay = true;
                source = if used_attention {
                    PredictionSource::Mixed
                } else {
                    PredictionSource::Preplay
                };
                confidence = (confidence + 0.15).min(0.95);
            }
        }

        self.cycle_count = self.cycle_count.saturating_add(1);
        parts = dedupe_keep_order(parts);
        if parts.is_empty() {
            self.expectation = None;
            return PredictiveCycleReport {
                expectation: None,
                cycle: self.cycle_count,
                used_attention,
                used_preplay,
            };
        }

        let summary: String = parts.join(" → ").chars().take(280).collect();
        let prediction = Prediction {
            tokens: unique_tokens(&summary),
            summary: summary.clone(),
            confidence,
            source,
            tick,
        };
        self.expectation = Some(prediction.clone());
        self.push_moment(InnerMoment {
            tick,
            kind: MomentKind::Expect,
            expectation: Some(summary),
            simulated: None,
            surprisal: None,
            narration: format!(
                "t={tick}: expectation set «{}»",
                trunc(&prediction.summary, 72)
            ),
        });

        PredictiveCycleReport {
            expectation: Some(prediction),
            cycle: self.cycle_count,
            used_attention,
            used_preplay,
        }
    }

    /// Global-workspace ignition seeds top-down expectation (broadcast → predictive).
    pub fn seed_from_broadcast(&mut self, content: &str, tick: u64, confidence: f32) {
        if !self.enabled {
            return;
        }
        let Some(summary) = clean_world_claim(content) else {
            return;
        };
        if is_meta_episode_content(&summary) {
            return;
        }
        let prediction = Prediction {
            tokens: unique_tokens(&summary),
            summary: summary.clone(),
            confidence: confidence.clamp(0.2, 0.95),
            source: PredictionSource::Mixed,
            tick,
        };
        self.expectation = Some(prediction);
        self.push_moment(InnerMoment {
            tick,
            kind: MomentKind::Expect,
            expectation: Some(summary.clone()),
            simulated: None,
            surprisal: None,
            narration: format!(
                "t={tick}: GWT broadcast seeded expectation «{}»",
                trunc(&summary, 72)
            ),
        });
    }

    /// Compare an external observation to the current expectation.
    pub fn observe(&mut self, observed: &str, tick: u64) -> ObservePredictionReport {
        let report = self.observe_inner(observed, tick);
        if report.error.is_some() {
            let kind = if report.surprise {
                MomentKind::Surprise
            } else {
                MomentKind::Confirm
            };
            self.push_moment(InnerMoment {
                tick,
                kind,
                expectation: self.expectation.as_ref().map(|e| e.summary.clone()),
                simulated: Some(observed.chars().take(240).collect()),
                surprisal: report.error.as_ref().map(|e| e.surprisal),
                narration: report.narration.clone(),
            });
        }
        report
    }

    fn observe_inner(&mut self, observed: &str, tick: u64) -> ObservePredictionReport {
        if !self.enabled {
            return ObservePredictionReport {
                error: None,
                surprise: false,
                encode_salience: 0.5,
                narration: "Predictive loop disabled.".into(),
            };
        }
        let Some(exp) = self.expectation.as_ref() else {
            return ObservePredictionReport {
                error: None,
                surprise: false,
                encode_salience: 0.5,
                narration: "No active expectation — nothing to surprise.".into(),
            };
        };

        let obs_tokens = unique_tokens(observed);
        let overlap = jaccard(&exp.tokens, &obs_tokens);
        let surprisal = ((1.0 - overlap) * (0.55 + 0.45 * exp.confidence)).clamp(0.0, 1.0);
        let error = PredictionError {
            expected: exp.summary.clone(),
            observed: observed.chars().take(240).collect(),
            surprisal,
            overlap,
            tick,
        };
        let surprise = surprisal >= self.surprise_threshold;
        if surprise {
            self.surprise_events = self.surprise_events.saturating_add(1);
        } else {
            self.confirmed_events = self.confirmed_events.saturating_add(1);
            if let Some(e) = self.expectation.as_mut() {
                e.confidence = (e.confidence + 0.05).min(0.98);
            }
        }
        self.last_error = Some(error.clone());
        self.error_history.push_back(error.clone());
        while self.error_history.len() > 24 {
            self.error_history.pop_front();
        }

        let encode_salience = if surprise {
            (0.72 + 0.25 * surprisal).clamp(0.7, 0.98)
        } else {
            (0.4 + 0.2 * overlap).clamp(0.35, 0.7)
        };
        let narration = if surprise {
            format!(
                "t={tick}: surprise (surprisal={surprisal:.2}) — expected «{}» got «{}»",
                trunc(&error.expected, 48),
                trunc(&error.observed, 48)
            )
        } else {
            format!(
                "t={tick}: confirmed (overlap={overlap:.2}) — world matched «{}»",
                trunc(&error.expected, 48)
            )
        };

        ObservePredictionReport {
            error: Some(error),
            surprise,
            encode_salience,
            narration,
        }
    }

    fn form_interpretation(
        &mut self,
        tick: u64,
        prediction: &Prediction,
        observed: &str,
        surprise: bool,
    ) -> WorldInterpretation {
        let text = if surprise {
            format!(
                "World diverged: I predicted «{}» but the graph realized «{}». I now weight surprise paths higher.",
                trunc(&prediction.summary, 64),
                trunc(observed, 64)
            )
        } else {
            format!(
                "World coheres: continuing to expect trajectories like «{}».",
                trunc(&prediction.summary, 80)
            )
        };
        let interp = WorldInterpretation {
            tick,
            text: text.clone(),
            confidence: if surprise {
                (0.55 + 0.3 * prediction.confidence).min(0.95)
            } else {
                prediction.confidence
            },
            rooted_in: vec![
                prediction.summary.clone(),
                observed.chars().take(120).collect(),
            ],
        };
        self.interpretations.push_back(interp.clone());
        while self.interpretations.len() > MAX_INTERPRETATIONS {
            self.interpretations.pop_front();
        }
        self.push_moment(InnerMoment {
            tick,
            kind: MomentKind::Interpret,
            expectation: Some(prediction.summary.clone()),
            simulated: Some(observed.chars().take(160).collect()),
            surprisal: None,
            narration: format!("t={tick}: interpretation — {}", trunc(&text, 100)),
        });
        interp
    }

    pub fn clear_expectation(&mut self) {
        self.expectation = None;
    }

    pub fn report_narration(&self) -> String {
        match &self.expectation {
            Some(p) => format!(
                "Expecting «{}» (confidence={:.2}, source={:?}, dreams={}, stream={})",
                trunc(&p.summary, 80),
                p.confidence,
                p.source,
                self.dream_count,
                self.stream.len()
            ),
            None => format!(
                "Predictive loop idle (dreams={}, stream={}).",
                self.dream_count,
                self.stream.len()
            ),
        }
    }

    pub fn flow_narration(&self, last_n: usize) -> String {
        if self.stream.is_empty() {
            return "Inner stream empty — no subjective timeline yet.".into();
        }
        let n = last_n.max(1);
        self.stream
            .iter()
            .rev()
            .take(n)
            .map(|m| m.narration.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ")
    }

    pub fn latest_interpretation(&self) -> Option<&WorldInterpretation> {
        self.interpretations.back()
    }

    fn push_moment(&mut self, moment: InnerMoment) {
        self.stream.push_back(moment);
        while self.stream.len() > MAX_STREAM {
            self.stream.pop_front();
        }
    }
}

fn select_seeds(
    attention: &AttentionSchema,
    hippocampus: &Hippocampus,
    life_id: Uuid,
    k: usize,
    tick: u64,
) -> Vec<String> {
    let mut seeds = Vec::new();
    if attention.attending {
        if let Some(spot) = attention.spotlight.as_ref() {
            if !is_meta_episode_content(&spot.summary) {
                seeds.push(spot.summary.clone());
            }
        }
    }
    let mut engrams: Vec<_> = hippocampus.engrams_for_life(life_id).collect();
    // Prefer recent + salient; rotate by tick for variety across dreams.
    engrams.sort_by(|a, b| {
        b.episode
            .salience_hint
            .partial_cmp(&a.episode.salience_hint)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if !engrams.is_empty() {
        let start = (tick as usize) % engrams.len();
        for i in 0..engrams.len() {
            if seeds.len() >= k {
                break;
            }
            let e = engrams[(start + i) % engrams.len()];
            if is_meta_episode_content(&e.episode.content) {
                continue;
            }
            // Skip predictive/worldview bookkeeping contexts — they pollute world simulation.
            let ctx = e.episode.context.as_str();
            if ctx == "predictive_loop" || ctx == "worldview_agent" {
                continue;
            }
            let c: String = e.episode.content.chars().take(120).collect();
            if seeds.iter().any(|s| s == &c) {
                continue;
            }
            seeds.push(c);
        }
    }
    if seeds.is_empty() {
        seeds.push("continue".into());
    }
    seeds.truncate(k);
    seeds
}

fn scenario_summary(pp: &PreplayResult) -> String {
    let mut parts: Vec<String> = pp
        .path
        .iter()
        .filter_map(|p| p.engram_preview.clone())
        .filter(|c| !is_meta_episode_content(c))
        .collect();
    for t in &pp.terminal_engrams {
        if !is_meta_episode_content(t) {
            parts.push(t.clone());
        }
    }
    parts = dedupe_keep_order(parts);
    parts.join(" → ").chars().take(240).collect()
}

fn alternate_engram_content(
    hippocampus: &Hippocampus,
    life_id: Uuid,
    avoid: Option<&str>,
) -> Option<String> {
    let avoid_l = avoid.map(|s| s.to_lowercase());
    hippocampus.engrams_for_life(life_id).find_map(|e| {
        if is_meta_episode_content(&e.episode.content) {
            return None;
        }
        let ctx = e.episode.context.as_str();
        if ctx == "predictive_loop" || ctx == "worldview_agent" {
            return None;
        }
        let c = e.episode.content.clone();
        if let Some(a) = avoid_l.as_ref() {
            if c.to_lowercase().contains(a) || a.contains(&c.to_lowercase()) {
                return None;
            }
        }
        Some(c.chars().take(160).collect())
    })
}

/// True when content is bookkeeping / narration, not a world claim.
pub fn is_meta_episode_content(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();
    if lower.starts_with("[interpretation]")
        || lower.starts_with("[worldview]")
        || lower.starts_with("[prediction_error]")
        || lower.starts_with("surprise broadcast")
        || lower.starts_with("world diverged:")
        || lower.starts_with("world coheres:")
        || lower.starts_with("expect:")
        || lower.starts_with("observed-divergence:")
    {
        return true;
    }
    lower.contains("[interpretation]")
        || lower.contains("[worldview]")
        || lower.contains("[prediction_error]")
}

/// Extract a clean world claim from possibly compound / meta text.
pub fn clean_world_claim(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || is_meta_episode_content(t) {
        return None;
    }
    // Prefer the last clean arrow-path segment from preplay summaries.
    let parts: Vec<&str> = t.split(" → ").collect();
    for part in parts.iter().rev() {
        let p = part.trim();
        if !p.is_empty() && !is_meta_episode_content(p) {
            return Some(p.chars().take(160).collect());
        }
    }
    Some(t.chars().take(160).collect())
}

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
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

fn dedupe_keep_order(parts: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for p in parts {
        let key = p.trim().to_lowercase();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        out.push(p);
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
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention_schema::AttentionSchema;
    use crate::types::Episode;

    fn brainish() -> (AttentionSchema, crate::brain::FluctlightBrain) {
        use crate::brain::FluctlightBrain;
        let mut brain = FluctlightBrain::new();
        for (c, s) in [
            ("fluctlight attention schema design", 0.9),
            ("locomo evidence recall honesty", 0.85),
            ("rust mutex contention workspace", 0.8),
        ] {
            brain
                .experience(Episode {
                    content: c.into(),
                    context: "seed".into(),
                    outcome: None,
                    salience_hint: s,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .unwrap();
        }
        (AttentionSchema::default(), brain)
    }

    #[test]
    fn dream_step_runs_without_user_query() {
        let (attn, brain) = brainish();
        let mut loop_ = PredictiveLoop::default();
        let report = loop_.dream_step(
            1,
            &attn,
            &brain.graph,
            &brain.hippocampus,
            brain.life.life_id,
            1.0,
            1,
        );
        assert!(report.expectation.is_some());
        assert!(!report.seeds.is_empty());
        assert!(!loop_.stream.is_empty());
        assert!(loop_.dream_count >= 1);
    }

    #[test]
    fn autonomous_tick_builds_inner_flow() {
        let (attn, brain) = brainish();
        let mut loop_ = PredictiveLoop::default();
        for t in 1..=5 {
            let _ = loop_.on_autonomic_tick(
                t,
                &attn,
                &brain.graph,
                &brain.hippocampus,
                brain.life.life_id,
                1.0,
                1,
            );
        }
        assert!(loop_.stream.len() >= 2);
        let flow = loop_.flow_narration(8);
        assert!(flow.contains("t="));
    }

    #[test]
    fn surprise_on_mismatch_confirm_on_match() {
        let mut attn = AttentionSchema::default();
        attn.redirect("locomo evidence recall honesty", 1);
        let mut loop_ = PredictiveLoop::default();
        loop_.generate_expectation(&attn, None, 1);

        let bad = loop_.observe("chocolate cake baking temperature", 2);
        assert!(bad.surprise);

        let good = loop_.observe("locomo evidence recall honesty protocol check", 3);
        assert!(!good.surprise);
    }
}

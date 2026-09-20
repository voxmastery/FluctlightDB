//! Global Neural Broadcasting — first-principles Global Workspace.
//!
//! # Spec (engineering, not phenomenology)
//! Global Workspace Theory (Baars) + CTM (Blum & Blum [arXiv:2011.09850](https://arxiv.org/abs/2011.09850)):
//! specialists compete; a winner ignites and is **broadcast to all systems**.
//!
//! Fluctlight first principles ([arXiv:2608.12365](https://arxiv.org/abs/2608.12365)):
//! - **Memory / sensory**: `activate()` over the episodic graph
//! - **Predictive**: dream / expectation loop
//! - **Worldview / goals / attention**: specialist modules
//! - **Graph buzz**: Hebbian `co_activate` so the graph is not a silent file
//! - **Present moment**: clash → merge → singular `now` stream each tick
//!
//! Not claimed: literal consciousness. Claimed: unified engineering "now" with
//! multi-receiver broadcast exactly as the upgrade stack requires.

use crate::attention_schema::AttentionSchema;
use crate::predictive_loop::{
    clean_world_claim, is_meta_episode_content, DreamReport, PredictiveLoop,
};
use crate::prefrontal::Prefrontal;
use crate::tokenize::tokenize;
use crate::types::ActivationResult;
use crate::worldview_agent::WorldviewAgent;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

const MAX_HISTORY: usize = 64;
const MAX_NOW_STREAM: usize = 64;
const MAX_CANDIDATES_LOG: usize = 24;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSource {
    Attention,
    Predictive,
    Worldview,
    Goal,
    Surprise,
    /// Episodic-graph / activate() recall — "memory & sensory" specialist.
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceCandidate {
    pub content: String,
    pub source: WorkspaceSource,
    pub activation: f32,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalBroadcast {
    pub content: String,
    pub source: WorkspaceSource,
    pub activation: f32,
    pub tick: u64,
    pub ignition: bool,
}

/// Singular integrated "present moment" after clash/merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentMoment {
    pub tick: u64,
    /// Melted singular now (memories + specialists fused).
    pub content: String,
    pub constituents: Vec<String>,
    pub sources: Vec<WorkspaceSource>,
    pub intensity: f32,
    pub clash_count: u32,
    pub merged: bool,
}

/// Proof that broadcast reached each specialist system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BroadcastReceipt {
    pub attention: bool,
    pub prefrontal: bool,
    pub neuromod: bool,
    pub graph: bool,
    pub predictive: bool,
    pub worldview: bool,
    pub receivers_hit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalWorkspaceReport {
    pub tick: u64,
    pub candidates: Vec<WorkspaceCandidate>,
    pub winner: Option<GlobalBroadcast>,
    pub now: Option<PresentMoment>,
    pub ignited: bool,
    pub receipt: BroadcastReceipt,
    pub narration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalWorkspace {
    pub enabled: bool,
    pub ignition_threshold: f32,
    /// Refractory ticks after ignition (prevents thrash; still continuous stream).
    pub refractory_ticks: u64,
    pub current: Option<GlobalBroadcast>,
    /// The unified present moment (singular "now").
    pub now: Option<PresentMoment>,
    #[serde(default)]
    pub now_stream: VecDeque<PresentMoment>,
    #[serde(default)]
    pub history: VecDeque<GlobalBroadcast>,
    #[serde(default)]
    pub last_candidates: Vec<WorkspaceCandidate>,
    pub cycle_count: u64,
    pub ignition_count: u64,
    pub clash_events: u64,
    pub merge_events: u64,
    #[serde(default)]
    refractory_until: u64,
}

impl Default for GlobalWorkspace {
    fn default() -> Self {
        Self {
            enabled: true,
            ignition_threshold: 0.45,
            refractory_ticks: 0,
            current: None,
            now: None,
            now_stream: VecDeque::new(),
            history: VecDeque::new(),
            last_candidates: Vec::new(),
            cycle_count: 0,
            ignition_count: 0,
            clash_events: 0,
            merge_events: 0,
            refractory_until: 0,
        }
    }
}

impl GlobalWorkspace {
    /// Gather candidates: specialists + episodic-graph activate() recalls.
    pub fn collect_candidates(
        tick: u64,
        attention: &AttentionSchema,
        predictive: &PredictiveLoop,
        dream: Option<&DreamReport>,
        worldview: &WorldviewAgent,
        prefrontal: &Prefrontal,
        sensory: Option<&ActivationResult>,
    ) -> Vec<WorkspaceCandidate> {
        let mut out = Vec::new();

        if attention.attending {
            if let Some(s) = attention.spotlight.as_ref() {
                if let Some(c) = clean_claim(&s.summary) {
                    out.push(WorkspaceCandidate {
                        content: c,
                        source: WorkspaceSource::Attention,
                        activation: (0.40 + 0.45 * attention.model_confidence.min(1.0)).min(0.95),
                        tick,
                    });
                }
            }
        }

        if let Some(d) = dream {
            if d.surprise {
                if let Some(sim) = d.moment.simulated.as_ref() {
                    if let Some(c) = clean_claim(sim) {
                        let surprisal = d.error.as_ref().map(|e| e.surprisal).unwrap_or(0.7);
                        out.push(WorkspaceCandidate {
                            content: c,
                            source: WorkspaceSource::Surprise,
                            activation: (0.55 + 0.40 * surprisal).min(1.10),
                            tick,
                        });
                    }
                }
            } else if let Some(exp) = d.expectation.as_ref() {
                if let Some(c) = clean_claim(&exp.summary) {
                    out.push(WorkspaceCandidate {
                        content: c,
                        source: WorkspaceSource::Predictive,
                        activation: (0.35 + 0.35 * exp.confidence).min(0.85),
                        tick,
                    });
                }
            }
        } else if let Some(exp) = predictive.expectation.as_ref() {
            if let Some(c) = clean_claim(&exp.summary) {
                out.push(WorkspaceCandidate {
                    content: c,
                    source: WorkspaceSource::Predictive,
                    activation: (0.30 + 0.30 * exp.confidence).min(0.80),
                    tick,
                });
            }
        }

        for b in worldview.top_beliefs(4) {
            if let Some(c) = clean_claim(&b.claim) {
                out.push(WorkspaceCandidate {
                    content: c,
                    source: WorkspaceSource::Worldview,
                    activation: (0.28 + 0.55 * b.confidence).min(0.95),
                    tick,
                });
            }
        }
        if let Some(ws) = worldview.workspace.as_ref() {
            if let Some(c) = clean_claim(&ws.content) {
                out.push(WorkspaceCandidate {
                    content: c,
                    source: WorkspaceSource::Worldview,
                    activation: (0.42 + 0.40 * ws.confidence).min(0.98),
                    tick,
                });
            }
        }

        for g in prefrontal.goals.iter().take(3) {
            if let Some(c) = clean_claim(&g.text) {
                out.push(WorkspaceCandidate {
                    content: c,
                    source: WorkspaceSource::Goal,
                    activation: (0.35 + 0.40 * g.salience.min(1.0)).min(0.90),
                    tick,
                });
            }
        }

        // Episodic graph is live: activate() recalls clash as Memory specialists.
        if let Some(act) = sensory {
            for (i, r) in act.recalls.iter().take(5).enumerate() {
                if let Some(c) = clean_claim(&r.episode.content) {
                    let act_n =
                        (0.38 + 0.12 * r.activation.min(2.0) - 0.03 * i as f32).clamp(0.2, 0.92);
                    out.push(WorkspaceCandidate {
                        content: c,
                        source: WorkspaceSource::Memory,
                        activation: act_n,
                        tick,
                    });
                }
            }
        }

        dedupe_candidates(out)
    }

    /// Compete, clash/merge into singular now, optionally ignite.
    pub fn compete(
        &mut self,
        tick: u64,
        candidates: Vec<WorkspaceCandidate>,
    ) -> (GlobalWorkspaceReport, bool) {
        self.cycle_count = self.cycle_count.saturating_add(1);
        self.last_candidates = candidates
            .iter()
            .take(MAX_CANDIDATES_LOG)
            .cloned()
            .collect();

        let in_refractory = tick < self.refractory_until;
        let winner = candidates
            .iter()
            .max_by(|a, b| {
                a.activation
                    .partial_cmp(&b.activation)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned();

        let clash_n = candidates.len().saturating_sub(1) as u32;
        if clash_n > 0 {
            self.clash_events = self.clash_events.saturating_add(1);
        }

        let mut ignited = false;
        let broadcast = if let Some(w) = winner.clone() {
            let can_ignite = !in_refractory && w.activation >= self.ignition_threshold;
            ignited = can_ignite;
            let gb = GlobalBroadcast {
                content: w.content.clone(),
                source: w.source,
                activation: w.activation,
                tick,
                ignition: can_ignite,
            };
            if can_ignite {
                self.ignition_count = self.ignition_count.saturating_add(1);
                self.current = Some(gb.clone());
                self.history.push_back(gb.clone());
                while self.history.len() > MAX_HISTORY {
                    self.history.pop_front();
                }
                if self.refractory_ticks > 0 {
                    self.refractory_until = tick.saturating_add(self.refractory_ticks);
                }
            } else if self.current.is_none() {
                self.current = Some(gb.clone());
            }
            Some(gb)
        } else {
            None
        };

        // Melt memories + specialists into one present moment (even without ignition).
        let now = if let Some(w) = winner.as_ref() {
            let moment = forge_present_moment(tick, w, &candidates, self.now.as_ref());
            if moment.merged {
                self.merge_events = self.merge_events.saturating_add(1);
            }
            self.now = Some(moment.clone());
            self.now_stream.push_back(moment.clone());
            while self.now_stream.len() > MAX_NOW_STREAM {
                self.now_stream.pop_front();
            }
            Some(moment)
        } else {
            None
        };

        let narration = match (&broadcast, &now) {
            (Some(b), Some(n)) if b.ignition => format!(
                "t={tick}: GWT IGNITION now=«{}» source={:?} act={:.2} clash={} receivers=pending",
                trunc(&n.content, 56),
                b.source,
                b.activation,
                clash_n
            ),
            (_, Some(n)) => format!(
                "t={tick}: present-moment «{}» intensity={:.2} clash={} (subthreshold)",
                trunc(&n.content, 56),
                n.intensity,
                clash_n
            ),
            _ => format!("t={tick}: GWT idle — empty stage"),
        };

        let report = GlobalWorkspaceReport {
            tick,
            candidates: self.last_candidates.clone(),
            winner: broadcast,
            now,
            ignited,
            receipt: BroadcastReceipt::default(),
            narration,
        };
        (report, ignited)
    }

    pub fn attach_receipt(
        &mut self,
        report: &mut GlobalWorkspaceReport,
        receipt: BroadcastReceipt,
    ) {
        report.receipt = receipt;
        if report.ignited {
            report.narration = format!(
                "{} | broadcast→attn={} pfc={} nm={} graph={} pred={} wv={} hits={}",
                trunc(&report.narration, 90),
                report.receipt.attention,
                report.receipt.prefrontal,
                report.receipt.neuromod,
                report.receipt.graph,
                report.receipt.predictive,
                report.receipt.worldview,
                report.receipt.receivers_hit
            );
        }
    }

    pub fn report_narration(&self) -> String {
        match &self.now {
            Some(n) => format!(
                "NOW «{}» intensity={:.2} ignitions={} clashes={} merges={} cycles={} stream={}",
                trunc(&n.content, 72),
                n.intensity,
                self.ignition_count,
                self.clash_events,
                self.merge_events,
                self.cycle_count,
                self.now_stream.len()
            ),
            None => format!(
                "GWT idle (cycles={}, ignitions={})",
                self.cycle_count, self.ignition_count
            ),
        }
    }

    pub fn now_flow(&self, last_n: usize) -> String {
        self.now_stream
            .iter()
            .rev()
            .take(last_n)
            .map(|n| format!("t={}: «{}»", n.tick, trunc(&n.content, 48)))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Clash then melt into a singular present moment.
fn forge_present_moment(
    tick: u64,
    winner: &WorkspaceCandidate,
    candidates: &[WorkspaceCandidate],
    prev: Option<&PresentMoment>,
) -> PresentMoment {
    let mut ranked: Vec<&WorkspaceCandidate> = candidates.iter().collect();
    ranked.sort_by(|a, b| {
        b.activation
            .partial_cmp(&a.activation)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut constituents = Vec::new();
    let mut sources = Vec::new();
    constituents.push(winner.content.clone());
    sources.push(winner.source);

    let win_tok = token_set(&winner.content);
    let mut merged = false;
    let mut clash_count = 0u32;

    for c in ranked.iter().skip(1).take(4) {
        clash_count += 1;
        let ov = jaccard(&win_tok, &token_set(&c.content));
        if ov >= 0.22
            && !constituents
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&c.content))
        {
            constituents.push(c.content.clone());
            if !sources.contains(&c.source) {
                sources.push(c.source);
            }
            merged = true;
        }
    }

    if let Some(p) = prev {
        let pov = jaccard(&win_tok, &token_set(&p.content));
        if pov >= 0.18
            && !constituents
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&p.content))
        {
            // Continuity of "now": prior moment residue melts in.
            constituents.push(trunc(&p.content, 120));
            merged = true;
        } else if pov < 0.12 {
            clash_count += 1; // sharp present-moment transition
        }
    }

    let content = if constituents.len() == 1 {
        winner.content.clone()
    } else {
        // Singular fused now: winner first, then related strands.
        let rest: Vec<String> = constituents
            .iter()
            .skip(1)
            .take(2)
            .map(|s| trunc(s, 64))
            .collect();
        format!("{} · {}", trunc(&winner.content, 100), rest.join(" · "))
            .chars()
            .take(220)
            .collect()
    };

    PresentMoment {
        tick,
        content,
        constituents,
        sources,
        intensity: winner.activation.min(1.0),
        clash_count,
        merged,
    }
}

fn clean_claim(s: &str) -> Option<String> {
    clean_world_claim(s).filter(|c| !is_meta_episode_content(c))
}

fn dedupe_candidates(mut v: Vec<WorkspaceCandidate>) -> Vec<WorkspaceCandidate> {
    let mut out: Vec<WorkspaceCandidate> = Vec::new();
    for c in v.drain(..) {
        if let Some(existing) = out
            .iter_mut()
            .find(|e| e.content.eq_ignore_ascii_case(&c.content))
        {
            if c.activation > existing.activation {
                *existing = c;
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn token_set(s: &str) -> HashSet<String> {
    tokenize(s).into_iter().collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let uni = a.union(b).count() as f32;
    if uni <= 0.0 {
        0.0
    } else {
        inter / uni
    }
}

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clash_merge_forges_singular_now() {
        let mut gwt = GlobalWorkspace::default();
        let (report, ignited) = gwt.compete(
            1,
            vec![
                WorkspaceCandidate {
                    content: "harbor beacon flashes before fog".into(),
                    source: WorkspaceSource::Memory,
                    activation: 0.9,
                    tick: 1,
                },
                WorkspaceCandidate {
                    content: "ships wait for harbor beacon in fog".into(),
                    source: WorkspaceSource::Worldview,
                    activation: 0.7,
                    tick: 1,
                },
                WorkspaceCandidate {
                    content: "keep ships safe in fog".into(),
                    source: WorkspaceSource::Goal,
                    activation: 0.5,
                    tick: 1,
                },
            ],
        );
        assert!(ignited);
        let now = report.now.expect("now");
        assert!(now.content.contains("harbor"));
        assert!(now.clash_count >= 1 || now.merged);
        assert!(gwt.now.is_some());
    }
}

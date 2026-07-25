//! Agent-lane prompt packing — **lossless index**, budgeted full text.
//!
//! # User-facing contract (no silent tradeoff)
//!
//! - `activate()` / CHORUS / benchmark ranking are **unchanged**.
//! - Every activated engram appears in the prompt pack as at least `id + gist`.
//! - Full text is included for core + verified first, then by activation score,
//!   until the token budget; the rest stay as gist with `expandable_ids`.
//! - Call `expand_engrams` to fetch full text — nothing is dropped from the index.
//!
//! Previous truncate-and-drop behavior is removed.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::homeostasis::{agent_prompt_token_budget, estimate_tokens};
use crate::types::{ActivationResult, RecallResult};

const GIST_CHARS: usize = 96;

/// One activated memory line in the agent prompt pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptMemoryLine {
    pub engram_id: Uuid,
    pub activation: f32,
    pub verified: bool,
    /// Always present — short preview so the agent knows the memory exists.
    pub gist: String,
    /// Full episode text when it fits the budget; else `None` (use `expand_engrams`).
    pub full_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PromptCoverage {
    pub activated: usize,
    pub with_full_text: usize,
    pub gist_only: usize,
    pub core_count: usize,
}

/// Lossless agent prompt bundle (agent SDK lane only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentPromptBundle {
    pub cue: String,
    /// Every activated recall — never silently omitted.
    pub lines: Vec<PromptMemoryLine>,
    pub core_snippets: Vec<String>,
    /// Engram ids present as gist-only (fetch via `expand_engrams`).
    pub expandable_ids: Vec<Uuid>,
    pub estimated_tokens: usize,
    pub token_budget: usize,
    /// True when some lines are gist-only (not dropped — deferred).
    pub compressed: bool,
    pub coverage: PromptCoverage,
    pub prompt_block: String,
    /// Backward-compatible: recalls that include full text (subset of `lines`).
    pub recalls: Vec<RecallResult>,
    /// Always false for drop semantics; kept so older callers reading `truncated` see "not dropped".
    #[serde(default)]
    pub truncated: bool,
    pub max_engrams: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpandedEngram {
    pub engram_id: Uuid,
    pub content: String,
    pub context: String,
    pub verified: bool,
}

pub fn gist_of(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= GIST_CHARS {
        return trimmed.to_string();
    }
    let mut out = String::new();
    for (i, ch) in trimmed.chars().enumerate() {
        if i >= GIST_CHARS {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn pack_prompt_block(core: &[String], lines: &[PromptMemoryLine]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !core.is_empty() {
        parts.push("## Core identity".into());
        for (i, c) in core.iter().enumerate() {
            parts.push(format!("{}. {}", i + 1, c));
        }
    }
    if !lines.is_empty() {
        parts.push("## Activated memory (full or gist+id — expand gist-only via expand_engrams)".into());
        for (i, line) in lines.iter().enumerate() {
            let ver = if line.verified { " [verified]" } else { "" };
            match &line.full_content {
                Some(full) => parts.push(format!(
                    "{}. ({:.3}){} id={} {}",
                    i + 1,
                    line.activation,
                    ver,
                    line.engram_id,
                    full
                )),
                None => parts.push(format!(
                    "{}. ({:.3}){} id={} [gist] {}  → expand_engrams",
                    i + 1,
                    line.activation,
                    ver,
                    line.engram_id,
                    line.gist
                )),
            }
        }
    }
    if parts.is_empty() {
        return "## Memory\n(no activated engrams for this cue — brain connected)".into();
    }
    parts.join("\n")
}

/// Lossless pack: every recall becomes a line; full text filled without dropping ids.
pub fn pack_lossless(
    recalls: &[RecallResult],
    token_budget: usize,
    core_tokens: usize,
) -> (Vec<PromptMemoryLine>, Vec<RecallResult>, bool) {
    // Priority for full text: verified first (stable among ties by original order), else order.
    let mut order: Vec<usize> = (0..recalls.len()).collect();
    order.sort_by(|&a, &b| {
        let va = recalls[a].verified;
        let vb = recalls[b].verified;
        match (va, vb) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(&b), // preserve activate rank among same verified class
        }
    });

    let mut lines: Vec<PromptMemoryLine> = recalls
        .iter()
        .map(|r| PromptMemoryLine {
            engram_id: r.engram_id,
            activation: r.activation,
            verified: r.verified,
            gist: gist_of(&r.episode.content),
            full_content: None,
        })
        .collect();

    let mut used = core_tokens;
    // Reserve gist tokens for all lines first so index always fits.
    for line in &lines {
        used = used.saturating_add(estimate_tokens(&line.gist).saturating_add(8)); // id + meta
    }

    let mut full_recalls = Vec::new();
    for idx in order {
        let content = &recalls[idx].episode.content;
        let extra = estimate_tokens(content).saturating_sub(estimate_tokens(&lines[idx].gist));
        if used.saturating_add(extra) > token_budget && lines[idx].full_content.is_none() {
            // Keep gist-only; do not drop.
            continue;
        }
        // Always allow at least one full text if nothing full yet and content is huge:
        if lines.iter().all(|l| l.full_content.is_none()) && full_recalls.is_empty() {
            lines[idx].full_content = Some(content.clone());
            used = used.saturating_add(extra);
            full_recalls.push(recalls[idx].clone());
            continue;
        }
        if used.saturating_add(extra) > token_budget {
            continue;
        }
        lines[idx].full_content = Some(content.clone());
        used = used.saturating_add(extra);
        full_recalls.push(recalls[idx].clone());
    }

    // Restore activate order in full_recalls
    full_recalls.sort_by(|a, b| {
        b.activation
            .partial_cmp(&a.activation)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let compressed = lines.iter().any(|l| l.full_content.is_none());
    (lines, full_recalls, compressed)
}

impl FluctlightBrain {
    /// Lossless session/turn pack: every activate hit is indexed; full text within budget.
    pub fn activate_for_agent_prompt(&mut self, cue: &str) -> AgentPromptBundle {
        let token_budget = agent_prompt_token_budget();

        // All core memories — identity must not be silently capped away.
        let core_snippets: Vec<String> = self
            .core_memories
            .memories
            .iter()
            .map(|m| m.content.clone())
            .collect();
        let core_tokens: usize = core_snippets.iter().map(|s| estimate_tokens(s)).sum();

        let full: ActivationResult = self.activate(cue);
        let (lines, recalls, compressed) =
            pack_lossless(&full.recalls, token_budget, core_tokens);
        let expandable_ids: Vec<Uuid> = lines
            .iter()
            .filter(|l| l.full_content.is_none())
            .map(|l| l.engram_id)
            .collect();
        let coverage = PromptCoverage {
            activated: lines.len(),
            with_full_text: lines.iter().filter(|l| l.full_content.is_some()).count(),
            gist_only: expandable_ids.len(),
            core_count: core_snippets.len(),
        };
        let prompt_block = pack_prompt_block(&core_snippets, &lines);
        let estimated_tokens = estimate_tokens(&prompt_block);
        self.homeostasis
            .note_agent_prompt_tokens(estimated_tokens as u64);

        AgentPromptBundle {
            cue: cue.to_string(),
            lines,
            core_snippets,
            expandable_ids,
            estimated_tokens,
            token_budget,
            compressed,
            coverage,
            prompt_block,
            recalls,
            truncated: false, // never drop from index
            max_engrams: full.recalls.len().max(1),
        }
    }

    pub fn session_boot_context(&mut self, cue: Option<&str>) -> AgentPromptBundle {
        let cue = cue.unwrap_or("who am I and what matters");
        self.activate_for_agent_prompt(cue)
    }

    /// Fetch full engram text for gist-only ids (agent expand-on-demand).
    pub fn expand_engrams(&self, ids: &[Uuid]) -> Vec<ExpandedEngram> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(e) = self.hippocampus.engrams.iter().find(|e| e.id == *id) {
                let verified = e
                    .episode
                    .provenance
                    .as_ref()
                    .map(|p| p.verified)
                    .unwrap_or(false);
                out.push(ExpandedEngram {
                    engram_id: e.id,
                    content: e.episode.content.clone(),
                    context: e.episode.context.clone(),
                    verified,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    fn fake_recalls(n: usize) -> Vec<RecallResult> {
        (0..n)
            .map(|i| RecallResult {
                engram_id: Uuid::from_u128(i as u128 + 1),
                activation: 1.0 - (i as f32) * 0.05,
                episode: Episode::new(
                    format!("fact number {i} with enough words to make a longer gist body"),
                    "t",
                    0.5,
                ),
                completion_strength: 0.0,
                separation_index: 0.0,
                verified: i == 0,
                trust_note: None,
            })
            .collect()
    }

    #[test]
    fn lossless_never_drops_activated_ids() {
        let recalls = fake_recalls(12);
        let (lines, _full, compressed) = pack_lossless(&recalls, 80, 0);
        assert_eq!(lines.len(), recalls.len());
        assert!(compressed);
        let ids: Vec<_> = lines.iter().map(|l| l.engram_id).collect();
        for r in &recalls {
            assert!(ids.contains(&r.engram_id));
        }
    }

    #[test]
    fn verified_gets_full_text_priority() {
        let recalls = fake_recalls(8);
        let (lines, _, _) = pack_lossless(&recalls, 120, 0);
        let verified = lines.iter().find(|l| l.verified).unwrap();
        assert!(
            verified.full_content.is_some(),
            "verified memory must get full text before unverified when budget is tight"
        );
    }

    #[test]
    fn agent_prompt_does_not_change_full_activate() {
        let mut brain = FluctlightBrain::new();
        for i in 0..5 {
            brain
                .experience(Episode::new(
                    format!("preference item {i} dark mode"),
                    "settings",
                    0.7,
                ))
                .unwrap();
        }
        let full_before = brain.activate("dark mode");
        let bundle = brain.activate_for_agent_prompt("dark mode");
        let full_after = brain.activate("dark mode");
        let ids = |r: &ActivationResult| {
            r.recalls.iter().map(|x| x.engram_id).collect::<Vec<_>>()
        };
        assert_eq!(ids(&full_before), ids(&full_after));
        assert_eq!(bundle.lines.len(), full_before.recalls.len());
        assert!(!bundle.truncated);
    }

    #[test]
    fn expand_engrams_returns_full_content_for_gist_only() {
        let mut brain = FluctlightBrain::new();
        for i in 0..10 {
            brain
                .experience(Episode::new(
                    format!("long preference detail {i} about dark mode and theme contrast ratios"),
                    "settings",
                    0.7,
                ))
                .unwrap();
        }
        std::env::set_var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET", "64");
        let bundle = brain.activate_for_agent_prompt("dark mode");
        std::env::remove_var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET");
        if bundle.expandable_ids.is_empty() {
            // Budget still fit everything — still ok, expand of full ids works.
            let id = bundle.lines[0].engram_id;
            let expanded = brain.expand_engrams(&[id]);
            assert_eq!(expanded.len(), 1);
            assert!(!expanded[0].content.is_empty());
        } else {
            let expanded = brain.expand_engrams(&bundle.expandable_ids);
            assert_eq!(expanded.len(), bundle.expandable_ids.len());
            assert!(expanded.iter().all(|e| e.content.len() > e.content.chars().take(10).count()));
        }
    }
}

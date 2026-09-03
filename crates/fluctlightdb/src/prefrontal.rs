use crate::tokenize::tokenize;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Default salience for a freshly-set goal — how strongly it biases recall.
const DEFAULT_GOAL_SALIENCE: f32 = 0.35;
/// Default per-tick decay of a goal's salience (working memory fades).
const DEFAULT_GOAL_DECAY: f32 = 0.01;
/// Default suppression strength for a new inhibit pattern.
const DEFAULT_INHIBIT_STRENGTH: f32 = 0.6;
/// Per-tick decay of task-context strength when not reinforced.
const TASK_DECAY_PER_TICK: f32 = 0.02;

/// Upper bound on goal-biased recall boost (don't overwhelm CHORUS scores).
const MAX_GOAL_BOOST: f32 = 0.5;
/// Lower bound on inhibitory suppression (escape hatch for very strong recalls).
const MIN_INHIBIT_SCORE: f32 = -0.8;

/// A goal held in working memory. Top-down goal signals bias hippocampal recall
/// toward goal-relevant engrams (Miller & Cohen 2001).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalEntry {
    pub text: String,
    /// How much to boost goal-matching engrams.
    pub salience: f32,
    /// Pre-tokenized goal terms for fast matching.
    pub tokens: Vec<String>,
    pub created_tick: u64,
    /// Goals fade if not reinforced (working-memory maintenance cost).
    pub decay_rate: f32,
}

/// An inhibitory-control pattern. The PFC suppresses conflicting / irrelevant
/// recall (Aron 2007) — engrams matching these tokens are down-scored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InhibitEntry {
    pub pattern: String,
    pub tokens: Vec<String>,
    /// How strongly to suppress matching engrams (0..1).
    pub strength: f32,
}

/// Abstract if/then routing rule that fires during recall (rule representation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PfcRule {
    /// If the cue contains these tokens, the rule fires.
    pub condition_tokens: Vec<String>,
    pub action: RuleAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuleAction {
    /// Surface verified engrams first.
    BoostVerified,
    /// Only keep engrams whose provenance source matches this string.
    RequireSource(String),
    /// Add this text to the recall context (top-down priming).
    InjectContext(String),
}

/// The task the agent is currently doing. Biases all recall toward the task's
/// needs even when individual queries don't mention it. Decays if not reinforced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskContext {
    pub description: String,
    pub tokens: Vec<String>,
    pub strength: f32,
    pub last_reinforced_tick: u64,
}

/// Executive region — goals, inhibition, rules, task context (unlocks at
/// Adolescent stage). Implements top-down goal-biased recall and inhibitory
/// control over the hippocampal read path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefrontal {
    pub unlocked: bool,

    /// Active goals — held in working memory, bias all recall.
    pub goals: Vec<GoalEntry>,

    /// Inhibitory control — content/actions to suppress.
    pub inhibit_patterns: Vec<InhibitEntry>,

    /// Rule set — explicit if/then routing rules.
    pub rules: Vec<PfcRule>,

    /// Task context — current active task (decays if not reinforced).
    pub task_context: Option<TaskContext>,
}

impl Prefrontal {
    /// Register a goal in working memory. Deduplicates by text.
    pub fn add_goal(&mut self, text: String, tick: u64) {
        if text.is_empty() || self.goals.iter().any(|g| g.text == text) {
            return;
        }
        let tokens = tokenize(&text);
        self.goals.push(GoalEntry {
            text,
            salience: DEFAULT_GOAL_SALIENCE,
            tokens,
            created_tick: tick,
            decay_rate: DEFAULT_GOAL_DECAY,
        });
    }

    /// Register an inhibitory pattern. Deduplicates by pattern.
    pub fn add_inhibit(&mut self, pattern: String) {
        if pattern.is_empty() || self.inhibit_patterns.iter().any(|i| i.pattern == pattern) {
            return;
        }
        let tokens = tokenize(&pattern);
        self.inhibit_patterns.push(InhibitEntry {
            pattern,
            tokens,
            strength: DEFAULT_INHIBIT_STRENGTH,
        });
    }

    /// Register an if/then routing rule.
    pub fn add_rule(&mut self, condition: &str, action: RuleAction) {
        self.rules.push(PfcRule {
            condition_tokens: tokenize(condition),
            action,
        });
    }

    /// Set the current task context (resets decay timer).
    pub fn set_task_context(&mut self, description: &str, tick: u64) {
        self.task_context = Some(TaskContext {
            description: description.to_string(),
            tokens: tokenize(description),
            strength: 1.0,
            last_reinforced_tick: tick,
        });
    }

    /// Refresh the task-context decay timer (keep the current task alive).
    pub fn reinforce_task(&mut self, tick: u64) {
        if let Some(tc) = self.task_context.as_mut() {
            tc.strength = 1.0;
            tc.last_reinforced_tick = tick;
        }
    }

    /// Goal-biased recall: returns a [0, MAX_GOAL_BOOST] boost for engrams whose
    /// content matches active goals. The boost is stronger when the cue itself is
    /// goal-relevant, and task context contributes a background bias so that recall
    /// stays task-focused even when the query doesn't mention the task.
    pub fn goal_bias_score(&self, engram_content: &str, cue: &str) -> f32 {
        if !self.unlocked {
            return 0.0;
        }
        let content_tokens: HashSet<String> = tokenize(engram_content).into_iter().collect();
        if content_tokens.is_empty() {
            return 0.0;
        }
        let cue_tokens: HashSet<String> = tokenize(cue).into_iter().collect();
        let mut boost = 0.0_f32;

        for g in &self.goals {
            if g.tokens.is_empty() {
                continue;
            }
            let matched = g
                .tokens
                .iter()
                .filter(|t| content_tokens.contains(*t))
                .count();
            if matched == 0 {
                continue;
            }
            let frac = matched as f32 / g.tokens.len() as f32;
            // A goal is more active when the query also relates to it.
            let cue_relevance = if g.tokens.iter().any(|t| cue_tokens.contains(t)) {
                1.0
            } else {
                0.6
            };
            boost += frac * g.salience * cue_relevance;
        }

        // Task context adds a background bias toward the current task's terms.
        if let Some(tc) = &self.task_context {
            if !tc.tokens.is_empty() {
                let matched = tc
                    .tokens
                    .iter()
                    .filter(|t| content_tokens.contains(*t))
                    .count();
                if matched > 0 {
                    let frac = matched as f32 / tc.tokens.len() as f32;
                    boost += frac * tc.strength * 0.15;
                }
            }
        }

        boost.clamp(0.0, MAX_GOAL_BOOST)
    }

    /// Inhibitory control: returns a [MIN_INHIBIT_SCORE, 0] suppression for engrams
    /// whose content matches inhibited patterns.
    pub fn inhibit_score(&self, engram_content: &str) -> f32 {
        if !self.unlocked || self.inhibit_patterns.is_empty() {
            return 0.0;
        }
        let content_tokens: HashSet<String> = tokenize(engram_content).into_iter().collect();
        if content_tokens.is_empty() {
            return 0.0;
        }
        let mut suppression = 0.0_f32;
        for i in &self.inhibit_patterns {
            if i.tokens.is_empty() {
                continue;
            }
            let matched = i
                .tokens
                .iter()
                .filter(|t| content_tokens.contains(*t))
                .count();
            if matched == 0 {
                continue;
            }
            let frac = matched as f32 / i.tokens.len() as f32;
            suppression -= frac * i.strength;
        }
        suppression.clamp(MIN_INHIBIT_SCORE, 0.0)
    }

    /// Returns rules whose condition tokens are all present in the cue.
    pub fn matching_rules(&self, cue: &str) -> Vec<&PfcRule> {
        if !self.unlocked || self.rules.is_empty() {
            return Vec::new();
        }
        let cue_tokens: HashSet<String> = tokenize(cue).into_iter().collect();
        self.rules
            .iter()
            .filter(|r| {
                !r.condition_tokens.is_empty()
                    && r.condition_tokens.iter().all(|t| cue_tokens.contains(t))
            })
            .collect()
    }

    /// Working memory fades: goals and task context decay over time. Goals whose
    /// salience reaches 0 are dropped; task context is cleared when exhausted.
    pub fn tick_decay(&mut self, _now: u64) {
        for g in &mut self.goals {
            g.salience = (g.salience - g.decay_rate).max(0.0);
        }
        self.goals.retain(|g| g.salience > 0.0);

        if let Some(tc) = self.task_context.as_mut() {
            tc.strength = (tc.strength - TASK_DECAY_PER_TICK).max(0.0);
        }
        if self
            .task_context
            .as_ref()
            .is_some_and(|t| t.strength <= 0.0)
        {
            self.task_context = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlocked() -> Prefrontal {
        Prefrontal {
            unlocked: true,
            ..Default::default()
        }
    }

    #[test]
    fn goal_bias_boosts_matching_engrams() {
        let mut pfc = unlocked();
        pfc.add_goal("quarterly pricing strategy".into(), 0);

        let matching = pfc.goal_bias_score("our pricing strategy for Q3", "tell me the plan");
        let unrelated = pfc.goal_bias_score("the cat sat on the mat", "tell me the plan");

        assert!(matching > 0.0, "goal-matching engram should be boosted");
        assert!(matching <= MAX_GOAL_BOOST, "boost must be capped at 0.5");
        assert_eq!(unrelated, 0.0, "unrelated engram gets no boost");
    }

    #[test]
    fn goal_bias_stronger_when_cue_relevant() {
        let mut pfc = unlocked();
        pfc.add_goal("pricing strategy".into(), 0);

        let cue_relevant = pfc.goal_bias_score("pricing strategy notes", "what is our pricing");
        let cue_irrelevant = pfc.goal_bias_score("pricing strategy notes", "unrelated question");

        assert!(cue_relevant > cue_irrelevant);
    }

    #[test]
    fn inhibit_suppresses_matching_engrams() {
        let mut pfc = unlocked();
        pfc.add_inhibit("deprecated legacy".into());

        let suppressed = pfc.inhibit_score("this is the deprecated legacy path");
        let clean = pfc.inhibit_score("the modern supported path");

        assert!(suppressed < 0.0, "inhibited content is suppressed");
        assert!(
            suppressed >= MIN_INHIBIT_SCORE,
            "suppression capped at -0.8"
        );
        assert_eq!(clean, 0.0, "clean content is untouched");
    }

    #[test]
    fn goals_decay_to_zero() {
        let mut pfc = unlocked();
        pfc.add_goal("temporary goal".into(), 0);
        assert_eq!(pfc.goals.len(), 1);

        for t in 0..1000 {
            pfc.tick_decay(t);
        }
        assert!(
            pfc.goals.is_empty(),
            "goal should decay away and be dropped"
        );
        assert_eq!(
            pfc.goal_bias_score("temporary goal content", "temporary goal"),
            0.0
        );
    }

    #[test]
    fn matching_rules_returns_correct_rules() {
        let mut pfc = unlocked();
        pfc.add_rule("pricing", RuleAction::BoostVerified);
        pfc.add_rule(
            "weather forecast",
            RuleAction::InjectContext("meteo".into()),
        );

        let hits = pfc.matching_rules("what is the pricing for enterprise");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].action, RuleAction::BoostVerified);

        // Rule with two condition tokens only fires when both are present.
        assert!(pfc.matching_rules("pricing only").len() == 1);
        assert!(pfc.matching_rules("weather is nice").is_empty());
        assert_eq!(pfc.matching_rules("weather forecast today").len(), 1);
    }

    #[test]
    fn locked_pfc_has_no_influence() {
        let mut pfc = Prefrontal::default(); // unlocked == false
        pfc.add_goal("pricing strategy".into(), 0);
        pfc.add_inhibit("deprecated".into());
        pfc.add_rule("pricing", RuleAction::BoostVerified);

        assert_eq!(
            pfc.goal_bias_score("pricing strategy notes", "pricing"),
            0.0
        );
        assert_eq!(pfc.inhibit_score("deprecated legacy path"), 0.0);
        assert!(pfc.matching_rules("pricing question").is_empty());
    }

    #[test]
    fn task_context_reinforcement_and_decay() {
        let mut pfc = unlocked();
        pfc.set_task_context("migrate billing system", 0);
        assert!(pfc.task_context.is_some());

        // Decay a bit, then reinforce back to full strength.
        pfc.tick_decay(1);
        pfc.tick_decay(2);
        pfc.reinforce_task(3);
        assert_eq!(pfc.task_context.as_ref().unwrap().strength, 1.0);

        // Without reinforcement it eventually clears.
        for t in 4..1000 {
            pfc.tick_decay(t);
        }
        assert!(pfc.task_context.is_none());
    }
}

//! Governance — PII scrub, delete-by-subject, audit trail for agent memory stores.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::error::Result;
use crate::query::{forget_before, forget_engram};

/// One governance action for compliance audit logs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub tick: u64,
    pub action: String,
    pub detail: String,
    pub affected: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GovernanceState {
    pub audit_log: Vec<AuditEntry>,
    #[serde(default = "default_max_audit")]
    pub max_audit_entries: usize,
}

fn default_max_audit() -> usize {
    10_000
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PiiScrubReport {
    pub engrams_scrubbed: u32,
    pub wm_slots_scrubbed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeleteBySubjectReport {
    pub engrams_removed: u32,
    pub chorus_removed: u32,
}

/// Simple pattern-based PII redaction (email, phone, SSN-like, credit card runs).
pub fn scrub_pii(text: &str) -> String {
    let mut out = text.to_string();
    // email
    out = replace_pattern(&out, "@", |s| {
        if let Some(at) = s.find('@') {
            let (local, domain) = s.split_at(at);
            if local.len() > 1 {
                format!("{}***{}", &local[..1], domain)
            } else {
                "***@***".into()
            }
        } else {
            s.to_string()
        }
    });
    // phone-ish: 10+ digit runs with optional separators
    out = redact_digit_runs(&out, 10, "[PHONE]");
    // SSN-ish: ###-##-####
    if out.contains('-') {
        let parts: Vec<&str> = out.split_whitespace().collect();
        let mut rebuilt = Vec::new();
        for word in parts {
            if word.len() == 11
                && word.chars().nth(3) == Some('-')
                && word.chars().nth(6) == Some('-')
                && word.chars().filter(|c| c.is_ascii_digit()).count() == 9
            {
                rebuilt.push("[SSN]");
            } else {
                rebuilt.push(word);
            }
        }
        out = rebuilt.join(" ");
    }
    // credit card-ish: 13–19 digit runs
    out = redact_digit_runs(&out, 13, "[CARD]");
    out
}

fn replace_pattern<F>(text: &str, needle: &str, f: F) -> String
where
    F: Fn(&str) -> String,
{
    if !text.contains(needle) {
        return text.to_string();
    }
    text.split_whitespace()
        .map(|w| {
            if w.contains(needle) {
                f(w)
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_digit_runs(text: &str, min_digits: usize, token: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut digit_buf = String::new();
    let flush = |out: &mut String, buf: &mut String, token: &str, min_digits: usize| {
        let digits = buf.chars().filter(|c| c.is_ascii_digit()).count();
        if digits >= min_digits {
            out.push_str(token);
        } else {
            out.push_str(buf);
        }
        buf.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_digit() || (ch == '-' || ch == ' ') && !digit_buf.is_empty() {
            digit_buf.push(ch);
        } else {
            if !digit_buf.is_empty() {
                flush(&mut out, &mut digit_buf, token, min_digits);
            }
            out.push(ch);
        }
    }
    if !digit_buf.is_empty() {
        flush(&mut out, &mut digit_buf, token, min_digits);
    }
    out
}

impl FluctlightBrain {
    pub fn governance_state(&self) -> &GovernanceState {
        &self.governance
    }

    fn audit(&mut self, action: &str, detail: impl Into<String>, affected: u32) {
        let entry = AuditEntry {
            tick: self.autonomic.total_ticks,
            action: action.into(),
            detail: detail.into(),
            affected,
        };
        self.governance.audit_log.push(entry);
        let max = self.governance.max_audit_entries.max(100);
        if self.governance.audit_log.len() > max {
            let drop = self.governance.audit_log.len() - max;
            self.governance.audit_log.drain(0..drop);
        }
    }

    /// Scrub PII patterns from engram content and WM slots.
    pub fn scrub_pii(&mut self) -> Result<PiiScrubReport> {
        self.reject_distributed_mutation("FluctlightBrain::scrub_pii")?;
        let mut report = PiiScrubReport::default();
        for e in &mut self.hippocampus.engrams {
            let scrubbed = scrub_pii(&e.episode.content);
            if scrubbed != e.episode.content {
                e.episode.content = scrubbed;
                report.engrams_scrubbed += 1;
            }
        }
        for slot in self.agent.wm.slots_mut() {
            let scrubbed = scrub_pii(&slot.content);
            if scrubbed != slot.content {
                slot.content = scrubbed;
                report.wm_slots_scrubbed += 1;
            }
        }
        self.invalidate_activation_cache();
        self.audit(
            "scrub_pii",
            format!(
                "engrams={} wm={}",
                report.engrams_scrubbed, report.wm_slots_scrubbed
            ),
            report.engrams_scrubbed + report.wm_slots_scrubbed,
        );
        Ok(report)
    }

    /// GDPR-style delete: remove engrams matching agent_id, context prefix, or content needle.
    pub fn delete_by_subject(&mut self, subject: &str) -> Result<DeleteBySubjectReport> {
        self.reject_distributed_mutation("FluctlightBrain::delete_by_subject")?;
        let needle = subject.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(DeleteBySubjectReport::default());
        }
        let mut report = DeleteBySubjectReport::default();
        let ids: Vec<Uuid> = self
            .hippocampus
            .engrams
            .iter()
            .filter(|e| {
                if e.is_core {
                    return false;
                }
                e.episode
                    .agent_id
                    .as_ref()
                    .map(|a| a.to_lowercase() == needle)
                    .unwrap_or(false)
                    || e.episode.context.to_lowercase().starts_with(&needle)
                    || e.episode.content.to_lowercase().contains(&needle)
            })
            .map(|e| e.id)
            .collect();
        for id in ids {
            if forget_engram(self, id) {
                report.engrams_removed += 1;
            }
        }
        if crate::chorus_runtime::chorus_enabled() {
            report.chorus_removed = self.chorus.remove_matching(|t| {
                t.content.to_lowercase().contains(&needle)
                    || t.context.to_lowercase().contains(&needle)
            });
        }
        self.audit(
            "delete_by_subject",
            format!("subject={subject}"),
            report.engrams_removed + report.chorus_removed,
        );
        Ok(report)
    }

    /// Delete all non-core engrams for a tenant/agent id.
    pub fn delete_by_agent_id(&mut self, agent_id: &str) -> Result<u32> {
        self.reject_distributed_mutation("FluctlightBrain::delete_by_agent_id")?;
        let needle = agent_id.trim().to_lowercase();
        let ids: Vec<Uuid> = self
            .hippocampus
            .engrams
            .iter()
            .filter(|e| {
                !e.is_core
                    && e.episode
                        .agent_id
                        .as_ref()
                        .map(|a| a.to_lowercase() == needle)
                        .unwrap_or(false)
            })
            .map(|e| e.id)
            .collect();
        let mut removed = 0u32;
        for id in ids {
            if forget_engram(self, id) {
                removed += 1;
            }
        }
        self.audit(
            "delete_by_agent_id",
            format!("agent_id={agent_id}"),
            removed,
        );
        Ok(removed)
    }

    /// Forget engrams encoded before `tick` (non-core).
    pub fn forget_before_tick(&mut self, tick: u64) -> Result<u32> {
        self.reject_distributed_mutation("FluctlightBrain::forget_before_tick")?;
        let n = forget_before(self, tick) as u32;
        self.audit("forget_before", format!("tick={tick}"), n);
        Ok(n)
    }

    pub fn audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        let n = limit.min(self.governance.audit_log.len());
        self.governance.audit_log[self.governance.audit_log.len() - n..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    #[test]
    fn scrub_email() {
        let s = scrub_pii("contact me at alice@example.com please");
        assert!(!s.contains("alice@example.com"));
        assert!(s.contains("@"));
    }

    #[test]
    fn delete_by_subject_agent() {
        let mut brain = FluctlightBrain::new();
        let mut ep = Episode::new("secret for user-42", "ledger:user-42", 0.8);
        ep.agent_id = Some("user-42".into());
        brain.experience(ep).unwrap();
        let r = brain.delete_by_subject("user-42").unwrap();
        assert_eq!(r.engrams_removed, 1);
    }
}

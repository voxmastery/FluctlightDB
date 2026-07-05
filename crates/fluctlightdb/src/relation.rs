//! Relational Binding — turn text into role-filler structure the phase parser can bind.
//!
//! # Why this exists
//! [`crate::phase_parse`] can bind *roles to fillers*, but only if something first decides what the
//! roles and fillers are. Word-order sequencing is a proxy; real relational memory needs the actual
//! grammar: who did what to whom (subject/verb/object) and entity→attribute→value facts. The
//! anterior temporal lobe + prefrontal cortex extract these relations before the hippocampus binds
//! them into an episode. This module is that extractor: a lightweight, dependency-free rule parser
//! that produces [`Relation`]s, which then bind into a phase vector via grammatical role vectors.
//!
//! It is intentionally shallow (no ML, no external NLP) — the point is to give the phase binder
//! *structured* roles so "user upgraded plan" and "plan upgraded user" become different memories
//! that can each be queried by role ("what did the user upgrade?").

use serde::{Deserialize, Serialize};

use crate::phase_parse::{PhaseParser, PhaseVector};

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "to", "of", "in", "on", "at", "is", "was", "were", "it", "for",
    "with", "their", "his", "her", "its", "my", "our", "your", "this", "that", "these", "those",
];

/// Common transitive verbs (surface forms) that anchor an SVO relation.
const VERBS: &[&str] = &[
    "upgraded", "bought", "purchased", "cancelled", "canceled", "completed", "sent", "created",
    "deleted", "changed", "moved", "sold", "booked", "paid", "redeemed", "installed", "removed",
    "added", "started", "finished", "joined", "left", "visited", "ordered", "returned", "signed",
];

/// A subject–verb–object relation extracted from a clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub subject: String,
    pub verb: String,
    pub object: String,
}

impl Relation {
    /// Grammatical role→filler pairs for phase binding.
    pub fn role_bindings(&self) -> Vec<(&str, &str)> {
        vec![
            ("subject", self.subject.as_str()),
            ("verb", self.verb.as_str()),
            ("object", self.object.as_str()),
        ]
    }
}

fn is_stop(w: &str) -> bool {
    STOP.contains(&w)
}

fn is_verb(w: &str) -> bool {
    VERBS.contains(&w) || (w.len() > 3 && w.ends_with("ed"))
}

fn words(sentence: &str) -> Vec<String> {
    sentence
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Extract SVO relations from free text (one attempt per sentence).
pub fn extract_relations(text: &str) -> Vec<Relation> {
    let mut out = Vec::new();
    for sentence in text.split(['.', '!', '?', ';']) {
        let ws = words(sentence);
        // First verb position.
        let Some(vi) = ws.iter().position(|w| is_verb(w)) else {
            continue;
        };
        // Subject: nearest content word before the verb.
        let subject = ws[..vi].iter().rev().find(|w| !is_stop(w)).cloned();
        // Object: last content word after the verb (clause head).
        let object = ws[vi + 1..].iter().rev().find(|w| !is_stop(w)).cloned();
        if let (Some(subject), Some(object)) = (subject, object) {
            out.push(Relation {
                subject,
                verb: ws[vi].clone(),
                object,
            });
        }
    }
    out
}

/// Bind all extracted relations of a text into one phase vector (bundle of role⊛filler).
pub fn encode_relations(parser: &PhaseParser, text: &str) -> Option<PhaseVector> {
    let relations = extract_relations(text);
    if relations.is_empty() {
        return None;
    }
    let mut bound = Vec::new();
    for r in &relations {
        let pairs = r.role_bindings();
        if let Some(v) = bundle_pairs(parser, &pairs) {
            bound.push(v);
        }
    }
    PhaseVector::bundle(&bound)
}

fn bundle_pairs(parser: &PhaseParser, pairs: &[(&str, &str)]) -> Option<PhaseVector> {
    let vs: Vec<PhaseVector> = pairs
        .iter()
        .map(|(r, f)| parser.role(r).bind(&PhaseVector::from_token(f, parser.dim)))
        .collect();
    PhaseVector::bundle(&vs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase_parse::Codebook;

    #[test]
    fn extracts_subject_verb_object() {
        let r = &extract_relations("The user upgraded the internet plan.")[0];
        assert_eq!(r.subject, "user");
        assert_eq!(r.verb, "upgraded");
        assert_eq!(r.object, "plan");
    }

    #[test]
    fn role_swap_yields_distinct_relations() {
        let a = &extract_relations("user upgraded plan")[0];
        let b = &extract_relations("plan upgraded user")[0];
        assert_ne!(a.subject, b.subject);
        assert_ne!(a.object, b.object);
    }

    #[test]
    fn heuristic_verb_detection_on_past_tense() {
        let rels = extract_relations("The agent completed the payment.");
        assert_eq!(rels[0].verb, "completed");
        assert_eq!(rels[0].object, "payment");
    }

    #[test]
    fn bound_relation_is_queryable_by_role() {
        let p = PhaseParser::default();
        let structure = encode_relations(&p, "user upgraded plan").unwrap();
        let mut cb = Codebook::default();
        for t in ["user", "upgraded", "plan", "agent", "payment"] {
            cb.intern(t, p.dim);
        }
        assert_eq!(p.readout_role(&structure, "subject", &cb).unwrap().0, "user");
        assert_eq!(p.readout_role(&structure, "object", &cb).unwrap().0, "plan");
    }

    #[test]
    fn multiple_sentences_extract_multiple_relations() {
        let rels = extract_relations("I bought a laptop. Later I cancelled the subscription.");
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].verb, "bought");
        assert_eq!(rels[1].verb, "cancelled");
    }

    #[test]
    fn no_verb_yields_no_relation() {
        assert!(extract_relations("a quiet blue sky").is_empty());
    }
}

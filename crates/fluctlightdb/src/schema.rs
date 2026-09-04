//! CortexSchema — durable neocortical schemas (CLS slow half).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SchemaStatus {
    Active,
    Provisional,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schema {
    pub id: Uuid,
    pub key: String,
    pub statement: String,
    pub slots: Vec<String>,
    pub support_engram_ids: Vec<Uuid>,
    pub confidence: f32,
    pub supersedes: Option<Uuid>,
    pub status: SchemaStatus,
}

impl Schema {
    pub fn new(statement: impl Into<String>, supports: Vec<Uuid>) -> Self {
        let statement = statement.into();
        let key = schema_key(&statement);
        Self {
            id: Uuid::new_v4(),
            key,
            statement,
            slots: Vec::new(),
            support_engram_ids: supports,
            confidence: 0.5,
            supersedes: None,
            status: SchemaStatus::Active,
        }
    }

    pub fn superseding(mut self, old: Uuid) -> Self {
        self.supersedes = Some(old);
        self
    }
}

pub fn schema_key(statement: &str) -> String {
    let t = statement.to_lowercase();
    if t.contains("theme") || t.contains("dark") || t.contains("light") {
        // Keep conflicting preferences as distinct heads so CF does not wipe the old pole.
        let has_dark = t.contains("dark");
        let has_light = t.contains("light");
        if has_dark && !has_light {
            return "theme:dark".into();
        }
        if has_light && !has_dark {
            return "theme:light".into();
        }
        "theme".into()
    } else {
        // stable coarse key: first 3 tokens
        t.split_whitespace().take(3).collect::<Vec<_>>().join("_")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SchemaStore {
    pub schemas: Vec<Schema>,
}

/// Opt-in recall: episodic activation plus matching neocortical schemas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaAwareActivation {
    pub episodic: crate::types::ActivationResult,
    pub schemas: Vec<Schema>,
}

impl SchemaStore {
    pub fn get(&self, id: Uuid) -> Option<&Schema> {
        self.schemas.iter().find(|s| s.id == id)
    }

    pub fn active(&self) -> impl Iterator<Item = &Schema> {
        self.schemas
            .iter()
            .filter(|s| s.status == SchemaStatus::Active)
    }

    pub fn active_head_for_key(&self, key: &str) -> Option<&Schema> {
        self.active().filter(|s| s.key == key).max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn upsert_active(&mut self, mut schema: Schema) -> Result<Uuid> {
        if schema.support_engram_ids.is_empty() {
            return Err(Error::Store("schema requires support_engram_ids".into()));
        }
        schema.status = SchemaStatus::Active;
        if let Some(old) = schema.supersedes {
            if let Some(s) = self.schemas.iter_mut().find(|s| s.id == old) {
                s.status = SchemaStatus::Superseded;
            }
        }
        // deactivate other active with same key
        for s in self.schemas.iter_mut() {
            if s.key == schema.key && s.status == SchemaStatus::Active {
                s.status = SchemaStatus::Superseded;
            }
        }
        let id = schema.id;
        self.schemas.push(schema);
        Ok(id)
    }
}

/// Parse atomic neocortical relations (not document joins).
///
/// Supported surface forms (single-token entities):
/// - `{Person} works in {Place}` / `{Person} lives in {Place}` → `works_in`
/// - `{Place} project uses {Lang}` / `{Place} office runs {Lang}` → `uses_lang`
/// - `{Team} ships {Lang}` / `{Svc} written in {Lang}` → `uses_lang` (place=team/svc)
/// - `{Person} joined {Team}` / `{Person} owns {Svc}` / `{Person} mentors {Role}` → `works_in`
fn try_atomic_schema(content: &str, support: Uuid) -> Option<Schema> {
    let words: Vec<&str> = content.split_whitespace().collect();
    if words.len() < 3 {
        return None;
    }
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();

    // A works in B / A lives in B
    if words.len() >= 4 && lower[1] == "works" && lower[2] == "in" {
        return Some(atomic_schema(
            "works_in", words[0], words[3], content, support, 0.8,
        ));
    }
    if words.len() >= 4 && lower[1] == "lives" && lower[2] == "in" {
        return Some(atomic_schema(
            "works_in", words[0], words[3], content, support, 0.8,
        ));
    }
    if words.len() >= 3 && lower[1] == "joined" {
        return Some(atomic_schema(
            "works_in", words[0], words[2], content, support, 0.75,
        ));
    }
    if words.len() >= 3 && lower[1] == "owns" {
        return Some(atomic_schema(
            "works_in", words[0], words[2], content, support, 0.75,
        ));
    }
    if words.len() >= 3 && lower[1] == "mentors" {
        return Some(atomic_schema(
            "works_in", words[0], words[2], content, support, 0.75,
        ));
    }

    // A project uses B / A office runs B
    if words.len() >= 4 && lower[1] == "project" && lower[2] == "uses" {
        return Some(atomic_schema(
            "uses_lang",
            words[0],
            words[3],
            content,
            support,
            0.8,
        ));
    }
    if words.len() >= 4 && lower[1] == "office" && lower[2] == "runs" {
        return Some(atomic_schema(
            "uses_lang",
            words[0],
            words[3],
            content,
            support,
            0.8,
        ));
    }
    if words.len() >= 3 && lower[1] == "ships" {
        return Some(atomic_schema(
            "uses_lang",
            words[0],
            words[2],
            content,
            support,
            0.75,
        ));
    }
    // "{svc} written in {lang}"
    if words.len() >= 4 && lower[1] == "written" && lower[2] == "in" {
        return Some(atomic_schema(
            "uses_lang",
            words[0],
            words[3],
            content,
            support,
            0.75,
        ));
    }
    // "{role} practice {lang} daily"
    if words.len() >= 3 && lower[1] == "practice" {
        return Some(atomic_schema(
            "uses_lang",
            words[0],
            words[2],
            content,
            support,
            0.7,
        ));
    }

    None
}

fn atomic_schema(
    rel: &str,
    a: &str,
    b: &str,
    content: &str,
    support: Uuid,
    confidence: f32,
) -> Schema {
    let a = a.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    let b = b.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    let key = format!("rel:{rel}:{}", a.to_lowercase());
    let statement = format!("{rel} {a} {b}");
    let mut schema = Schema::new(statement, vec![support]);
    schema.key = key;
    schema.slots = vec![rel.to_string(), a, b];
    schema.confidence = confidence;
    let _ = content;
    schema
}

/// Query-time compositional hop: Person/Entity → Place → Lang.
///
/// Returns **ephemeral** schemas (not written to the store). This is the CLS
/// slow-half operation that beats both top-k lookup and sleep-time string joins:
/// the held-out (person, lang) pair need not co-occur in any stored statement.
pub fn compose_schemas_for_cue(store: &SchemaStore, cue: &str) -> Vec<Schema> {
    let cue_l = cue.to_lowercase();
    let works: Vec<&Schema> = store
        .active()
        .filter(|s| s.slots.first().map(|x| x.as_str()) == Some("works_in") && s.slots.len() >= 3)
        .collect();
    let uses: Vec<&Schema> = store
        .active()
        .filter(|s| s.slots.first().map(|x| x.as_str()) == Some("uses_lang") && s.slots.len() >= 3)
        .collect();

    let mut out = Vec::new();
    for w in works {
        let person = &w.slots[1];
        let place = &w.slots[2];
        if !cue_l.contains(&person.to_lowercase()) {
            continue;
        }
        for u in &uses {
            if !u.slots[1].eq_ignore_ascii_case(place) {
                continue;
            }
            let lang = &u.slots[2];
            let statement = format!("{person} uses {lang} via {place}");
            let mut supports = w.support_engram_ids.clone();
            for id in &u.support_engram_ids {
                if !supports.contains(id) {
                    supports.push(*id);
                }
            }
            let mut schema = Schema::new(statement, supports);
            schema.key = format!("compose:stack:{}", person.to_lowercase());
            schema.slots = vec![
                "compose_stack".into(),
                person.clone(),
                place.clone(),
                lang.clone(),
            ];
            schema.confidence = ((w.confidence + u.confidence) * 0.5).clamp(0.5, 0.95);
            out.push(schema);
        }
    }
    out
}

/// Deterministic CLS crystallize: theme groups + atomic relations.
/// Composition across relations happens at query time via [`compose_schemas_for_cue`].
pub fn crystallize_from_engrams(store: &mut SchemaStore, engrams: &[crate::engram::Engram]) {
    use std::collections::HashMap;

    let mut groups: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut statements: HashMap<String, String> = HashMap::new();
    for e in engrams {
        let key = schema_key(&e.episode.content);
        groups.entry(key.clone()).or_default().push(e.id);
        statements
            .entry(key)
            .or_insert_with(|| e.episode.content.clone());
    }
    for (key, ids) in groups {
        if ids.len() < 2 {
            continue;
        }
        // Theme cues crystallize at 2+; other keys need 3+ (conservative).
        if !(key == "theme" || key.starts_with("theme:")) && ids.len() < 3 {
            continue;
        }
        if (key == "theme" || key.starts_with("theme:")) && ids.len() < 2 {
            continue;
        }
        let statement = statements.get(&key).cloned().unwrap_or_else(|| key.clone());
        let mut schema = Schema::new(statement, ids);
        schema.key = key;
        if let Some(prev) = store.active_head_for_key(&schema.key) {
            schema = schema.superseding(prev.id);
        }
        let _ = store.upsert_active(schema);
    }

    // Atomic relations — one schema per extracted edge (no person|lang concat).
    for e in engrams {
        if let Some(mut schema) = try_atomic_schema(&e.episode.content, e.id) {
            if let Some(prev) = store.active_head_for_key(&schema.key) {
                schema = schema.superseding(prev.id);
            }
            let _ = store.upsert_active(schema);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn reject_schema_without_supports() {
        let mut store = SchemaStore::default();
        let s = Schema::new("user prefers dark mode", vec![]);
        assert!(store.upsert_active(s).is_err());
    }

    #[test]
    fn upsert_requires_existing_support_ids_checked_by_caller() {
        let mut store = SchemaStore::default();
        let id = Uuid::new_v4();
        let s = Schema::new("user prefers dark mode", vec![id]);
        assert!(store.upsert_active(s).is_ok());
        assert_eq!(store.active().count(), 1);
    }

    #[test]
    fn supersede_keeps_old_resolvable() {
        let mut store = SchemaStore::default();
        let a = Uuid::new_v4();
        let old = store
            .upsert_active(Schema::new("theme=dark preference", vec![a]))
            .unwrap();
        let new = store
            .upsert_active(Schema::new("theme=dark preference v2", vec![a]).superseding(old))
            .unwrap();
        assert_eq!(store.active_head_for_key("theme:dark").unwrap().id, new);
        assert!(store.get(old).unwrap().status == SchemaStatus::Superseded);
    }

    #[test]
    fn atomic_relations_and_query_time_compose() {
        use crate::engram::Engram;
        use crate::types::Episode;
        let mut store = SchemaStore::default();
        #[allow(deprecated)]
        let e1 = Engram::new(
            Uuid::nil(),
            Episode::new("Alice works in Berlin", "bio", 0.9),
            0.9,
            1,
            0,
        );
        #[allow(deprecated)]
        let e2 = Engram::new(
            Uuid::nil(),
            Episode::new("Berlin project uses Rust", "proj", 0.9),
            0.9,
            2,
            0,
        );
        crystallize_from_engrams(&mut store, &[e1, e2]);
        // Stored: atomic only — no Alice+Rust co-occurrence in one statement
        assert!(
            store
                .active()
                .all(|s| !(s.statement.contains("Alice") && s.statement.contains("Rust"))),
            "sleep must not pre-join held-out pair: {:?}",
            store.active().collect::<Vec<_>>()
        );
        let composed = compose_schemas_for_cue(&store, "What stack does Alice use?");
        assert!(
            composed.iter().any(|s| {
                s.statement.contains("Alice")
                    && s.statement.contains("Rust")
                    && s.slots.first().map(|x| x.as_str()) == Some("compose_stack")
            }),
            "query-time compose must produce Alice→Rust, got {composed:?}"
        );
    }

    #[test]
    fn crystallize_theme_from_two_engrams() {
        use crate::engram::Engram;
        use crate::types::Episode;
        let mut store = SchemaStore::default();
        #[allow(deprecated)]
        let e1 = Engram::new(
            Uuid::nil(),
            Episode::new("User prefers dark mode theme v0", "p", 0.8),
            0.8,
            1,
            0,
        );
        #[allow(deprecated)]
        let e2 = Engram::new(
            Uuid::nil(),
            Episode::new("User prefers dark mode theme v1", "p", 0.8),
            0.8,
            2,
            0,
        );
        crystallize_from_engrams(&mut store, &[e1, e2]);
        assert!(
            store
                .active()
                .any(|s| s.key == "theme" || s.key.starts_with("theme:")),
            "theme schema expected"
        );
    }
}

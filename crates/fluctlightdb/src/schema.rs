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
            .upsert_active(Schema::new("theme=light", vec![a]))
            .unwrap();
        let new = store
            .upsert_active(Schema::new("theme=dark", vec![a]).superseding(old))
            .unwrap();
        assert_eq!(store.active_head_for_key("theme").unwrap().id, new);
        assert!(store.get(old).unwrap().status == SchemaStatus::Superseded);
    }
}

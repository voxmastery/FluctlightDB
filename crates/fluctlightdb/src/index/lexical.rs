//! In-memory lexical index — token → engram ID seeds.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::error::Result;

#[derive(Default)]
pub struct LexicalIndex {
    token_to_ids: HashMap<String, HashSet<Uuid>>,
    engram_tokens: HashMap<Uuid, HashSet<String>>,
}

impl LexicalIndex {
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn open_path(_path: &std::path::Path) -> Result<Self> {
        Ok(Self::default())
    }

    pub fn clear(&mut self) {
        self.token_to_ids.clear();
        self.engram_tokens.clear();
    }

    pub fn upsert(&mut self, engram_id: Uuid, content: &str) -> Result<()> {
        self.remove(engram_id)?;
        let tokens: HashSet<String> = content
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .filter(|s| s.len() > 2)
            .collect();
        for t in &tokens {
            self.token_to_ids.entry(t.clone()).or_default().insert(engram_id);
        }
        self.engram_tokens.insert(engram_id, tokens);
        Ok(())
    }

    pub fn remove(&mut self, engram_id: Uuid) -> Result<()> {
        if let Some(tokens) = self.engram_tokens.remove(&engram_id) {
            for t in tokens {
                if let Some(set) = self.token_to_ids.get_mut(&t) {
                    set.remove(&engram_id);
                    if set.is_empty() {
                        self.token_to_ids.remove(&t);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn search(&self, cue: &str, limit: usize) -> Result<Vec<Uuid>> {
        let mut scores: HashMap<Uuid, usize> = HashMap::new();
        for token in cue.split_whitespace().map(|s| s.to_lowercase()) {
            if token.len() <= 2 {
                continue;
            }
            if let Some(ids) = self.token_to_ids.get(&token) {
                for id in ids {
                    *scores.entry(*id).or_insert(0) += 1;
                }
            }
        }
        let mut ranked: Vec<(Uuid, usize)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(ranked.into_iter().take(limit).map(|(id, _)| id).collect())
    }

    pub fn rebuild(brain: &FluctlightBrain) -> Result<Self> {
        let mut idx = Self::default();
        for e in brain.hippocampus.engrams_for_life(brain.life.life_id) {
            idx.upsert(e.id, &e.episode.content)?;
        }
        Ok(idx)
    }
}

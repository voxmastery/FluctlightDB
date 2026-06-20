use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engram::Engram;

/// Fast episodic store — pattern separation + completion (Marr / CLS).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hippocampus {
    pub engrams: Vec<Engram>,
    /// Idempotent RAG chunk lookup: `doc_id#chunk_id` → engram id.
    #[serde(default)]
    pub rag_index: HashMap<String, Uuid>,
}

pub fn rag_chunk_key(doc_id: &str, chunk_id: &str) -> String {
    format!("{doc_id}#{chunk_id}")
}

impl Hippocampus {
    pub fn encode(&mut self, engram: Engram) {
        if let Some(ref rag) = engram.episode.rag {
            if let (Some(doc), Some(chunk)) = (&rag.doc_id, &rag.chunk_id) {
                self.rag_index
                    .insert(rag_chunk_key(doc, chunk), engram.id);
            }
        }
        self.engrams.push(engram);
    }

    pub fn find_rag_chunk(&self, doc_id: &str, chunk_id: &str) -> Option<Uuid> {
        self.rag_index
            .get(&rag_chunk_key(doc_id, chunk_id))
            .copied()
            .and_then(|id| {
                if self.engrams.iter().any(|e| e.id == id) {
                    Some(id)
                } else {
                    None
                }
            })
    }

    pub fn rebuild_rag_index(&mut self) {
        self.rag_index.clear();
        for e in &self.engrams {
            if let Some(ref rag) = e.episode.rag {
                if let (Some(doc), Some(chunk)) = (&rag.doc_id, &rag.chunk_id) {
                    self.rag_index.insert(rag_chunk_key(doc, chunk), e.id);
                }
            }
        }
    }

    pub fn engrams_for_life(&self, life_id: uuid::Uuid) -> impl Iterator<Item = &Engram> {
        self.engrams.iter().filter(move |e| e.life_id == life_id)
    }

    pub fn clear_ephemeral(&mut self, life_id: uuid::Uuid) {
        self.engrams
            .retain(|e| e.life_id != life_id || e.is_core);
    }

    pub fn mark_core(&mut self, engram_id: uuid::Uuid) {
        if let Some(e) = self.engrams.iter_mut().find(|e| e.id == engram_id) {
            e.is_core = true;
        }
    }

    pub fn recent(&self, life_id: uuid::Uuid, n: usize) -> Vec<&Engram> {
        let mut v: Vec<_> = self.engrams_for_life(life_id).collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.encoded_at_tick));
        v.truncate(n);
        v
    }
}

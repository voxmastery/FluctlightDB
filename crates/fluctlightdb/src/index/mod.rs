//! Hybrid recall index — FTS5 sidecar + HNSW semantic seeds.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::error::Result;
use crate::semantic::SemanticField;

mod lexical;
mod semantic;
mod sidecar;

use lexical::LexicalIndex;
use semantic::{semantic_similarities_for, semantic_top_k};
use sidecar::SidecarIndex;

pub const DEFAULT_CANDIDATE_CAP: usize = 128;
pub const LEXICAL_SEED_LIMIT: usize = 64;
pub const SEMANTIC_SEED_LIMIT: usize = 50;

enum IndexBackend {
    Sidecar(SidecarIndex),
    Memory(Mutex<LexicalIndex>),
}

pub struct RecallIndex {
    backend: Mutex<IndexBackend>,
    path: Option<PathBuf>,
}

impl RecallIndex {
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            backend: Mutex::new(IndexBackend::Memory(Mutex::new(
                LexicalIndex::open_in_memory()?,
            ))),
            path: None,
        })
    }

    pub fn open_sidecar(brain_path: &Path) -> Result<Self> {
        let db_path = Self::sidecar_path(brain_path);
        let sidecar = SidecarIndex::open(&db_path)?;
        Ok(Self {
            backend: Mutex::new(IndexBackend::Sidecar(sidecar)),
            path: Some(db_path),
        })
    }

    pub fn sidecar_path(brain_path: &Path) -> PathBuf {
        if brain_path.is_dir() {
            brain_path.join("recall_index.sqlite")
        } else {
            brain_path.with_extension("flct.index.sqlite")
        }
    }

    pub fn rebuild(brain: &FluctlightBrain) -> Result<Self> {
        if let Some(path) = brain.brain_store_path() {
            let idx = Self::open_sidecar(path)?;
            if let Ok(guard) = idx.backend.lock() {
                if let IndexBackend::Sidecar(ref s) = *guard {
                    s.rebuild_from_brain(brain)?;
                }
            }
            return Ok(idx);
        }
        let idx = Self::open_in_memory()?;
        if let Ok(guard) = idx.backend.lock() {
            if let IndexBackend::Memory(lex_mtx) = &*guard {
                let mut lex = lex_mtx
                    .lock()
                    .map_err(|e| crate::error::Error::Store(format!("lexical lock: {e}")))?;
                lex.clear();
                for e in brain.hippocampus.engrams_for_life(brain.life.life_id) {
                    lex.upsert(e.id, &e.episode.content)?;
                }
            }
        }
        Ok(idx)
    }

    pub fn upsert_engram(
        &self,
        engram_id: Uuid,
        content: &str,
        vector: Option<&[f32]>,
    ) -> Result<()> {
        let guard = self
            .backend
            .lock()
            .map_err(|e| crate::error::Error::Store(format!("recall index lock: {e}")))?;
        match &*guard {
            IndexBackend::Sidecar(s) => s.upsert(engram_id, content, vector),
            IndexBackend::Memory(lex) => lex
                .lock()
                .map_err(|e| crate::error::Error::Store(format!("lexical lock: {e}")))?
                .upsert(engram_id, content),
        }
    }

    pub fn remove_engram(&self, engram_id: Uuid) -> Result<()> {
        let guard = self
            .backend
            .lock()
            .map_err(|e| crate::error::Error::Store(format!("recall index lock: {e}")))?;
        match &*guard {
            IndexBackend::Sidecar(s) => s.remove(engram_id),
            IndexBackend::Memory(lex) => lex
                .lock()
                .map_err(|e| crate::error::Error::Store(format!("lexical lock: {e}")))?
                .remove(engram_id),
        }
    }

    /// Union of FTS hits and HNSW / semantic top-k, capped.
    pub fn hybrid_candidates(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        semantic: &SemanticField,
        cap: usize,
    ) -> Result<Vec<Uuid>> {
        let cap = cap.max(1).min(DEFAULT_CANDIDATE_CAP);
        let mut set = HashSet::new();

        let guard = self
            .backend
            .lock()
            .map_err(|e| crate::error::Error::Store(format!("recall index lock: {e}")))?;
        match &*guard {
            IndexBackend::Sidecar(s) => {
                for id in s.fts_search(cue, LEXICAL_SEED_LIMIT)? {
                    set.insert(id);
                }
                if let Some(vec) = cue_vector {
                    for id in s.semantic_search(vec, SEMANTIC_SEED_LIMIT)? {
                        set.insert(id);
                    }
                }
            }
            IndexBackend::Memory(lex) => {
                let lex = lex
                    .lock()
                    .map_err(|e| crate::error::Error::Store(format!("lexical lock: {e}")))?;
                for id in lex.search(cue, LEXICAL_SEED_LIMIT)? {
                    set.insert(id);
                }
                if let Some(vec) = cue_vector {
                    for id in semantic_top_k(semantic, vec, SEMANTIC_SEED_LIMIT) {
                        set.insert(id);
                    }
                }
            }
        }
        drop(guard);

        let mut out: Vec<Uuid> = set.into_iter().collect();
        if out.len() > cap {
            out.truncate(cap);
        }
        Ok(out)
    }

    pub fn semantic_sims_for_candidates(
        semantic: &SemanticField,
        cue_vector: Option<&[f32]>,
        candidates: &[Uuid],
    ) -> std::collections::HashMap<Uuid, f32> {
        match cue_vector {
            Some(v) => semantic_similarities_for(semantic, v, candidates),
            None => std::collections::HashMap::new(),
        }
    }

    pub fn uses_sidecar(&self) -> bool {
        self.path.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::FluctlightBrain;
    use crate::types::Episode;
    use tempfile::tempdir;

    #[test]
    fn sidecar_fts_on_attach() {
        let dir = tempdir().unwrap();
        let brain_path = dir.path().join("brain");
        std::fs::create_dir_all(&brain_path).unwrap();
        let mut brain = FluctlightBrain::new();
        brain.attach_store_path(brain_path.clone());
        brain
            .experience(Episode {
                content: "wallet balance verified ledger".into(),
                context: "test".into(),
                outcome: None,
                salience_hint: 0.8,
                semantic_vector: Some(vec![1.0, 0.0, 0.0]),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        assert!(brain.has_sidecar_index());
        let result = brain.activate_with_semantic("wallet balance", Some(&[1.0, 0.0, 0.0]));
        assert!(!result.recalls.is_empty());
        assert!(RecallIndex::sidecar_path(&brain_path).exists());
    }
}

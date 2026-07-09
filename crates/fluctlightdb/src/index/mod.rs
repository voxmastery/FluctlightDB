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
/// Absolute ceiling for caller-supplied candidate caps (memory safety valve).
pub const MAX_CANDIDATE_CAP: usize = 4096;
pub const LEXICAL_SEED_LIMIT: usize = 64;
pub const SEMANTIC_SEED_LIMIT: usize = 50;

fn lexical_seed_limit(cap: usize) -> usize {
    std::env::var("FLUCTLIGHT_LEXICAL_SEED_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| LEXICAL_SEED_LIMIT.max(cap).min(512))
}

fn semantic_seed_limit(cap: usize) -> usize {
    std::env::var("FLUCTLIGHT_SEMANTIC_SEED_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| SEMANTIC_SEED_LIMIT.max(cap).min(512))
}

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
        let db_path = Self::resolve_sidecar_path(brain_path);
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

    /// Pre-v4 directory brains kept the sidecar as a sibling `*.flct.index.sqlite` file.
    pub fn legacy_sidecar_path(brain_path: &Path) -> PathBuf {
        if brain_path.is_dir() {
            let name = brain_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("brain");
            let stem = name.strip_suffix(".brain").unwrap_or(name);
            brain_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{stem}.flct.index.sqlite"))
        } else {
            Self::sidecar_path(brain_path)
        }
    }

    fn resolve_sidecar_path(brain_path: &Path) -> PathBuf {
        let primary = Self::sidecar_path(brain_path);
        if primary.exists() {
            return primary;
        }
        let legacy = Self::legacy_sidecar_path(brain_path);
        if legacy.exists() {
            if let Some(parent) = primary.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&legacy, &primary);
            return primary;
        }
        primary
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
        // Callers may exceed DEFAULT_CANDIDATE_CAP (e.g. k=150 bench runs), but an
        // unbounded cap lets one recall allocate the whole store; hard-limit it.
        let cap = cap.max(1).min(MAX_CANDIDATE_CAP);
        let lex_limit = lexical_seed_limit(cap);
        let sem_limit = semantic_seed_limit(cap);
        let mut set = HashSet::new();

        let guard = self
            .backend
            .lock()
            .map_err(|e| crate::error::Error::Store(format!("recall index lock: {e}")))?;
        match &*guard {
            IndexBackend::Sidecar(s) => {
                for id in s.fts_search(cue, lex_limit)? {
                    set.insert(id);
                }
                if let Some(vec) = cue_vector {
                    for id in s.semantic_search(vec, sem_limit)? {
                        set.insert(id);
                    }
                }
            }
            IndexBackend::Memory(lex) => {
                let lex = lex
                    .lock()
                    .map_err(|e| crate::error::Error::Store(format!("lexical lock: {e}")))?;
                for id in lex.search(cue, lex_limit)? {
                    set.insert(id);
                }
                if let Some(vec) = cue_vector {
                    for id in semantic_top_k(semantic, vec, sem_limit) {
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

    #[test]
    fn sidecar_recall_after_checkpoint_reload() {
        let dir = tempdir().unwrap();
        let brain_path = dir.path().join("agent.brain");
        let eid = {
            let mut brain = FluctlightBrain::new();
            brain.attach_store_path(brain_path.clone());
            brain
                .experience(Episode {
                    content: "user prefers dark mode in all applications".into(),
                    context: "prefs".into(),
                    outcome: None,
                    salience_hint: 0.8,
                    semantic_vector: Some(vec![0.77, 0.23, 0.0]),
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .unwrap();
            brain.checkpoint().unwrap();
            let result = brain.activate_with_semantic("dark mode", Some(&[0.77, 0.23, 0.0]));
            assert!(!result.recalls.is_empty());
            result.recalls[0].engram_id
        };
        let brain2 = FluctlightBrain::open(&brain_path).unwrap();
        let result = brain2.activate_with_semantic("dark mode", Some(&[0.77, 0.23, 0.0]));
        assert!(
            !result.recalls.is_empty(),
            "recall after reload should find checkpointed engram"
        );
        assert_eq!(result.recalls[0].engram_id, eid);
    }
}

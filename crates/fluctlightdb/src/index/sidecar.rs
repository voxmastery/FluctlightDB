//! Persistent recall sidecar — SQLite FTS5 + in-memory HNSW for semantic seeds.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use fast_hnsw::distance::Cosine;
use fast_hnsw::labeled::LabeledIndex;
use fast_hnsw::Builder;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};

const SCHEMA: &str = r#"
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS engram_fts USING fts5(content, engram_id UNINDEXED, tokenize='unicode61');
CREATE TABLE IF NOT EXISTS engram_vec (
    engram_id TEXT PRIMARY KEY,
    dim INTEGER NOT NULL,
    vec BLOB NOT NULL
);
"#;

pub struct SidecarIndex {
    conn: Mutex<Connection>,
    hnsw: Mutex<LabeledIndex<Cosine, String>>,
}

impl SidecarIndex {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(Error::Io)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(SCHEMA)?;
        let sidecar = Self {
            conn: Mutex::new(conn),
            hnsw: Mutex::new(new_hnsw()),
        };
        sidecar.rebuild_hnsw_from_db()?;
        Ok(sidecar)
    }

    pub fn upsert(&self, engram_id: Uuid, content: &str, vector: Option<&[f32]>) -> Result<()> {
        let id_str = engram_id.to_string();
        let conn = self.conn.lock().map_err(lock_err)?;
        conn.execute("DELETE FROM engram_fts WHERE engram_id = ?1", params![id_str])?;
        conn.execute(
            "INSERT INTO engram_fts(content, engram_id) VALUES (?1, ?2)",
            params![content, id_str],
        )?;
        if let Some(vec) = vector {
            if !vec.is_empty() {
                let blob = vector_to_blob(vec);
                conn.execute(
                    "INSERT INTO engram_vec(engram_id, dim, vec) VALUES (?1, ?2, ?3)
                     ON CONFLICT(engram_id) DO UPDATE SET dim=excluded.dim, vec=excluded.vec",
                    params![id_str, vec.len() as i64, blob],
                )?;
            }
        }
        drop(conn);
        if vector.is_some() {
            self.rebuild_hnsw_from_db()?;
        }
        Ok(())
    }

    pub fn remove(&self, engram_id: Uuid) -> Result<()> {
        let id_str = engram_id.to_string();
        let conn = self.conn.lock().map_err(lock_err)?;
        conn.execute("DELETE FROM engram_fts WHERE engram_id = ?1", params![id_str])?;
        conn.execute("DELETE FROM engram_vec WHERE engram_id = ?1", params![id_str])?;
        drop(conn);
        self.rebuild_hnsw_from_db()?;
        Ok(())
    }

    pub fn fts_search(&self, cue: &str, limit: usize) -> Result<Vec<Uuid>> {
        let conn = self.conn.lock().map_err(lock_err)?;
        let query = cue
            .split_whitespace()
            .filter(|t| t.len() > 2)
            .collect::<Vec<_>>()
            .join(" OR ");
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sql = "SELECT engram_id FROM engram_fts WHERE engram_fts MATCH ?1 ORDER BY rank LIMIT ?2";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            let id: String = row.get(0)?;
            Ok(id)
        })?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(id_str) = row {
                if let Ok(id) = Uuid::parse_str(&id_str) {
                    out.push(id);
                }
            }
        }
        Ok(out)
    }

    pub fn semantic_search(&self, cue_vector: &[f32], limit: usize) -> Result<Vec<Uuid>> {
        if cue_vector.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let h = self.hnsw.lock().map_err(lock_err)?;
        if h.is_empty() {
            return Ok(Vec::new());
        }
        let ef = limit.max(50).min(256);
        let results = h.search(cue_vector, limit, ef);
        let mut out = Vec::new();
        for r in results {
            if let Ok(id) = Uuid::parse_str(r.payload) {
                out.push(id);
            }
        }
        Ok(out)
    }

    pub fn rebuild_from_brain(&self, brain: &FluctlightBrain) -> Result<()> {
        {
            let conn = self.conn.lock().map_err(lock_err)?;
            conn.execute_batch(
                "DELETE FROM engram_fts; DELETE FROM engram_vec; DELETE FROM meta;",
            )?;
            for e in brain.hippocampus.engrams_for_life(brain.life.life_id) {
                let id_str = e.id.to_string();
                conn.execute(
                    "INSERT INTO engram_fts(content, engram_id) VALUES (?1, ?2)",
                    params![e.episode.content, id_str],
                )?;
                if let Some(vec) = brain.semantic.engram_vectors.get(&e.id) {
                    if !vec.is_empty() {
                        conn.execute(
                            "INSERT INTO engram_vec(engram_id, dim, vec) VALUES (?1, ?2, ?3)",
                            params![id_str, vec.len() as i64, vector_to_blob(vec)],
                        )?;
                    }
                }
            }
        }
        self.rebuild_hnsw_from_db()?;
        Ok(())
    }

    fn rebuild_hnsw_from_db(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(lock_err)?;
        let mut stmt = conn.prepare("SELECT engram_id, dim, vec FROM engram_vec")?;
        let rows: Vec<(String, i64, Vec<u8>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        drop(conn);

        let mut h = new_hnsw();
        for (id_str, dim, blob) in rows {
            let vec = blob_to_vector(&blob, dim as usize);
            if vec.is_empty() {
                continue;
            }
            h.insert(vec, id_str);
        }
        *self.hnsw.lock().map_err(lock_err)? = h;
        Ok(())
    }
}

fn new_hnsw() -> LabeledIndex<Cosine, String> {
    Builder::new()
        .m(16)
        .ef_construction(200)
        .seed(42)
        .build_labeled(Cosine)
}

fn vector_to_blob(vec: &[f32]) -> Vec<u8> {
    vec.iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

fn blob_to_vector(blob: &[u8], dim: usize) -> Vec<f32> {
    if dim == 0 || blob.len() < dim * 4 {
        return Vec::new();
    }
    blob.chunks_exact(4)
        .take(dim)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn lock_err<T: std::fmt::Display>(e: T) -> Error {
    Error::Store(format!("sidecar lock poisoned: {e}"))
}

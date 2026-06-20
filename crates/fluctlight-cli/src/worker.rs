//! Persistent in-process worker — JSON lines on stdin/stdout (brain loaded once).

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use fluctlightdb::api_slim;
use fluctlightdb::{store, FluctlightBrain};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
struct WorkerRequest {
    #[serde(default)]
    id: u64,
    op: String,
    #[serde(default)]
    cue: Option<String>,
    #[serde(default)]
    semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    batch: Option<Vec<BatchItem>>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    steps: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct BatchItem {
    #[serde(default)]
    cue: String,
    #[serde(default)]
    semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    agent_id: Option<String>,
}

pub fn run_worker(path: PathBuf) -> io::Result<()> {
    let mut brain = FluctlightBrain::open_readonly(&path).map_err(io_err)?;
    let mut loaded_mtime = store::snapshot_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);

    eprintln!(
        "fluctlight worker ready (path={}, engrams={})",
        path.display(),
        brain.hippocampus.engrams.len()
    );

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: WorkerRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                write_response(&json!({"ok": false, "error": format!("parse: {e}")}))?;
                continue;
            }
        };

        if req.op == "shutdown" || req.op == "quit" {
            write_response(&json!({"ok": true, "id": req.id, "op": "shutdown"}))?;
            break;
        }

        maybe_reload(&path, &mut brain, &mut loaded_mtime);

        let resp = match req.op.as_str() {
            "ping" => json!({"ok": true, "id": req.id, "pong": true}),
            "reload" => {
                brain = FluctlightBrain::open_readonly(&path).map_err(io_err)?;
                loaded_mtime = store::snapshot_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);
                json!({
                    "ok": true,
                    "id": req.id,
                    "engrams": brain.hippocampus.engrams.len(),
                })
            }
            "status" => json!({"ok": true, "id": req.id, "status": brain.status()}),
            "activate" => {
                let cue = req.cue.unwrap_or_default();
                let mut result = brain.activate_scoped(
                    &cue,
                    req.semantic_vector.as_deref(),
                    req.agent_id.as_deref(),
                );
                api_slim::slim_activation_for_api(&mut result, req.limit);
                json!({"ok": true, "id": req.id, "result": result})
            }
            "activate_batch" => {
                let items: Vec<(String, Option<Vec<f32>>, Option<String>)> = req
                    .batch
                    .unwrap_or_default()
                    .into_iter()
                    .map(|b| (b.cue, b.semantic_vector, b.agent_id))
                    .collect();
                let mut results = brain.activate_batch(&items);
                for result in &mut results {
                    api_slim::slim_activation_for_api(result, req.limit);
                }
                json!({
                    "ok": true,
                    "id": req.id,
                    "results": results,
                    "count": results.len(),
                })
            }
            "verified_context" => {
                let limit = req.limit.unwrap_or(12);
                let ctx = brain.verified_context(limit);
                json!({"ok": true, "id": req.id, "context": ctx})
            }
            "preplay" => {
                let goal = req.goal.unwrap_or_default();
                let steps = req.steps.unwrap_or(4);
                let result = brain.preplay(&goal, steps);
                json!({"ok": true, "id": req.id, "result": result})
            }
            other => json!({"ok": false, "id": req.id, "error": format!("unknown op: {other}")}),
        };

        write_response(&resp)?;
    }
    Ok(())
}

fn write_response(v: &Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, v).map_err(io_err)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

fn maybe_reload(path: &Path, brain: &mut FluctlightBrain, loaded_mtime: &mut SystemTime) {
    let current = store::snapshot_mtime(path).unwrap_or(SystemTime::UNIX_EPOCH);
    if current > *loaded_mtime {
        if let Ok(next) = FluctlightBrain::open_readonly(path) {
            *brain = next;
            *loaded_mtime = current;
        }
    }
}

fn io_err(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

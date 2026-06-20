//! Trim activation payloads for HTTP / worker APIs (drop 384-d vectors from responses).

use crate::types::ActivationResult;

/// Default recall count returned over HTTP/worker (matches internal top-k).
pub const DEFAULT_API_RECALL_LIMIT: usize = 8;

/// Strip heavy fields from activation results before serializing to clients.
pub fn slim_activation_for_api(result: &mut ActivationResult, limit: Option<usize>) {
    let cap = limit.unwrap_or(DEFAULT_API_RECALL_LIMIT);
    if result.recalls.len() > cap {
        result.recalls.truncate(cap);
    }
    for recall in &mut result.recalls {
        recall.episode.semantic_vector = None;
    }
}

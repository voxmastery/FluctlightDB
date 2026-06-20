//! Brain-native query layer — list, forget, hybrid search, stats.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::types::{ActivationResult, RagRef};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueryRequest {
    ListEngrams {
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default = "default_page")]
        page: usize,
        #[serde(default = "default_page_size")]
        page_size: usize,
    },
    ListVerified {
        #[serde(default = "default_page")]
        page: usize,
        #[serde(default = "default_page_size")]
        page_size: usize,
    },
    ListUnverified {
        #[serde(default = "default_page")]
        page: usize,
        #[serde(default = "default_page_size")]
        page_size: usize,
    },
    GetEngram {
        engram_id: Uuid,
    },
    Forget {
        engram_id: Uuid,
    },
    ForgetBefore {
        tick: u64,
    },
    SearchHybrid {
        cue: String,
        #[serde(default)]
        vector: Option<Vec<f32>>,
        #[serde(default = "default_top_k")]
        top_k: usize,
        #[serde(default)]
        agent_id: Option<String>,
    },
    SearchByRag {
        doc_id: String,
        #[serde(default)]
        chunk_id: Option<String>,
        #[serde(default = "default_page")]
        page: usize,
        #[serde(default = "default_page_size")]
        page_size: usize,
    },
    Stats,
}

fn default_page() -> usize {
    0
}
fn default_page_size() -> usize {
    50
}
fn default_top_k() -> usize {
    8
}

const MAX_PAGE_SIZE: usize = 200;
const MAX_TOP_K: usize = 64;

fn clamp_page_size(n: usize) -> usize {
    n.clamp(1, MAX_PAGE_SIZE)
}

fn clamp_top_k(n: usize) -> usize {
    n.clamp(1, MAX_TOP_K)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum QueryResponse {
    ListEngrams {
        total: usize,
        page: usize,
        items: Vec<EngramSummary>,
    },
    ListVerified {
        total: usize,
        page: usize,
        items: Vec<EngramSummary>,
    },
    ListUnverified {
        total: usize,
        page: usize,
        items: Vec<EngramSummary>,
    },
    GetEngram {
        item: Option<EngramSummary>,
    },
    Forget {
        removed: bool,
    },
    ForgetBefore {
        removed: usize,
    },
    SearchHybrid {
        activation: ActivationResult,
    },
    SearchByRag {
        total: usize,
        page: usize,
        items: Vec<EngramSummary>,
    },
    Stats {
        stats: BrainQueryStats,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngramSummary {
    pub engram_id: Uuid,
    pub content: String,
    pub context: String,
    pub agent_id: Option<String>,
    pub tenant_id: Option<String>,
    pub encoded_at_tick: u64,
    pub salience: f32,
    pub is_core: bool,
    #[serde(default)]
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag: Option<RagRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainQueryStats {
    pub engrams: usize,
    pub synapses: usize,
    pub experiences: u64,
    pub sleep_cycles: u64,
    pub auto_sleeps: u64,
    pub synapse_pressure: f32,
}

pub fn execute(brain: &FluctlightBrain, req: QueryRequest) -> QueryResponse {
    match req {
        QueryRequest::ListEngrams {
            agent_id,
            page,
            page_size,
        } => {
            let filtered: Vec<_> = brain
                .hippocampus
                .engrams
                .iter()
                .filter(|e| match &agent_id {
                    Some(a) => e.episode.agent_id.as_deref() == Some(a.as_str()),
                    None => true,
                })
                .map(engram_summary)
                .collect();
            paginate(filtered, page, page_size)
        }
        QueryRequest::ListVerified { page, page_size } => {
            let filtered: Vec<_> = brain
                .hippocampus
                .engrams
                .iter()
                .filter(|e| {
                    e.episode
                        .provenance
                        .as_ref()
                        .map(|p| p.verified)
                        .unwrap_or(false)
                })
                .map(engram_summary)
                .collect();
            paginate_verified(filtered, page, page_size)
        }
        QueryRequest::ListUnverified { page, page_size } => {
            let filtered: Vec<_> = brain
                .hippocampus
                .engrams
                .iter()
                .filter(|e| is_unverified_factual(e))
                .map(engram_summary)
                .collect();
            paginate_unverified(filtered, page, page_size)
        }
        QueryRequest::GetEngram { engram_id } => QueryResponse::GetEngram {
            item: brain
                .hippocampus
                .engrams
                .iter()
                .find(|e| e.id == engram_id)
                .map(engram_summary),
        },
        QueryRequest::Forget { .. } | QueryRequest::ForgetBefore { .. } => {
            QueryResponse::Forget { removed: false }
        }
        QueryRequest::SearchHybrid {
            cue,
            vector,
            top_k,
            agent_id,
        } => {
            let mut result = brain.activate_with_semantic(&cue, vector.as_deref());
            if let Some(ref aid) = agent_id {
                result
                    .recalls
                    .retain(|r| r.episode.agent_id.as_deref() == Some(aid.as_str()));
            }
            result.recalls.truncate(clamp_top_k(top_k));
            QueryResponse::SearchHybrid { activation: result }
        }
        QueryRequest::SearchByRag {
            doc_id,
            chunk_id,
            page,
            page_size,
        } => {
            if let Some(ref cid) = chunk_id {
                if let Some(id) = brain.hippocampus.find_rag_chunk(&doc_id, cid) {
                    let item = brain
                        .hippocampus
                        .engrams
                        .iter()
                        .find(|e| e.id == id)
                        .map(engram_summary);
                    return QueryResponse::SearchByRag {
                        total: usize::from(item.is_some()),
                        page,
                        items: item.into_iter().collect(),
                    };
                }
            }
            let filtered: Vec<_> = brain
                .hippocampus
                .engrams
                .iter()
                .filter(|e| {
                    e.episode.rag.as_ref().is_some_and(|r| {
                        r.doc_id.as_deref() == Some(doc_id.as_str())
                            && chunk_id
                                .as_ref()
                                .map(|c| r.chunk_id.as_deref() == Some(c.as_str()))
                                .unwrap_or(true)
                    })
                })
                .map(engram_summary)
                .collect();
            paginate_rag(filtered, page, page_size)
        }
        QueryRequest::Stats => {
            let s = brain.status();
            QueryResponse::Stats {
                stats: BrainQueryStats {
                    engrams: s.engrams,
                    synapses: s.synapses,
                    experiences: s.experiences,
                    sleep_cycles: s.sleep_cycles,
                    auto_sleeps: s.auto_sleeps,
                    synapse_pressure: s.synapse_pressure,
                },
            }
        }
    }
}

fn paginate(filtered: Vec<EngramSummary>, page: usize, page_size: usize) -> QueryResponse {
    let total = filtered.len();
    let start = page.saturating_mul(page_size);
    let items = filtered
        .into_iter()
        .skip(start)
        .take(clamp_page_size(page_size))
        .collect();
    QueryResponse::ListEngrams { total, page, items }
}

fn paginate_verified(filtered: Vec<EngramSummary>, page: usize, page_size: usize) -> QueryResponse {
    let total = filtered.len();
    let start = page.saturating_mul(page_size);
    let items = filtered
        .into_iter()
        .skip(start)
        .take(clamp_page_size(page_size))
        .collect();
    QueryResponse::ListVerified { total, page, items }
}

fn paginate_unverified(
    filtered: Vec<EngramSummary>,
    page: usize,
    page_size: usize,
) -> QueryResponse {
    let total = filtered.len();
    let start = page.saturating_mul(page_size);
    let items = filtered
        .into_iter()
        .skip(start)
        .take(clamp_page_size(page_size))
        .collect();
    QueryResponse::ListUnverified { total, page, items }
}

fn paginate_rag(filtered: Vec<EngramSummary>, page: usize, page_size: usize) -> QueryResponse {
    let total = filtered.len();
    let start = page.saturating_mul(page_size);
    let items = filtered
        .into_iter()
        .skip(start)
        .take(clamp_page_size(page_size))
        .collect();
    QueryResponse::SearchByRag { total, page, items }
}

fn is_unverified_factual(e: &crate::engram::Engram) -> bool {
    let verified = e
        .episode
        .provenance
        .as_ref()
        .map(|p| p.verified)
        .unwrap_or(false);
    if verified {
        return false;
    }
    let c = e.episode.content.to_lowercase();
    c.contains('$')
        || c.contains("balance")
        || c.contains("wallet")
        || c.contains("ledger")
        || c.chars().any(|ch| ch.is_ascii_digit())
}

fn engram_summary(e: &crate::engram::Engram) -> EngramSummary {
    let verified = e
        .episode
        .provenance
        .as_ref()
        .map(|p| p.verified)
        .unwrap_or(false);
    let provenance_kind = e
        .episode
        .provenance
        .as_ref()
        .map(|p| format!("{:?}", p.kind));
    let source_uri = e
        .episode
        .provenance
        .as_ref()
        .and_then(|p| p.source_uri.clone());
    let trust_note = if verified {
        None
    } else if is_unverified_factual(e) {
        Some("unverified factual claim — check ledger/tools".into())
    } else {
        None
    };
    EngramSummary {
        engram_id: e.id,
        content: e.episode.content.clone(),
        context: e.episode.context.clone(),
        agent_id: e.episode.agent_id.clone(),
        tenant_id: e.episode.tenant_id.clone(),
        encoded_at_tick: e.encoded_at_tick,
        salience: e.salience,
        is_core: e.is_core,
        verified,
        provenance_kind,
        source_uri,
        trust_note,
        rag: e.episode.rag.clone(),
    }
}

pub fn forget_engram(brain: &mut FluctlightBrain, engram_id: Uuid) -> bool {
    let before = brain.hippocampus.engrams.len();
    brain.hippocampus.engrams.retain(|e| e.id != engram_id);
    brain.semantic.engram_vectors.remove(&engram_id);
    brain.semantic.ec_semantic_neurons.remove(&engram_id);
    let _ = brain.remove_from_recall_index(engram_id);
    brain.invalidate_activation_cache();
    brain.hippocampus.engrams.len() < before
}

pub fn forget_before(brain: &mut FluctlightBrain, tick: u64) -> usize {
    let before = brain.hippocampus.engrams.len();
    let removed_ids: Vec<Uuid> = brain
        .hippocampus
        .engrams
        .iter()
        .filter(|e| e.encoded_at_tick < tick && !e.is_core)
        .map(|e| e.id)
        .collect();
    brain
        .hippocampus
        .engrams
        .retain(|e| e.encoded_at_tick >= tick || e.is_core);
    for id in &removed_ids {
        brain.semantic.engram_vectors.remove(id);
        brain.semantic.ec_semantic_neurons.remove(id);
        let _ = brain.remove_from_recall_index(*id);
    }
    brain.invalidate_activation_cache();
    before.saturating_sub(brain.hippocampus.engrams.len())
}

pub fn execute_mut(brain: &mut FluctlightBrain, req: QueryRequest) -> QueryResponse {
    match req {
        QueryRequest::Forget { engram_id } => QueryResponse::Forget {
            removed: forget_engram(brain, engram_id),
        },
        QueryRequest::ForgetBefore { tick } => QueryResponse::ForgetBefore {
            removed: forget_before(brain, tick),
        },
        other => execute(brain, other),
    }
}

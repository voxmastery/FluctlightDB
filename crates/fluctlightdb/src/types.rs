use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dentate::SeparationResult;
use crate::id::NeuronId;

/// One lived moment — the atomic unit of experience.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Episode {
    pub content: String,
    pub context: String,
    pub outcome: Option<String>,
    pub salience_hint: f32,
    /// Optional distributional embedding (entorhinal input). Recall still spreads through synapses.
    #[serde(default)]
    pub semantic_vector: Option<Vec<f32>>,
    /// Optional agent scope for multi-tenant recall isolation.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Optional tenant scope (storage routing hint).
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// RAG-adjunct metadata when this episode mirrors an external document chunk.
    #[serde(default)]
    pub rag: Option<RagRef>,
    /// Where this memory came from and whether it is grounded in external truth.
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

/// Memory provenance — separates chat assertions from verified ground truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Provenance {
    #[serde(default)]
    pub kind: ProvenanceKind,
    #[serde(default)]
    pub source_uri: Option<String>,
    /// 0..1 — ledger/file verified = high; chat claim = lower
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    #[default]
    ChatAssertion,
    FileObservation,
    ToolGrounded,
    LedgerVerified,
    UserExplicit,
}

/// External document chunk reference for RAG-adjunct memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RagRef {
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub doc_id: Option<String>,
    #[serde(default)]
    pub chunk_id: Option<String>,
}

impl Episode {
    pub fn new(content: impl Into<String>, context: impl Into<String>, salience_hint: f32) -> Self {
        Self {
            content: content.into(),
            context: context.into(),
            outcome: None,
            salience_hint,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        }
    }
}

/// Returned when an experience is encoded — includes separation telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceReport {
    pub engram_id: Uuid,
    pub separation: SeparationResult,
    #[serde(default)]
    pub deduplicated: bool,
    #[serde(default)]
    pub gate_rejected: bool,
    #[serde(default)]
    pub confusion_risk: f32,
    #[serde(default)]
    pub gate_reason: Option<String>,
}

impl ExperienceReport {
    pub fn ok(engram_id: Uuid, separation: SeparationResult, deduplicated: bool) -> Self {
        Self {
            engram_id,
            separation,
            deduplicated,
            gate_rejected: false,
            confusion_risk: 0.0,
            gate_reason: None,
        }
    }

    pub fn dedup(engram_id: Uuid) -> Self {
        Self {
            engram_id,
            separation: SeparationResult {
                ec_neurons: vec![],
                dg_neurons: vec![],
                ca3_neurons: vec![],
                separation_index: 1.0,
                max_overlap_before: 0.0,
                max_overlap_after: 0.0,
                separators_added: 0,
                token_count: 0,
            },
            deduplicated: true,
            gate_rejected: false,
            confusion_risk: 0.0,
            gate_reason: None,
        }
    }
}

/// Result of spreading activation recall (not vector similarity).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallResult {
    pub engram_id: Uuid,
    pub activation: f32,
    pub episode: Episode,
    pub completion_strength: f32,
    #[serde(default)]
    pub separation_index: f32,
    /// Whether episode provenance is verified ground truth.
    #[serde(default)]
    pub verified: bool,
    /// Human hint when recall may be chat assertion, not fact.
    #[serde(default)]
    pub trust_note: Option<String>,
}

/// Full activation wave output.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ActivationResult {
    pub recalls: Vec<RecallResult>,
    pub active_neurons: usize,
    pub hops: u32,
    pub myelinated: bool,
}

/// Report from one sleep / consolidation cycle.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SleepReport {
    pub replays: u32,
    pub consolidated: u32,
    pub pruned_synapses: u32,
    pub stage_before: String,
    pub stage_after: String,
    pub advanced: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Region {
    Prefrontal,
    HippocampusDg,
    HippocampusCa3,
    HippocampusCa1,
    Amygdala,
    Cortex,
    Brainstem,
}

impl Region {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prefrontal => "prefrontal",
            Self::HippocampusDg => "hippocampus_dg",
            Self::HippocampusCa3 => "hippocampus_ca3",
            Self::HippocampusCa1 => "hippocampus_ca1",
            Self::Amygdala => "amygdala",
            Self::Cortex => "cortex",
            Self::Brainstem => "brainstem",
        }
    }
}

/// Map tokens to sparse neuron IDs (content-addressed, not embeddings).
pub fn tokens_to_neurons(tokens: &[String]) -> Vec<NeuronId> {
    let mut ids: Vec<NeuronId> = tokens.iter().map(|t| NeuronId::from_token(t)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// JSON export for visual dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VizExport {
    pub stage: String,
    pub tick: u64,
    pub synapses: usize,
    pub engrams: usize,
    pub synapse_pressure: f32,
    pub ticks_since_sleep: u64,
    pub recent_separations: Vec<SeparationResult>,
    pub development: DevelopmentViz,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DevelopmentViz {
    pub experiences: u64,
    pub sleep_cycles: u64,
    pub pruned_synapses: u64,
}

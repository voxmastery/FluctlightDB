//! # FluctlightDB
//!
//! A brain-native storage and cognition engine for AI agents.
//! Not a vector database. Not SQL. Memory as engrams, recall as activation.

pub mod activation;
pub mod agent_prompt;
pub mod agent_runtime;
pub mod amygdala;
pub mod api_slim;
pub mod auth;
pub mod auth_store;
pub mod autonomic;
pub mod brain;
pub mod brain_snapshot;
pub mod budget;
pub mod cache;
pub mod calcium;
pub mod checkpoint_fault;
pub mod checkpoint_policy;
pub mod chorus;
pub mod chorus_runtime;
pub mod chronos;
pub mod compact;
pub mod confidence;
pub mod config;
pub mod conflict_lattice;
pub mod consensus;
#[cfg(feature = "distributed")]
pub mod control;
pub mod cortex;
#[cfg(feature = "cortex-sim")]
pub mod cortex_sim;
pub mod crystallize;
pub mod dentate;
pub mod derive;
pub mod development;
pub mod engram;
pub mod error;
pub mod fabric_runtime;
pub mod forgetting;
pub mod fovea;
pub mod governance;
pub mod homeostasis;
pub mod graph;
pub mod graph_export;
pub mod hippocampus;
pub mod id;
pub mod index;
pub mod late_interaction;
pub mod lattice;
pub mod legacy_hippocampus;
pub mod life;
pub mod manifest;
pub mod metrics;
pub mod muon;
pub mod muon_runtime;
pub mod neurogenesis;
pub mod neuromodulator;
pub mod neuron;
pub mod partial;
pub mod phase_parse;
pub mod photon;
pub mod placement;
pub mod plasticity;
pub mod prefrontal;
pub mod preplay;
pub mod prism;
pub mod query;
pub mod rate_limit;
pub mod raw_export;
pub mod reality;
pub mod recall_fabric;
pub mod recall_router;
pub mod relation;
pub mod replicate;
pub mod retention_policy;
pub mod capture_gate;
pub mod eligibility;
pub mod schema;
pub mod segment;
pub mod semantic;
pub mod separation_gate;
pub mod serve;
pub mod shard;
pub mod sleep;
pub mod sleep_trigger;
pub mod somnus;
pub mod spectrum;
pub mod stage_schedule;
pub mod storage;
pub mod store;
pub mod store_lock;
pub mod tau;
pub mod tau_runtime;
pub mod tenant;
/// Process-wide env lock for tests that mutate `FLUCTLIGHT_*` (unit + integration).
#[doc(hidden)]
pub mod test_env;
pub mod tokenize;
pub mod types;
pub mod wal;
pub mod wal_sync;
pub mod wm_ring;

pub use agent_prompt::AgentPromptBundle;
pub use agent_runtime::{enable_agent_env, AgentState, ConsolidateReport, ToolObserveInput};
pub use autonomic::{AutonomicConfig, AutonomicState, TickReport};
pub use brain::{BrainStatus, FluctlightBrain};
pub use brain_snapshot::{
    export_snapshot_json, import_snapshot, import_snapshot_json, BrainSnapshot,
    SnapshotImportReport, SNAPSHOT_FORMAT, SNAPSHOT_VERSION,
};
pub use homeostasis::HomeostasisReport;
pub use cache::ActivationCache;
pub use chorus::{
    ChorusConfig, ChorusField, ChorusHit, ChorusImprintInput, ChorusRecallOpts, ChorusSleepReport,
    ChorusTrace, Complex as ChorusComplex, ProvenanceSheath,
};
pub use chronos::{Chronos, Event};
pub use compact::CompactReport;
pub use confidence::{recall_confidence, Evidence, SourceKind};
pub use conflict_lattice::ResolvedFact;
pub use consensus::{Claim, Consensus, SharedMemory};
pub use crystallize::{Crystal, Crystallizer};
pub use dentate::SeparationResult;
pub use development::{DevStage, DevelopmentState};
pub use engram::Engram;
pub use error::{Error, Result};
pub use forgetting::{interference, LoadController, MemoryTrace};
pub use fovea::{scan_file, scan_text, FoveaConfig, FoveaPacket};
pub use governance::{AuditEntry, DeleteBySubjectReport, GovernanceState, PiiScrubReport};
pub use graph_export::GraphExport;
pub use index::RecallIndex;
pub use lattice::{Axis, GridCode, Lattice, LatticeCode, LatticeStore};
pub use life::{CoreMemory, LifeState};
pub use manifest::{load_v4_dir, migrate_v3_file_to_v4, save_v4_dir};
pub use muon::{MuonHit, MuonImprintInput, MuonLane};
pub use neurogenesis::NeurogenesisReport;
pub use phase_parse::{Codebook, PhaseParser, PhaseVector};
pub use photon::{PhotonCode, PhotonStore, SimHasher};
pub use preplay::{PreplayResult, PreplayStep};
pub use prism::{PrismCode, PrismSignature, DEFAULT_CERTIFY_M, DEFAULT_PRISM_FULL_MAX};
pub use raw_export::{import_raw, import_raw_json, RawExport, RawImportReport};
pub use reality::{VerifiedContext, VerifiedFact};
pub use recall_fabric::{FabricConfig, FabricHit, RecallFabric};
pub use recall_router::{RecallMode, TemporalFilter, UnifiedRecallHit, UnifiedRecallResult};
pub use relation::{extract_relations, Relation};
pub use replicate::open_replica_brain;
pub use retention_policy::{RetentionPolicy, RetentionReport, RetentionState};
pub use schema::{Schema, SchemaAwareActivation, SchemaStatus, SchemaStore};
pub use semantic::{SemanticField, DEFAULT_SEMANTIC_DIM};
pub use separation_gate::SeparationGateResult;
pub use serve::request_shutdown;
pub use serve::reset_shutdown_for_tests;
pub use serve::BrainServer;
pub use spectrum::{SpectrumSignature, DEFAULT_FULL_READOUT_MAX};
pub use stage_schedule::StageConsolidationReport;
pub use storage::{default_brain_path, default_tenant_brain_dir, StorageFormat};
pub use store::{verify_path, BrainVerifyReport};
pub use store_lock::{SharedStoreLock, StoreLock};
pub use tau::{TauHit, TauLane, TauShard};
pub use types::{
    ActivationResult, Episode, ExperienceReport, Provenance, ProvenanceKind, RecallResult,
    SleepReport, VizExport,
};
pub use wm_ring::{WmFlushReport, WmRing, WmSlot};

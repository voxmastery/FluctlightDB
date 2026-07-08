//! # FluctlightDB
//!
//! A brain-native storage and cognition engine for AI agents.
//! Not a vector database. Not SQL. Memory as engrams, recall as activation.

pub mod activation;
pub mod agent_runtime;
pub mod amygdala;
pub mod api_slim;
pub mod auth;
pub mod auth_store;
pub mod autonomic;
pub mod brain;
pub mod budget;
pub mod cache;
pub mod checkpoint_policy;
pub mod chronos;
pub mod chorus;
pub mod chorus_runtime;
pub mod compact;
pub mod confidence;
pub mod conflict_lattice;
pub mod consensus;
pub mod cortex;
pub mod crystallize;
pub mod fabric_runtime;
pub mod dentate;
pub mod development;
pub mod engram;
pub mod error;
pub mod fovea;
pub mod graph;
pub mod graph_export;
pub mod brain_snapshot;
pub mod governance;
pub mod hippocampus;
pub mod id;
pub mod index;
pub mod lattice;
pub mod legacy_hippocampus;
pub mod life;
pub mod manifest;
pub mod metrics;
pub mod muon;
pub mod muon_runtime;
pub mod tau;
pub mod tau_runtime;
pub mod forgetting;
pub mod neurogenesis;
pub mod neuromodulator;
pub mod partial;
pub mod phase_parse;
pub mod photon;
pub mod spectrum;
pub mod plasticity;
pub mod prefrontal;
pub mod preplay;
pub mod query;
pub mod rate_limit;
pub mod recall_fabric;
pub mod recall_router;
pub mod relation;
pub mod raw_export;
pub mod retention_policy;
pub mod reality;
pub mod replicate;
pub mod segment;
pub mod semantic;
pub mod separation_gate;
pub mod serve;
pub mod shard;
pub mod sleep;
pub mod sleep_trigger;
pub mod stage_schedule;
pub mod storage;
pub mod store;
pub mod store_lock;
pub mod tenant;
pub mod tokenize;
pub mod types;
pub mod wm_ring;
pub mod wal;
pub mod wal_sync;

pub use agent_runtime::{
    enable_agent_env, AgentState, ConsolidateReport, ToolObserveInput,
};
pub use autonomic::{AutonomicConfig, AutonomicState, TickReport};
pub use conflict_lattice::ResolvedFact;
pub use recall_router::{RecallMode, TemporalFilter, UnifiedRecallHit, UnifiedRecallResult};
pub use retention_policy::{RetentionPolicy, RetentionReport, RetentionState};
pub use wm_ring::{WmFlushReport, WmRing, WmSlot};
pub use brain::{BrainStatus, FluctlightBrain};
pub use cache::ActivationCache;
pub use compact::CompactReport;
pub use dentate::SeparationResult;
pub use development::{DevStage, DevelopmentState};
pub use engram::Engram;
pub use error::{Error, Result};
pub use fovea::{scan_file, scan_text, FoveaConfig, FoveaPacket};
pub use brain_snapshot::{
    export_snapshot_json, import_snapshot, import_snapshot_json, BrainSnapshot,
    SnapshotImportReport, SNAPSHOT_FORMAT, SNAPSHOT_VERSION,
};
pub use governance::{
    AuditEntry, DeleteBySubjectReport, GovernanceState, PiiScrubReport,
};
pub use graph_export::GraphExport;
pub use index::RecallIndex;
pub use lattice::{Axis, GridCode, Lattice, LatticeCode, LatticeStore};
pub use phase_parse::{Codebook, PhaseParser, PhaseVector};
pub use photon::{PhotonCode, PhotonStore, SimHasher};
pub use spectrum::{SpectrumSignature, DEFAULT_FULL_READOUT_MAX};
pub use muon::{MuonHit, MuonImprintInput, MuonLane};
pub use tau::{TauHit, TauLane, TauShard};
pub use chronos::{Chronos, Event};
pub use chorus::{
    ChorusConfig, ChorusField, ChorusHit, ChorusImprintInput, ChorusRecallOpts, ChorusSleepReport,
    ChorusTrace,
    Complex as ChorusComplex, ProvenanceSheath,
};
pub use confidence::{recall_confidence, Evidence, SourceKind};
pub use consensus::{Claim, Consensus, SharedMemory};
pub use crystallize::{Crystal, Crystallizer};
pub use forgetting::{interference, LoadController, MemoryTrace};
pub use recall_fabric::{FabricConfig, FabricHit, RecallFabric};
pub use relation::{extract_relations, Relation};
pub use life::{CoreMemory, LifeState};
pub use manifest::{load_v4_dir, migrate_v3_file_to_v4, save_v4_dir};
pub use neurogenesis::NeurogenesisReport;
pub use preplay::{PreplayResult, PreplayStep};
pub use raw_export::{import_raw, import_raw_json, RawExport, RawImportReport};
pub use reality::{VerifiedContext, VerifiedFact};
pub use replicate::{open_replica_brain, sync_once, ReplicaStatus};
pub use semantic::{SemanticField, DEFAULT_SEMANTIC_DIM};
pub use separation_gate::SeparationGateResult;
pub use serve::request_shutdown;
pub use serve::BrainServer;
pub use stage_schedule::StageConsolidationReport;
pub use storage::{default_brain_path, default_tenant_brain_dir, StorageFormat};
pub use store::{verify_path, BrainVerifyReport};
pub use store_lock::{SharedStoreLock, StoreLock};
pub use types::{
    ActivationResult, Episode, ExperienceReport, Provenance, ProvenanceKind, RecallResult,
    SleepReport, VizExport,
};

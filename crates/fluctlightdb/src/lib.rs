//! # FluctlightDB
//!
//! A brain-native storage and cognition engine for AI agents.
//! Not a vector database. Not SQL. Memory as engrams, recall as activation.

pub mod activation;
pub mod api_slim;
pub mod amygdala;
pub mod auth;
pub mod auth_store;
pub mod autonomic;
pub mod brain;
pub mod budget;
pub mod cache;
pub mod checkpoint_policy;
pub mod compact;
pub mod cortex;
pub mod dentate;
pub mod development;
pub mod engram;
pub mod error;
pub mod fovea;
pub mod graph;
pub mod graph_export;
pub mod hippocampus;
pub mod id;
pub mod index;
pub mod legacy_hippocampus;
pub mod life;
pub mod manifest;
pub mod metrics;
pub mod neuromodulator;
pub mod partial;
pub mod plasticity;
pub mod prefrontal;
pub mod query;
pub mod rate_limit;
pub mod raw_export;
pub mod replicate;
pub mod segment;
pub mod shard;
pub mod semantic;
pub mod serve;
pub mod sleep;
pub mod sleep_trigger;
pub mod storage;
pub mod neurogenesis;
pub mod preplay;
pub mod reality;
pub mod separation_gate;
pub mod stage_schedule;
pub mod store;
pub mod store_lock;
pub mod tenant;
pub mod tokenize;
pub mod types;
pub mod wal;
pub mod wal_sync;

pub use autonomic::{AutonomicConfig, AutonomicState, TickReport};
pub use cache::ActivationCache;
pub use index::RecallIndex;
pub use brain::{BrainStatus, FluctlightBrain};
pub use compact::CompactReport;
pub use dentate::SeparationResult;
pub use development::{DevStage, DevelopmentState};
pub use engram::Engram;
pub use error::{Error, Result};
pub use life::{CoreMemory, LifeState};
pub use graph_export::GraphExport;
pub use raw_export::{import_raw, import_raw_json, RawExport, RawImportReport};
pub use replicate::{open_replica_brain, sync_once, ReplicaStatus};
pub use manifest::{migrate_v3_file_to_v4, load_v4_dir, save_v4_dir};
pub use storage::{default_brain_path, default_tenant_brain_dir, StorageFormat};
pub use store::{verify_path, BrainVerifyReport};
pub use store_lock::{SharedStoreLock, StoreLock};
pub use fovea::{scan_file, scan_text, FoveaConfig, FoveaPacket};
pub use neurogenesis::NeurogenesisReport;
pub use preplay::{PreplayResult, PreplayStep};
pub use reality::{VerifiedContext, VerifiedFact};
pub use separation_gate::SeparationGateResult;
pub use stage_schedule::StageConsolidationReport;
pub use semantic::{SemanticField, DEFAULT_SEMANTIC_DIM};
pub use serve::BrainServer;
pub use serve::request_shutdown;
pub use types::{
    ActivationResult, Episode, ExperienceReport, Provenance, ProvenanceKind, RecallResult,
    SleepReport, VizExport,
};

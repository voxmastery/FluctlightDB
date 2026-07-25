use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::activation::{activate_from_hybrid, cap_candidates, complete, default_candidate_cap};
use crate::agent_runtime::AgentState;
use crate::amygdala::Amygdala;
use crate::autonomic::{AutonomicState, TickReport};
use crate::budget::{self, WiringBudget, PRESSURE_COMPACT_THRESHOLD};
use crate::cache::ActivationCache;
use crate::checkpoint_policy::CheckpointPolicy;
use crate::compact::{compact_brain, CompactReport};
use crate::cortex::Cortex;
use crate::dentate::SeparationResult;
use crate::development::{DevStage, DevelopmentState};
use crate::engram::Engram;
use crate::error::{Error, Result};
use crate::graph::BrainGraph;
use crate::graph_export::{export_graph, export_graph_lite, GraphExport};
use crate::hippocampus::Hippocampus;
use crate::index::RecallIndex;
use crate::life::{CoreMemoryStore, LifeState};
use crate::neuromodulator::Neuromodulators;
use crate::prefrontal::{Prefrontal, RuleAction};
use crate::raw_export::{export_raw, RawExport};
use crate::semantic::SemanticField;
use crate::sleep::{separate_and_encode, sleep_cycle};
use crate::sleep_trigger::SleepTrigger;
use crate::store;
use crate::store_lock::{SharedStoreLock, StoreLock};
use crate::types::Region::HippocampusCa1;
use crate::types::{
    ActivationResult, DevelopmentViz, Episode, ExperienceReport, ProvenanceKind, SleepReport,
    VizExport,
};
use crate::wal::{self, WalEntry, WalIdentity};

const MAX_RECENT_SEPARATIONS: usize = 12;
const COMPACT_EVERY_N_SLEEPS: u64 = 48;

/// The living brain — main API for agents.
#[derive(Serialize, Deserialize)]
pub struct FluctlightBrain {
    #[serde(default)]
    pub wal_seq: u64,
    pub life: LifeState,
    pub development: DevelopmentState,
    pub neuromodulators: Neuromodulators,
    pub graph: BrainGraph,
    pub hippocampus: Hippocampus,
    pub cortex: Cortex,
    pub amygdala: Amygdala,
    pub prefrontal: Prefrontal,
    pub core_memories: CoreMemoryStore,
    pub autonomic: AutonomicState,
    #[serde(default)]
    pub agent: AgentState,
    #[serde(default)]
    pub governance: crate::governance::GovernanceState,
    #[serde(default)]
    pub semantic: SemanticField,
    #[serde(default)]
    pub recent_separations: Vec<SeparationResult>,
    #[serde(skip)]
    checkpoint_policy: CheckpointPolicy,
    /// Runtime counter for autonomic Somnus durability seals (not semantic sleep).
    #[serde(skip)]
    ticks_since_systems_seal: u64,
    /// Wake WAL records since last systems seal (Somnus pressure trigger).
    #[serde(skip)]
    wal_records_since_seal: u64,
    /// Organ health metrics (measurement only).
    #[serde(skip)]
    pub(crate) homeostasis: crate::homeostasis::HomeostasisState,
    #[serde(skip)]
    store_path: Option<PathBuf>,
    #[serde(skip)]
    wal_identity: Option<WalIdentity>,
    #[serde(skip)]
    store_lock: Option<BrainStoreLock>,
    #[serde(skip)]
    recall_index: Option<RecallIndex>,
    #[serde(skip)]
    activation_cache: Mutex<ActivationCache>,
    /// Runtime-only temporal/causal index (Recall Fabric). Rebuilt per session, never persisted.
    #[serde(skip)]
    pub(crate) chronos: crate::chronos::Chronos,
    /// Runtime-only consolidated cortical map (Recall Fabric). Rebuilt per session, never persisted.
    #[serde(skip)]
    pub(crate) crystallizer: crate::crystallize::Crystallizer,
    /// Runtime-only composed recall index (Photon + Lattice + Phase). Never persisted.
    #[serde(skip)]
    pub(crate) fabric: crate::recall_fabric::RecallFabric,
    /// Runtime-only Ebbinghaus traces keyed by engram id. Never persisted.
    #[serde(skip)]
    pub(crate) fabric_traces: Mutex<HashMap<String, crate::forgetting::MemoryTrace>>,
    /// Runtime-only multi-agent shared claims (Recall Fabric). Never persisted.
    #[serde(skip)]
    pub(crate) consensus: crate::consensus::SharedMemory,
    /// Runtime-only penetrative session imprints (Muon Lane). Never persisted.
    #[serde(skip)]
    pub(crate) muon: crate::muon::MuonLane,
    /// Runtime-only episodic fission shards (Tau Lane). Never persisted.
    #[serde(skip)]
    pub(crate) tau: crate::tau::TauLane,
    /// Runtime-only CHORUS phase field (θ–γ wavelet substrate). Never persisted.
    #[serde(skip)]
    pub(crate) chorus: crate::chorus::ChorusField,
}

pub(crate) enum BrainStoreLock {
    Exclusive(StoreLock),
    Shared(SharedStoreLock),
}

impl Default for FluctlightBrain {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedMemory {
    pub engram_id: uuid::Uuid,
    pub content: String,
    pub context: String,
    pub salience: f32,
}

impl FluctlightBrain {
    pub fn new() -> Self {
        let (fabric, fabric_traces) = crate::fabric_runtime::new_fabric_state();
        let mut brain = Self {
            wal_seq: 0,
            life: LifeState::birth(0),
            development: DevelopmentState::default(),
            neuromodulators: Neuromodulators::default(),
            graph: BrainGraph::default(),
            hippocampus: Hippocampus::default(),
            cortex: Cortex::default(),
            amygdala: Amygdala::default(),
            prefrontal: Prefrontal::default(),
            core_memories: CoreMemoryStore::default(),
            autonomic: AutonomicState::new(),
            agent: AgentState::default(),
            governance: crate::governance::GovernanceState::default(),
            semantic: SemanticField::default(),
            recent_separations: Vec::new(),
            checkpoint_policy: CheckpointPolicy::default(),
            ticks_since_systems_seal: 0,
            wal_records_since_seal: 0,
            homeostasis: crate::homeostasis::HomeostasisState::default(),
            store_path: None,
            wal_identity: None,
            store_lock: None,
            recall_index: None,
            activation_cache: Mutex::new(ActivationCache::new()),
            chronos: crate::chronos::Chronos::default(),
            crystallizer: crate::crystallize::Crystallizer::default(),
            fabric,
            fabric_traces,
            consensus: crate::consensus::SharedMemory::default(),
            muon: crate::muon_runtime::new_muon_lane(),
            tau: crate::tau_runtime::new_tau_lane(),
            chorus: crate::chorus_runtime::new_chorus_field(),
        };
        brain.development.on_tick();
        brain.prefrontal.unlocked = brain.development.pfc_unlocked();
        brain
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        store::load(path.as_ref())
    }

    pub fn open_readonly(path: impl AsRef<Path>) -> Result<Self> {
        store::load_readonly(path.as_ref())
    }

    pub fn save(&self) -> Result<()> {
        self.checkpoint()
    }

    pub fn checkpoint(&self) -> Result<()> {
        if self.store_is_readonly() {
            return Err(Error::Store("cannot checkpoint a read-only brain".into()));
        }
        if let Some(ref path) = self.store_path {
            store::save_locked(self, path)?;
            if crate::somnus::somnus_enabled() {
                let _ = crate::manifest::prune_old_generations(path, crate::somnus::somnus_keep())?;
            }
        }
        Ok(())
    }

    fn store_is_readonly(&self) -> bool {
        match &self.store_lock {
            Some(BrainStoreLock::Shared(lock)) => {
                let _ = lock;
                true
            }
            Some(BrainStoreLock::Exclusive(lock)) => {
                let _ = lock;
                false
            }
            None => false,
        }
    }

    pub fn maybe_checkpoint(&mut self) -> Result<()> {
        // Somnus (default): wake activity is hippocampal WAL/trace only.
        // Full v4 systems seals happen on sleep / explicit checkpoint().
        if crate::somnus::somnus_enabled() {
            return Ok(());
        }
        self.checkpoint_policy.note_write();
        if self.checkpoint_policy.should_checkpoint() {
            self.checkpoint()?;
            self.checkpoint_policy.mark_checkpointed();
        }
        Ok(())
    }

    /// Systems consolidation seal — immutable generation + prune obsolete seals.
    ///
    /// Durability only: does not run semantic `sleep_cycle` (no synapse prune / crystallize).
    /// Safe for benchmarks — recall ranking is unchanged by this call.
    pub fn systems_seal(&mut self) -> Result<()> {
        self.checkpoint()?;
        self.checkpoint_policy.mark_checkpointed();
        self.ticks_since_systems_seal = 0;
        self.wal_records_since_seal = 0;
        self.homeostasis.note_systems_seal();
        Ok(())
    }

    /// Autonomic Somnus durability: seal without semantic sleep when due.
    ///
    /// No-op when Somnus is debug-disabled or semantic sleep already sealed this tick.
    /// Never mutates the recall graph. Seals on earlier of tick cadence or WAL pressure.
    fn maybe_somnus_autonomic_seal(&mut self, checkpoint: bool, already_sealed: bool) -> Result<bool> {
        if !checkpoint || already_sealed || !crate::somnus::somnus_enabled() {
            return Ok(false);
        }
        let every_ticks = crate::somnus::somnus_seal_every_ticks();
        let every_wal = crate::somnus::somnus_seal_every_wal_records();
        self.ticks_since_systems_seal = self.ticks_since_systems_seal.saturating_add(1);
        let tick_due = every_ticks > 0 && self.ticks_since_systems_seal >= every_ticks;
        let wal_due = every_wal > 0 && self.wal_records_since_seal >= every_wal;
        if !tick_due && !wal_due {
            return Ok(false);
        }
        self.systems_seal()?;
        Ok(true)
    }

    fn wal_append(&mut self, entry: WalEntry) -> Result<()> {
        if let Some(ref path) = self.store_path {
            self.wal_seq += 1;
            self.wal_records_since_seal = self.wal_records_since_seal.saturating_add(1);
            if let Some(identity) = self.wal_identity {
                wal::append_fenced(path, self.wal_seq, &entry, &identity)?;
            } else {
                wal::append(path, self.wal_seq, &entry)?;
            }
        }
        Ok(())
    }

    pub fn set_wal_identity(&mut self, identity: Option<WalIdentity>) {
        self.wal_identity = identity;
    }

    pub fn wal_identity(&self) -> Option<WalIdentity> {
        self.wal_identity
    }

    #[cfg(feature = "distributed")]
    pub(crate) fn store_path(&self) -> Option<&Path> {
        self.store_path.as_deref()
    }

    pub(crate) fn reject_distributed_mutation(&self, operation: &'static str) -> Result<()> {
        if self.wal_identity.is_some() {
            return Err(Error::DistributedMutationDisabled { operation });
        }
        Ok(())
    }

    pub fn stage(&self) -> DevStage {
        self.development.stage
    }

    /// Encode lived experience — DG separate + CA3 wire + store engram.
    pub fn experience(&mut self, episode: Episode) -> Result<ExperienceReport> {
        let assigned_engram_id = Uuid::new_v4();
        if wal::wal_enabled() {
            self.wal_append(WalEntry::Experience {
                episode: episode.clone(),
                assigned_engram_id: Some(assigned_engram_id),
            })?;
        }
        let report = self.experience_internal_assigned(episode, true, Some(assigned_engram_id))?;
        self.agent_on_experience(report.engram_id);
        if !report.gate_rejected && !report.deduplicated {
            self.cortex.eligibility.tag(report.engram_id);
        }
        Ok(report)
    }

    pub(crate) fn experience_internal(
        &mut self,
        episode: Episode,
        checkpoint: bool,
    ) -> Result<ExperienceReport> {
        self.experience_internal_assigned(episode, checkpoint, None)
    }

    pub(crate) fn experience_internal_assigned(
        &mut self,
        episode: Episode,
        checkpoint: bool,
        assigned_engram_id: Option<Uuid>,
    ) -> Result<ExperienceReport> {
        if !self.life.alive {
            return Err(Error::LifeEnded);
        }

        if self.development.stage == DevStage::Embryonic && !episode.content.starts_with("reflex:")
        {
            return Err(Error::EmbryonicOnlyReflex);
        }

        if let Some(ref rag) = episode.rag {
            if let (Some(doc), Some(chunk)) = (&rag.doc_id, &rag.chunk_id) {
                if let Some(existing) = self.hippocampus.find_rag_chunk(doc, chunk) {
                    return Ok(ExperienceReport::dedup(existing));
                }
            }
        }

        let salience =
            (episode.salience_hint + self.amygdala.weight_for(Uuid::nil())).clamp(0.0, 1.0);

        // Index-mode IR benchmarks: O(1) ingest per doc when caller supplies a dense vector.
        // Without semantic_vector, fall through to dentate/graph wiring so lexical recall works.
        if crate::activation::fast_ingest_mode()
            && crate::activation::vector_fast_mode()
            && episode.semantic_vector.is_some()
        {
            let tick = self.development.metrics.ticks;
            let engram_id = assigned_engram_id.unwrap_or_else(Uuid::new_v4);
            let rich = crate::tokenize::tokenize_rich(
                &episode.content,
                &episode.context,
                episode.outcome.as_deref(),
            );
            let ec_neurons: Vec<crate::id::NeuronId> = rich
                .iter()
                .map(|t| crate::id::NeuronId::from_seeds(&["ec", &t.surface]))
                .collect();
            let separation = SeparationResult {
                ec_neurons: ec_neurons.clone(),
                dg_neurons: vec![],
                ca3_neurons: vec![],
                separation_index: 1.0,
                max_overlap_before: 0.0,
                max_overlap_after: 0.0,
                separators_added: 0,
                token_count: rich.len(),
            };
            let mut engram = Engram {
                id: engram_id,
                life_id: self.life.life_id,
                neurons: ec_neurons.clone(),
                ec_neurons: ec_neurons.clone(),
                dg_neurons: vec![],
                separation_index: 1.0,
                episode: episode.clone(),
                salience,
                encoded_at_tick: tick,
                encoded_at_stage: self.development.stage as u8,
                replay_count: 0,
                is_core: false,
            };
            if let Some(ref vector) = episode.semantic_vector {
                let ec_sem =
                    self.semantic
                        .register_engram(engram_id, self.life.life_id, vector.clone());
                engram.ec_neurons.extend(ec_sem);
            }
            self.amygdala.tag(engram_id, salience);
            self.hippocampus.encode(engram);
            let vector_for_index = episode.semantic_vector.clone();
            self.index_engram(engram_id, &episode.content, vector_for_index.as_deref());
            self.activation_cache.lock().unwrap().invalidate();
            self.development.on_experience(salience);
            self.prefrontal.unlocked = self.development.pfc_unlocked();
            self.fabric_on_experience(
                engram_id,
                &episode.content,
                salience,
                tick,
                vector_for_index.as_deref(),
            );
            if checkpoint {
                self.maybe_checkpoint()?;
            }
            return Ok(ExperienceReport::ok(engram_id, separation, false));
        }
        let gate = self.neuromodulators.plasticity_gate(salience);
        if episode.salience_hint > 0.5 {
            self.neuromodulators.on_surprise(episode.salience_hint);
        }

        // Prediction error signal (Schultz 1997; O'Reilly & Frank 2006):
        // Use the cortex's prior knowledge of this content as the "expected" activation.
        // High cortex prior (content seen before) = expected; low = truly unexpected.
        // PE = actual_salience − expected → DA/NE update before encoding.
        let expected_activation = (self.cortex.fact_boost(&episode.content)
            + self.cortex.fact_boost(&episode.context) * 0.5)
            .clamp(0.0, 1.0);
        self.neuromodulators
            .prediction_error(expected_activation, salience);

        let verified = episode
            .provenance
            .as_ref()
            .map(|p| p.verified)
            .unwrap_or(false);
        if crate::separation_gate::gate_enabled()
            && !verified
            && !episode.context.starts_with("ledger:")
        {
            let gate =
                crate::separation_gate::assess(&self.hippocampus, &episode, self.life.life_id);
            if !gate.allowed {
                return Ok(ExperienceReport {
                    engram_id: Uuid::nil(),
                    separation: crate::dentate::SeparationResult {
                        ec_neurons: vec![],
                        dg_neurons: vec![],
                        ca3_neurons: vec![],
                        separation_index: gate.separation_index,
                        max_overlap_before: gate.max_overlap,
                        max_overlap_after: gate.max_overlap,
                        separators_added: 0,
                        token_count: 0,
                    },
                    deduplicated: false,
                    gate_rejected: true,
                    confusion_risk: gate.confusion_risk,
                    gate_reason: gate.reason,
                });
            }
        }

        let tick = self.development.metrics.ticks;
        let budget = WiringBudget::for_stage(self.development.stage);
        let (mut engram, separation) = separate_and_encode(
            &mut self.graph,
            &self.hippocampus,
            &episode,
            self.life.life_id,
            tick,
            self.development.stage as u8,
            salience,
            assigned_engram_id.unwrap_or_else(Uuid::new_v4),
        );

        // ACh novelty/familiarity signal (Hasselmo 2006):
        // High separation_index → truly novel pattern → DG orthogonalised it heavily →
        //   raise ACh to suppress CA3 recurrence and encode cleanly without interference.
        // Low separation_index → familiar / near-duplicate → lower ACh to open CA3
        //   recurrent collaterals so subsequent retrieval can pattern-complete from partial cues.
        if separation.separation_index > 0.6 {
            self.neuromodulators.on_novelty();
        } else if separation.separation_index < 0.25 {
            self.neuromodulators.on_retrieval();
        }

        if let Some(ref vector) = episode.semantic_vector {
            let ec_sem =
                self.semantic
                    .register_engram(engram.id, self.life.life_id, vector.clone());
            for &n in &ec_sem {
                engram.ec_neurons.push(n);
                self.graph.register_neuron(n, HippocampusCa1);
            }
            budget::wire_chain(
                &mut self.graph,
                &ec_sem,
                HippocampusCa1,
                0.25,
                budget.max_semantic_ec_links,
            );
            budget::wire_dg_to_ec(
                &mut self.graph,
                &engram.dg_neurons,
                &ec_sem,
                budget.max_dg_to_ec_links,
            );
        }

        let pressure = self.autonomic.synapse_pressure(
            self.graph.synapse_count(),
            self.development.stage.max_synapses(),
        );
        if pressure >= PRESSURE_COMPACT_THRESHOLD {
            let _ = self.compact_internal(false);
        }

        let engram_id = engram.id;
        self.amygdala.tag(engram_id, salience);
        self.hippocampus.encode(engram);

        let content_for_index = self
            .hippocampus
            .engrams
            .last()
            .map(|e| e.episode.content.clone())
            .unwrap_or_else(|| episode.content.clone());
        let vector_for_index = episode
            .semantic_vector
            .clone()
            .or_else(|| self.semantic.engram_vectors.get(&engram_id).cloned());
        self.index_engram(engram_id, &content_for_index, vector_for_index.as_deref());
        self.activation_cache.lock().unwrap().invalidate();

        let active =
            crate::activation::active_set_from_engram(self.hippocampus.engrams.last().unwrap());
        self.graph.co_activate(&active, gate);

        self.push_separation(separation.clone());
        self.development.on_experience(salience);
        self.prefrontal.unlocked = self.development.pfc_unlocked();

        self.fabric_on_experience(
            engram_id,
            &content_for_index,
            salience,
            tick,
            vector_for_index.as_deref(),
        );

        if checkpoint {
            self.maybe_checkpoint()?;
        }
        Ok(ExperienceReport::ok(engram_id, separation, false))
    }

    /// Background heartbeat — auto-sleep when due (brainstem / autonomic).
    pub fn tick(&mut self) -> Result<TickReport> {
        self.wal_append(WalEntry::Tick { n: 1 })?;
        self.tick_internal(true)
    }

    pub(crate) fn tick_internal(&mut self, checkpoint: bool) -> Result<TickReport> {
        self.autonomic.on_tick();
        self.development.on_tick();
        // Neuromodulator decay: ACh/DA/NE/5HT drift back to baseline each tick (Doya mapping).
        self.neuromodulators.tick_decay();
        // PFC working memory fades: goals & task context decay without rehearsal.
        self.prefrontal.tick_decay(self.autonomic.total_ticks);
        self.autonomic.roll_sleep_window(self.autonomic.total_ticks);
        let _ = self.agent_on_tick()?;

        let pressure = self.autonomic.synapse_pressure(
            self.graph.synapse_count(),
            self.development.stage.max_synapses(),
        );

        let mut slept = false;
        let mut sleep_report = None;
        let mut stage_advanced = false;

        if self.autonomic.should_sleep(
            self.graph.synapse_count(),
            self.development.stage.max_synapses(),
        ) {
            let before = self.development.stage.as_str().to_string();
            let report = self.sleep_internal(checkpoint, SleepTrigger::Autonomic)?;
            stage_advanced = report.advanced;
            sleep_report = Some(report);
            slept = true;
            let _ = before;
        } else if checkpoint && !crate::somnus::somnus_enabled() {
            // Legacy only: ticks must not mint systems seals under Somnus
            // (brainstem ≠ neocortical reprint).
            self.maybe_checkpoint()?;
        }

        // Somnus autonomic durability: systems seal on its own (no user toggle,
        // no semantic sleep_cycle). Skipped when semantic sleep already sealed.
        let _ = self.maybe_somnus_autonomic_seal(checkpoint, slept)?;

        Ok(TickReport {
            tick: self.autonomic.total_ticks,
            stage: self.development.stage.as_str().to_string(),
            ticks_since_sleep: self.autonomic.ticks_since_sleep,
            synapse_pressure: pressure,
            slept,
            sleep_report,
            stage_advanced,
        })
    }

    /// Run N background ticks (for agents / demos).
    pub fn tick_n(&mut self, n: u64) -> Result<Vec<TickReport>> {
        self.wal_append(WalEntry::Tick { n })?;
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let checkpoint = i + 1 == n;
            out.push(self.tick_internal(checkpoint)?);
        }
        Ok(out)
    }

    pub fn activate(&self, cue: &str) -> ActivationResult {
        self.activate_with_semantic(cue, None)
    }

    /// Opt-in schema lane: episodic `activate` unchanged + matching active schemas.
    /// Does not alter default `activate()` ranking.
    pub fn activate_with_schemas(&self, cue: &str) -> crate::schema::SchemaAwareActivation {
        let episodic = self.activate(cue);
        let cue_l = cue.to_lowercase();
        let cue_toks: Vec<&str> = cue_l.split_whitespace().collect();
        let schemas: Vec<_> = self
            .cortex
            .schemas
            .active()
            .filter(|s| {
                let st = s.statement.to_lowercase();
                cue_toks.iter().any(|t| st.contains(t)) || st.split_whitespace().any(|t| cue_l.contains(t))
            })
            .cloned()
            .collect();
        crate::schema::SchemaAwareActivation { episodic, schemas }
    }

    pub fn activate_with_semantic(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
    ) -> ActivationResult {
        self.activate_scoped(
            cue,
            cue_vector,
            None,
            crate::api_slim::DEFAULT_API_RECALL_LIMIT,
        )
    }

    pub fn activate_scoped(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        agent_id: Option<&str>,
        top_k: usize,
    ) -> ActivationResult {
        let top_k = top_k.clamp(1, crate::index::MAX_CANDIDATE_CAP);
        if let Some(cached) = self
            .activation_cache
            .lock()
            .unwrap()
            .get(cue, agent_id, top_k)
        {
            return cached;
        }

        let candidate_cap = top_k.max(default_candidate_cap());
        // Hybrid BM25+dense sidecar when present (LongMemEval / connect_index).
        // When absent, scan all hippocampal engrams — do not substitute Fabric Photon
        // prefilter (that path is for CHORUS rerank, not hybrid activate).
        let candidate_set: Option<HashSet<uuid::Uuid>> = self
            .recall_index
            .as_ref()
            .and_then(|idx| {
                idx.hybrid_candidates(cue, cue_vector, &self.semantic, candidate_cap)
                    .ok()
            })
            .and_then(|ids| {
                let capped = cap_candidates(ids, candidate_cap);
                if capped.is_empty() {
                    None
                } else {
                    Some(capped)
                }
            });

        let mut result = activate_from_hybrid(
            cue,
            cue_vector,
            &self.graph,
            &self.hippocampus,
            &self.semantic,
            self.life.life_id,
            crate::activation::activation_max_hops(),
            self.development.stage.myelination(),
            top_k,
            candidate_set.as_ref(),
        );
        let cortex_boost = self.cortex.fact_boost(cue) + self.cortex.semantic_boost(cue_vector);
        let field_boost = cue_vector
            .map(|v| self.semantic.centroid_boost(v))
            .unwrap_or(0.0);
        let cortex_w = std::env::var("FLUCTLIGHT_CORTEX_WEIGHT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.1);
        for recall in &mut result.recalls {
            recall.activation += (cortex_boost + field_boost) * cortex_w;
            if recall.verified {
                recall.activation += 0.15;
            }
        }
        // CA3 Hopfield attractor: pattern-completion recall pathway (Marr 1971; Hopfield 1982).
        // Active when in retrieval mode (ACh < 0.6): low ACh opens CA3 recurrent collaterals,
        // allowing a partial query cue to "relax" to the nearest stored engram attractor state.
        // This rescues recalls that BM25+dense missed because the surface tokens didn't overlap.
        if !self.neuromodulators.is_encoding() {
            let gain = self.neuromodulators.ca3_recurrent_gain();
            let rich = crate::tokenize::tokenize_rich(cue, "", None);
            let cue_neurons: Vec<crate::id::NeuronId> = rich
                .iter()
                .map(|t| crate::id::NeuronId::from_seeds(&["ec", &t.surface]))
                .collect();
            if let Some(completed) = self.hippocampus.ca3_attractor_complete(
                &cue_neurons,
                self.life.life_id,
                gain,
                0.07, // 7% Jaccard: loose enough for partial cues, tight enough to avoid noise
            ) {
                let boost = gain * 0.35;
                if let Some(r) = result
                    .recalls
                    .iter_mut()
                    .find(|r| r.engram_id == completed.id)
                {
                    // Amplify existing result — CA3 confirms the BM25+dense hit
                    r.activation += boost;
                    r.completion_strength = (r.completion_strength + gain).min(1.0);
                } else {
                    // CA3 rescued a recall that hybrid search missed
                    result.recalls.push(crate::types::RecallResult {
                        engram_id: completed.id,
                        activation: boost,
                        episode: completed.episode.clone(),
                        completion_strength: gain,
                        separation_index: completed.separation_index,
                        verified: completed
                            .episode
                            .provenance
                            .as_ref()
                            .is_some_and(|p| p.verified),
                        trust_note: None,
                    });
                }
            }
        }

        // Recall Fabric (opt-in): blend composed scores into hybrid activation. Off by default.
        self.fabric_on_activate(cue, cue_vector, &mut result.recalls);
        self.merge_chorus_recalls(cue, cue_vector, &mut result.recalls, top_k);

        // ── Neuromodulator-gated recall scoring (Schultz 1997; Hasselmo 2006) ─────────────
        // DA (dopamine) amplifies ALL recall scores proportionally — high DA means the
        // current context is reward-relevant; the whole memory landscape is amplified.
        // NE (norepinephrine) sharpens SNR: boosts strong signals, attenuates weak ones,
        // modelling the "attentional spotlight" during arousal (Sara & Bouret 2012).
        {
            let da = self.neuromodulators.dopamine;
            let ne = self.neuromodulators.norepinephrine;
            if da > 0.5 || ne > 0.3 {
                let da_boost = 1.0 + (da - 0.5_f32).max(0.0) * 0.20;
                for recall in &mut result.recalls {
                    // DA: uniform amplification proportional to above-baseline dopamine
                    recall.activation *= da_boost;
                    // NE: sharpens by amplifying confident hits, softly suppressing noise
                    if recall.activation > 1.0 {
                        recall.activation *= 1.0 + ne * 0.10;
                    } else {
                        recall.activation *= (1.0 - ne * 0.08).max(0.5);
                    }
                }
            }
        }

        // ── LIF temporal coding: spike timing as recall confidence ────────────────────────
        // Each recall score is treated as synaptic current driving a LIF neuron.
        // Engrams that fire their neuron quickly (strong current = high activation) get a
        // small bonus — "earlier spike = stronger representation" (rate → temporal recoding).
        // This adds a non-linear sharpening effect between strong and marginal memories.
        for recall in &mut result.recalls {
            recall.activation += lif_score_boost(recall.activation);
        }

        // ── Prefrontal cortex: top-down goal bias + inhibitory control ────────────────────
        // dlPFC: goals in working memory boost recall of goal-relevant engrams (Miller & Cohen 2001).
        // vlPFC: inhibitory patterns suppress conflicting/irrelevant content (Aron 2007).
        // ACC: rule set routes recall through explicit if/then executive filters.
        if self.prefrontal.unlocked {
            for recall in &mut result.recalls {
                let content = &recall.episode.content;
                // Goal-biased recall: up to +0.5 boost for goal-matching engrams
                recall.activation += self.prefrontal.goal_bias_score(content, cue);
                // Inhibitory control: suppresses engrams matching inhibited patterns
                let suppression = self.prefrontal.inhibit_score(content);
                recall.activation = (recall.activation + suppression).max(0.0);
            }
            // PFC rules: scan once, apply to whole recall set
            let rules = self.prefrontal.matching_rules(cue);
            if !rules.is_empty() {
                for rule in rules {
                    match &rule.action {
                        RuleAction::BoostVerified => {
                            for recall in &mut result.recalls {
                                if recall.verified {
                                    recall.activation *= 1.20;
                                }
                            }
                        }
                        RuleAction::RequireSource(src) => {
                            let src = src.clone();
                            result.recalls.retain(|r| {
                                // Check provenance.source_uri then rag.source_uri
                                let prov_match = r
                                    .episode
                                    .provenance
                                    .as_ref()
                                    .and_then(|p| p.source_uri.as_deref())
                                    == Some(src.as_str());
                                let rag_match = r
                                    .episode
                                    .rag
                                    .as_ref()
                                    .and_then(|rag| rag.source_uri.as_deref())
                                    == Some(src.as_str());
                                prov_match || rag_match
                            });
                        }
                        RuleAction::InjectContext(ctx) => {
                            // Prepend context hint to every recall episode so downstream
                            // consumers see the top-down priming signal.
                            let prefix = format!("[ctx: {}] ", ctx);
                            for recall in &mut result.recalls {
                                if !recall.episode.context.starts_with('[') {
                                    recall.episode.context =
                                        format!("{}{}", prefix, recall.episode.context);
                                }
                            }
                        }
                    }
                }
            }
        }

        result
            .recalls
            .sort_by(|a, b| b.activation.partial_cmp(&a.activation).unwrap());
        if let Some(aid) = agent_id {
            result
                .recalls
                .retain(|r| r.episode.agent_id.as_deref() == Some(aid));
        }
        prefer_ledger_truth_on_balance_cue(cue, &self.hippocampus, &mut result.recalls);

        // Exact query override (generalised from ledger-truth).
        // When the user asks for precise/deterministic data (IDs, amounts, phone numbers,
        // status queries), probabilistic activation is the wrong model — verified engrams
        // must WIN over all associative results, regardless of activation score.
        // detect_exact_query() identifies these patterns; exact_verified_recall() scans
        // only provenance-backed engrams and injects them at activation 2.0 (guaranteed top).
        if crate::recall_router::detect_exact_query(cue) {
            exact_verified_recall(
                cue,
                &self.hippocampus,
                self.life.life_id,
                &mut result.recalls,
            );
        }

        annotate_recall_trust(&mut result.recalls);
        self.activation_cache
            .lock()
            .unwrap()
            .put(cue, agent_id, top_k, result.clone());
        result
    }

    /// Batch activate — one brain lock, many cues (production agent hot path).
    #[allow(clippy::type_complexity)]
    pub fn activate_batch(
        &self,
        items: &[(String, Option<Vec<f32>>, Option<String>)],
        top_k: usize,
    ) -> Vec<ActivationResult> {
        items
            .iter()
            .map(|(cue, vec, agent)| {
                self.activate_scoped(cue, vec.as_deref(), agent.as_deref(), top_k)
            })
            .collect()
    }

    /// Mark an engram as verified ground truth (ledger, tool, file).
    pub fn verify_fact(
        &mut self,
        engram_id: Uuid,
        kind: crate::types::ProvenanceKind,
        source_uri: Option<String>,
        confidence: f32,
    ) -> Result<()> {
        self.reject_distributed_mutation("FluctlightBrain::verify_fact")?;
        let engram = self
            .hippocampus
            .engrams
            .iter_mut()
            .find(|e| e.id == engram_id)
            .ok_or_else(|| Error::Store(format!("engram not found: {engram_id}")))?;
        engram.episode.provenance = Some(crate::types::Provenance {
            kind,
            source_uri,
            confidence: confidence.clamp(0.0, 1.0),
            verified: true,
        });
        self.activation_cache.lock().unwrap().invalidate();
        self.maybe_checkpoint()?;
        Ok(())
    }

    /// Saccadic file intake — foveated packets encoded as separate experiences.
    pub fn fovea_ingest(
        &mut self,
        path: &Path,
        cfg: &crate::fovea::FoveaConfig,
    ) -> Result<Vec<ExperienceReport>> {
        let packets = crate::fovea::scan_file(path, cfg)?;
        let source = format!("file://{}", path.display());
        let mut reports = Vec::with_capacity(packets.len());
        for pkt in packets {
            let content = format!(
                "[fix{}] {} | …{}… {} …",
                pkt.fixation, pkt.foveal, pkt.peripheral_before, pkt.peripheral_after
            );
            let chunk_id = format!("fovea-{}", pkt.fixation);
            let report = self.experience(Episode {
                content,
                context: format!(
                    "fovea:{}",
                    path.file_name().and_then(|s| s.to_str()).unwrap_or("file")
                ),
                outcome: None,
                salience_hint: pkt.salience_hint,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: Some(crate::types::RagRef {
                    source_uri: Some(source.clone()),
                    doc_id: Some(path.display().to_string()),
                    chunk_id: Some(chunk_id),
                }),
                provenance: Some(crate::types::Provenance {
                    kind: crate::types::ProvenanceKind::FileObservation,
                    source_uri: Some(source.clone()),
                    confidence: 0.75,
                    verified: false,
                }),
            })?;
            reports.push(report);
        }
        Ok(reports)
    }

    /// Prospective spreading activation — hippocampal preplay for planning.
    pub fn preplay(&self, goal: &str, steps: u32) -> crate::preplay::PreplayResult {
        crate::preplay::preplay_forward(
            goal,
            steps,
            &self.graph,
            &self.hippocampus,
            self.life.life_id,
            self.development.stage.myelination(),
        )
    }

    /// Adult neurogenesis pulse — seed immature probes, prune weak separators.
    pub fn neurogenesis_pulse(&mut self) -> Result<crate::neurogenesis::NeurogenesisReport> {
        self.reject_distributed_mutation("FluctlightBrain::neurogenesis_pulse")?;
        let tick = self.development.metrics.ticks;
        let stage = self.development.stage as u8;
        let report =
            crate::neurogenesis::pulse(&mut self.hippocampus, self.life.life_id, tick, stage);
        self.maybe_checkpoint()?;
        Ok(report)
    }

    /// Verified ground-truth facts for prompt injection (reality monitoring).
    pub fn verified_context(&self, limit: usize) -> crate::reality::VerifiedContext {
        crate::reality::verified_context(self, limit)
    }

    /// CLS stage consolidation metrics for agent introspection.
    pub fn stage_report(&self) -> crate::stage_schedule::StageConsolidationReport {
        crate::stage_schedule::report(self)
    }

    pub fn complete(&self, cue: &str) -> Option<Engram> {
        complete(cue, &self.hippocampus, self.life.life_id)
    }

    /// Neocortical fact readout for cue (post-sleep consolidation).
    pub fn cortex_facts_for_cue(&self, cue: &str, limit: usize) -> Vec<(String, f32)> {
        self.cortex.top_facts_for_cue(cue, limit)
    }

    pub fn sleep(&mut self) -> Result<SleepReport> {
        self.wal_append(WalEntry::Sleep)?;
        self.sleep_internal(true, SleepTrigger::Manual)
    }

    pub(crate) fn sleep_internal(
        &mut self,
        checkpoint: bool,
        trigger: SleepTrigger,
    ) -> Result<SleepReport> {
        let stage_before = self.development.stage.as_str().to_string();
        let mut report = sleep_cycle(
            &mut self.hippocampus,
            &mut self.cortex,
            &mut self.amygdala,
            &mut self.graph,
            &mut self.neuromodulators,
            &mut self.semantic,
            &self.life,
            &self.development,
            16,
        );
        // CaptureGate: only eligibility-tagged engrams crystallize schemas (CLS capture).
        let tagged = self.cortex.eligibility.tags.clone();
        let cap = crate::capture_gate::capture_schemas(
            &mut self.cortex.schemas,
            &self.hippocampus.engrams,
            &tagged,
            &["theme", "dark", "light"],
        )?;
        if !cap.rolled_back {
            for id in &tagged {
                self.cortex.eligibility.tags.remove(id);
            }
        }
        let _ = cap;
        self.development.on_sleep(report.pruned_synapses);
        self.prefrontal.unlocked = self.development.pfc_unlocked();
        report.stage_after = self.development.stage.as_str().to_string();
        report.advanced = stage_before != report.stage_after;

        match trigger {
            SleepTrigger::Autonomic | SleepTrigger::Pressure => self.autonomic.on_sleep(),
            SleepTrigger::Manual => {}
        }

        if self
            .development
            .metrics
            .sleep_cycles
            .is_multiple_of(COMPACT_EVERY_N_SLEEPS)
        {
            let _ = self.compact_internal(false);
        }

        self.fabric_on_sleep();

        if checkpoint {
            // Sleep = systems consolidation: one seal + decay obsolete generations.
            if crate::somnus::somnus_enabled() {
                self.systems_seal()?;
            } else {
                self.maybe_checkpoint()?;
            }
        }
        Ok(report)
    }

    pub fn compact(&mut self) -> Result<CompactReport> {
        self.wal_append(WalEntry::Compact)?;
        self.compact_internal(true)
    }

    pub(crate) fn compact_internal(&mut self, checkpoint: bool) -> Result<CompactReport> {
        let report = compact_brain(self);
        if checkpoint {
            self.maybe_checkpoint()?;
        }
        Ok(report)
    }

    pub fn reward(&mut self, magnitude: f32) -> Result<()> {
        self.wal_append(WalEntry::Reward { magnitude })?;
        self.reward_internal(magnitude);
        self.checkpoint()
    }

    pub(crate) fn reward_internal(&mut self, magnitude: f32) {
        self.neuromodulators.on_reward(magnitude);
    }

    /// Rewrite an existing engram in place (Tier A reconsolidation).
    pub fn reconsolidate(
        &mut self,
        engram_id: Uuid,
        content: Option<String>,
        outcome: Option<String>,
        salience_boost: f32,
        semantic_vector: Option<Vec<f32>>,
        supersede_similar: bool,
    ) -> Result<ReconsolidateReport> {
        self.reject_distributed_mutation("FluctlightBrain::reconsolidate")?;
        let tick = self.development.metrics.ticks;
        let idx = self
            .hippocampus
            .engrams
            .iter()
            .position(|e| e.id == engram_id)
            .ok_or_else(|| Error::Store(format!("engram not found: {engram_id}")))?;
        {
            let engram = &mut self.hippocampus.engrams[idx];
            if let Some(c) = content {
                engram.episode.content = c.chars().take(500).collect();
            }
            if let Some(o) = outcome {
                engram.episode.outcome = Some(o.chars().take(200).collect());
            }
            if let Some(v) = semantic_vector {
                engram.episode.semantic_vector = Some(v);
            }
            engram.salience = (engram.salience + salience_boost).clamp(0.0, 1.0);
            engram.replay_count = engram.replay_count.saturating_add(1);
        }
        if let Some(v) = self.hippocampus.engrams[idx]
            .episode
            .semantic_vector
            .clone()
        {
            let eid = self.hippocampus.engrams[idx].id;
            let lid = self.hippocampus.engrams[idx].life_id;
            self.semantic.register_engram(eid, lid, v);
        }
        let report_content = self.hippocampus.engrams[idx].episode.content.clone();
        let revision = self.hippocampus.engrams[idx].replay_count;
        let labile_until_tick = tick.saturating_add(64);

        let mut superseded_others = 0usize;
        if supersede_similar {
            let needle = report_content.to_lowercase();
            for other in &mut self.hippocampus.engrams {
                if other.id == engram_id {
                    continue;
                }
                if other.life_id != self.life.life_id {
                    continue;
                }
                let similar = other.episode.content.to_lowercase() == needle
                    || (needle.len() > 12
                        && other
                            .episode
                            .content
                            .to_lowercase()
                            .contains(needle.get(..32).unwrap_or(&needle)));
                if similar && other.salience > 0.05 {
                    other.salience *= 0.45;
                    superseded_others += 1;
                }
            }
        }

        self.hippocampus.rebuild_rag_index();
        self.activation_cache.lock().unwrap().invalidate();
        self.maybe_checkpoint()?;
        Ok(ReconsolidateReport {
            updated: true,
            engram_id,
            content: report_content,
            revision,
            labile_until_tick,
            superseded_others,
        })
    }

    /// Executive goal bias (HTTP API — stores goal in working memory; PFC biases recall
    /// toward matching engrams once unlocked).
    pub fn api_set_goal(&mut self, goal: String) -> Result<()> {
        self.reject_distributed_mutation("FluctlightBrain::api_set_goal")?;
        let goal = goal.chars().take(200).collect::<String>();
        self.prefrontal.add_goal(goal, self.autonomic.total_ticks);
        self.maybe_checkpoint()?;
        Ok(())
    }

    /// Inhibit recall phrases matching pattern (HTTP API — PFC suppresses matching engrams).
    pub fn api_inhibit(&mut self, action: String) -> Result<()> {
        self.reject_distributed_mutation("FluctlightBrain::api_inhibit")?;
        let action = action.chars().take(200).collect::<String>();
        self.prefrontal.add_inhibit(action);
        self.maybe_checkpoint()?;
        Ok(())
    }

    pub fn mark_core(&mut self, engram_id: Uuid, key: String) -> Result<()> {
        self.wal_append(WalEntry::MarkCore {
            engram_id,
            key: key.clone(),
        })?;
        self.mark_core_internal(engram_id, key);
        self.checkpoint()
    }

    pub(crate) fn mark_core_internal(&mut self, engram_id: Uuid, key: String) {
        self.hippocampus.mark_core(engram_id);
        if let Some(e) = self.hippocampus.engrams.iter().find(|e| e.id == engram_id) {
            self.core_memories.persist(
                key,
                e.episode.content.clone(),
                self.life.life_id,
                Some(engram_id),
            );
        }
    }

    pub fn death(&mut self, cause: &str) -> Result<Uuid> {
        self.wal_append(WalEntry::Death {
            cause: cause.to_string(),
        })?;
        self.death_internal(cause, true)
    }

    pub(crate) fn death_internal(&mut self, cause: &str, checkpoint: bool) -> Result<Uuid> {
        self.core_memories.persist(
            format!("death:{}", self.life.death_count + 1),
            cause.to_string(),
            self.life.life_id,
            None,
        );
        self.life.death();
        self.hippocampus.clear_ephemeral(self.life.life_id);
        self.development.metrics.deaths_survived += 1;
        let new_life = self.life.respawn(self.development.metrics.ticks);
        self.development.on_experience(0.9);
        if checkpoint {
            self.checkpoint()?;
        }
        Ok(new_life)
    }

    pub fn export_viz(&self) -> VizExport {
        VizExport {
            stage: self.development.stage.as_str().to_string(),
            tick: self.autonomic.total_ticks,
            synapses: self.graph.synapse_count(),
            engrams: self.hippocampus.engrams.len(),
            synapse_pressure: self.autonomic.synapse_pressure(
                self.graph.synapse_count(),
                self.development.stage.max_synapses(),
            ),
            ticks_since_sleep: self.autonomic.ticks_since_sleep,
            recent_separations: self.recent_separations.clone(),
            development: DevelopmentViz {
                experiences: self.development.metrics.experience_count,
                sleep_cycles: self.development.metrics.sleep_cycles,
                pruned_synapses: self.development.metrics.pruned_synapses,
            },
        }
    }

    /// Recent temporal-axis events (Recall Fabric). Empty unless `FLUCTLIGHT_FABRIC=1` during ingest.
    pub fn timeline(&self, n: usize) -> Vec<crate::chronos::Event> {
        self.chronos.recent(n)
    }

    /// Count of crystallized concepts on the consolidated cortical map (Recall Fabric).
    pub fn crystal_count(&self) -> usize {
        self.crystallizer.len()
    }

    // ---- Chronos (temporal + causal) user API ----

    pub fn chronos_events_in_range(
        &self,
        from_tick: u64,
        to_tick: u64,
    ) -> Vec<crate::chronos::Event> {
        self.chronos.in_range(from_tick, to_tick)
    }

    pub fn chronos_preceding(&self, event_id: &str, n: usize) -> Vec<crate::chronos::Event> {
        self.chronos.preceding(event_id, n)
    }

    pub fn chronos_link_cause(&mut self, cause_id: &str, effect_id: &str) {
        self.chronos.link_cause(cause_id, effect_id);
    }

    pub fn chronos_causal_ancestors(&self, effect_id: &str) -> Vec<String> {
        self.chronos.causal_ancestors(effect_id)
    }

    pub fn chronos_before(&self, a: &str, b: &str) -> Option<bool> {
        self.chronos.before(a, b)
    }

    pub fn chronos_bucket(&self, event_id: &str, scale: u64) -> Option<u64> {
        self.chronos.bucket(event_id, scale)
    }

    pub fn chronos_len(&self) -> usize {
        self.chronos.len()
    }

    // ---- Consensus (multi-agent shared memory) user API ----

    pub fn consensus_assert_claim(&mut self, key: &str, claim: crate::consensus::Claim) {
        self.consensus.assert(key, claim);
    }

    pub fn consensus_resolve(
        &self,
        key: &str,
        viewer_agent_id: Option<&str>,
    ) -> Option<crate::consensus::Consensus> {
        self.consensus.resolve(key, viewer_agent_id)
    }

    pub fn consensus_claims(
        &self,
        key: &str,
        viewer_agent_id: Option<&str>,
    ) -> Vec<crate::consensus::Claim> {
        self.consensus
            .readable_claims(key, viewer_agent_id)
            .into_iter()
            .cloned()
            .collect()
    }

    pub fn consensus_is_contested(&self, key: &str, viewer_agent_id: Option<&str>) -> bool {
        self.consensus.is_contested(key, viewer_agent_id)
    }

    pub fn export_graph(&self) -> GraphExport {
        export_graph(&self.hippocampus, &self.graph, &self.core_memories)
    }

    pub fn export_graph_lite(&self) -> GraphExport {
        export_graph_lite(&self.hippocampus, &self.graph, &self.core_memories)
    }

    pub fn consolidate_episodes(&self, min_salience: f32, limit: usize) -> Vec<ConsolidatedMemory> {
        let mut items: Vec<_> = self
            .hippocampus
            .engrams
            .iter()
            .filter(|e| e.salience >= min_salience)
            .map(|e| ConsolidatedMemory {
                engram_id: e.id,
                content: e.episode.content.clone(),
                context: e.episode.context.clone(),
                salience: e.salience,
            })
            .collect();
        items.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(limit);
        items
    }

    pub fn export_raw(&self) -> RawExport {
        export_raw(self)
    }

    fn push_separation(&mut self, sep: SeparationResult) {
        self.recent_separations.push(sep);
        if self.recent_separations.len() > MAX_RECENT_SEPARATIONS {
            self.recent_separations.remove(0);
        }
    }

    pub fn brain_store_path(&self) -> Option<&Path> {
        self.store_path.as_deref()
    }

    pub fn has_sidecar_index(&self) -> bool {
        self.recall_index
            .as_ref()
            .map(|i| i.uses_sidecar())
            .unwrap_or(false)
    }

    pub(crate) fn has_recall_index(&self) -> bool {
        self.recall_index.is_some()
    }

    pub fn invalidate_activation_cache(&mut self) {
        self.activation_cache.lock().unwrap().invalidate();
    }

    pub fn attach_store_path(&mut self, path: PathBuf) {
        self.store_path = Some(path.clone());
        self.recall_index = RecallIndex::open_sidecar(&path)
            .or_else(|_| RecallIndex::rebuild(self))
            .ok();
    }

    pub(crate) fn attach_store_lock(&mut self, lock: BrainStoreLock) {
        self.store_lock = Some(lock);
    }

    fn index_engram(&mut self, engram_id: Uuid, content: &str, vector: Option<&[f32]>) {
        if self.recall_index.is_none() {
            self.recall_index = RecallIndex::rebuild(self).ok();
        }
        if let Some(ref idx) = self.recall_index {
            let _ = idx.upsert_engram(engram_id, content, vector);
        }
    }

    pub(crate) fn remove_from_recall_index(&mut self, engram_id: Uuid) -> Result<()> {
        if let Some(ref idx) = self.recall_index {
            idx.remove_engram(engram_id)?;
        }
        Ok(())
    }

    pub fn status(&self) -> BrainStatus {
        let generation_dirs = self
            .store_path
            .as_ref()
            .and_then(|p| crate::homeostasis::count_generation_dirs(p));
        let keep = crate::somnus::somnus_keep();
        let generation_count_ok = generation_dirs.map(|n| n <= keep);
        let token_budget = crate::homeostasis::agent_prompt_token_budget();
        let last = self.homeostasis.last_prompt_tokens_est;
        BrainStatus {
            life_id: self.life.life_id,
            stage: self.development.stage.as_str().to_string(),
            experiences: self.development.metrics.experience_count,
            sleep_cycles: self.development.metrics.sleep_cycles,
            auto_sleeps: self.autonomic.auto_sleeps,
            sleeps_in_window: self.autonomic.sleeps_in_window,
            synapses: self.graph.synapse_count(),
            engrams: self.hippocampus.engrams.len(),
            core_memories: self.core_memories.memories.len(),
            semantic_engrams: self.semantic.engram_vectors.len(),
            semantic_centroids: self.semantic.centroids.len(),
            pfc_unlocked: self.prefrontal.unlocked,
            alive: self.life.alive,
            autonomic_ticks: self.autonomic.total_ticks,
            ticks_since_sleep: self.autonomic.ticks_since_sleep,
            synapse_pressure: self.autonomic.synapse_pressure(
                self.graph.synapse_count(),
                self.development.stage.max_synapses(),
            ),
            wal_seq: self.wal_seq,
            homeostasis: crate::homeostasis::HomeostasisReport {
                somnus_enabled: crate::somnus::somnus_enabled(),
                somnus_keep: keep,
                somnus_seal_every_ticks: crate::somnus::somnus_seal_every_ticks(),
                systems_seals_total: self.homeostasis.systems_seals_total,
                ticks_since_systems_seal: self.ticks_since_systems_seal,
                generation_dirs,
                generation_count_ok,
                agent_prompt_calls: self.homeostasis.agent_prompt_calls,
                last_prompt_tokens_est: last,
                median_prompt_tokens_est: self.homeostasis.median_prompt_tokens_est(),
                agent_prompt_token_budget: token_budget,
                agent_prompt_max_engrams: crate::homeostasis::agent_prompt_max_engrams(),
                tokens_within_budget: self.homeostasis.agent_prompt_calls == 0
                    || (last as usize) <= token_budget,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_snapshot(
        wal_seq: u64,
        life: crate::life::LifeState,
        development: crate::development::DevelopmentState,
        neuromodulators: crate::neuromodulator::Neuromodulators,
        graph: crate::graph::BrainGraph,
        hippocampus: crate::hippocampus::Hippocampus,
        cortex: crate::cortex::Cortex,
        amygdala: crate::amygdala::Amygdala,
        prefrontal: crate::prefrontal::Prefrontal,
        core_memories: crate::life::CoreMemoryStore,
        autonomic: AutonomicState,
        recent_separations: Vec<SeparationResult>,
        semantic: SemanticField,
    ) -> Self {
        let mut graph = graph;
        graph.rebuild_index();
        let mut hippocampus = hippocampus;
        if hippocampus.rag_index.is_empty() && !hippocampus.engrams.is_empty() {
            hippocampus.rebuild_rag_index();
        }
        Self {
            wal_seq,
            life,
            development,
            neuromodulators,
            graph,
            hippocampus,
            cortex,
            amygdala,
            prefrontal,
            core_memories,
            autonomic,
            agent: AgentState::default(),
            governance: crate::governance::GovernanceState::default(),
            semantic,
            recent_separations,
            checkpoint_policy: CheckpointPolicy::default(),
            ticks_since_systems_seal: 0,
            wal_records_since_seal: 0,
            homeostasis: crate::homeostasis::HomeostasisState::default(),
            store_path: None,
            wal_identity: None,
            store_lock: None,
            recall_index: None,
            activation_cache: Mutex::new(ActivationCache::new()),
            chronos: crate::chronos::Chronos::default(),
            crystallizer: crate::crystallize::Crystallizer::default(),
            fabric: crate::recall_fabric::RecallFabric::new(crate::fabric_runtime::fabric_config()),
            fabric_traces: Mutex::new(HashMap::new()),
            consensus: crate::consensus::SharedMemory::default(),
            muon: crate::muon_runtime::new_muon_lane(),
            tau: crate::tau_runtime::new_tau_lane(),
            chorus: crate::chorus_runtime::new_chorus_field(),
        }
    }
}

impl std::fmt::Debug for FluctlightBrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FluctlightBrain")
            .field("wal_seq", &self.wal_seq)
            .field("engrams", &self.hippocampus.engrams.len())
            .field("synapses", &self.graph.synapse_count())
            .finish_non_exhaustive()
    }
}

impl Clone for FluctlightBrain {
    fn clone(&self) -> Self {
        let cache = self.activation_cache.lock().unwrap().clone();
        Self {
            wal_seq: self.wal_seq,
            life: self.life.clone(),
            development: self.development.clone(),
            neuromodulators: self.neuromodulators.clone(),
            graph: self.graph.clone(),
            hippocampus: self.hippocampus.clone(),
            cortex: self.cortex.clone(),
            amygdala: self.amygdala.clone(),
            prefrontal: self.prefrontal.clone(),
            core_memories: self.core_memories.clone(),
            autonomic: self.autonomic.clone(),
            agent: self.agent.clone(),
            governance: self.governance.clone(),
            semantic: self.semantic.clone(),
            recent_separations: self.recent_separations.clone(),
            checkpoint_policy: self.checkpoint_policy.clone(),
            ticks_since_systems_seal: self.ticks_since_systems_seal,
            wal_records_since_seal: self.wal_records_since_seal,
            homeostasis: self.homeostasis.clone(),
            store_path: None,
            wal_identity: None,
            store_lock: None,
            recall_index: None,
            activation_cache: Mutex::new(cache),
            chronos: self.chronos.clone(),
            crystallizer: self.crystallizer.clone(),
            fabric: self.fabric.clone(),
            fabric_traces: Mutex::new(self.fabric_traces.lock().unwrap().clone()),
            consensus: self.consensus.clone(),
            muon: self.muon.clone(),
            tau: self.tau.clone(),
            chorus: self.chorus.clone(),
        }
    }
}

/// LIF temporal coding: convert an engram's activation score into a small score bonus.
///
/// Treats `activation` as a synaptic current driving a default LIF neuron.
/// If the neuron fires within 50ms, the bonus scales inversely with fire time:
/// fast-firing (strong current) → up to +0.30 bonus; slow/no-fire → 0.
///
/// Biology: temporal coding hypothesis (Thorpe 1996) — neurons that fire earlier
/// carry stronger/more reliable information. Used here as a non-linear sharpening
/// of the recall score: strong memories win over marginal ones by a slightly larger margin.
fn lif_score_boost(activation: f32) -> f32 {
    // Scale activation to nA current: I_threshold = 0.15 nA, so threshold activation ≈ 3.0
    let i_syn = activation * 0.05_f32;
    if i_syn < 0.01 {
        return 0.0; // far below threshold — no boost
    }
    let mut n = crate::neuron::LIFNeuron::default();
    for t in 1..=50u64 {
        if n.integrate(i_syn, 1.0, t) {
            // Fired at tick t: boost inversely proportional to fire time
            // (t=1 → 0.30, t=50 → 0.0)
            return (51 - t) as f32 / 50.0 * 0.30;
        }
    }
    0.0
}

fn prefer_ledger_truth_on_balance_cue(
    cue: &str,
    hippocampus: &crate::hippocampus::Hippocampus,
    recalls: &mut Vec<crate::types::RecallResult>,
) {
    let low = cue.to_lowercase();
    if !any_contains(
        &low,
        &["balance", "wallet", "ledger", "$", "money", "credit"],
    ) {
        return;
    }

    if let Some(id) = hippocampus.find_rag_chunk("ledger", "wallet-balance") {
        boost_recall_engram(hippocampus, recalls, id);
        return;
    }

    let ledger_id = hippocampus
        .engrams
        .iter()
        .filter(|e| {
            let prov = e.episode.provenance.as_ref();
            let verified = prov.map(|p| p.verified).unwrap_or(false);
            let ledger = e.episode.context.starts_with("ledger:")
                || prov
                    .map(|p| p.kind == ProvenanceKind::LedgerVerified)
                    .unwrap_or(false);
            if !(verified && ledger) {
                return false;
            }
            let c = e.episode.content.to_lowercase();
            c.contains("wallet") || c.contains("balance") || c.contains("ledger")
        })
        .max_by(|a, b| {
            a.salience
                .partial_cmp(&b.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|e| e.id);

    if let Some(id) = ledger_id {
        boost_recall_engram(hippocampus, recalls, id);
    }
}

fn boost_recall_engram(
    hippocampus: &crate::hippocampus::Hippocampus,
    recalls: &mut Vec<crate::types::RecallResult>,
    id: uuid::Uuid,
) {
    if let Some(recall) = recalls.iter_mut().find(|r| r.engram_id == id) {
        recall.activation = recall.activation.max(5.0) + 5.0;
        recall.verified = true;
        recall.trust_note = None;
    } else if let Some(engram) = hippocampus.engrams.iter().find(|e| e.id == id) {
        recalls.insert(
            0,
            crate::types::RecallResult {
                engram_id: id,
                activation: 10.0,
                episode: engram.episode.clone(),
                completion_strength: 1.0,
                separation_index: engram.separation_index,
                verified: true,
                trust_note: None,
            },
        );
    }
    recalls.sort_by(|a, b| b.activation.partial_cmp(&a.activation).unwrap());
    recalls.truncate(8);
}

fn any_contains(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Exact / deterministic recall — scans verified and tool-grounded engrams for content
/// that overlaps with the cue, then injects them at the TOP of the recall list with
/// activation 10.0, guaranteeing they win over any probabilistic associative result.
///
/// This is the correct model for "what is invoice #4821?" — the answer is a fact,
/// not a probability distribution. The brain must produce the verified engram or
/// nothing at all, not hallucinate a plausible answer from activation spreading.
///
/// Ranking within exact results: verified ground-truth (ledger/tool) first,
/// then tool-grounded (unverified but from a reliable tool), then chat-asserted.
fn exact_verified_recall(
    cue: &str,
    hippocampus: &crate::hippocampus::Hippocampus,
    life_id: uuid::Uuid,
    recalls: &mut Vec<crate::types::RecallResult>,
) {
    use crate::types::ProvenanceKind;

    let cue_low = cue.to_lowercase();
    // Tokenize into content keywords (skip stopwords, short tokens)
    let cue_tokens: Vec<String> = cue_low
        .split_whitespace()
        .filter(|t| t.len() > 2 && !matches!(*t, "the" | "and" | "for" | "what" | "how" | "is"))
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|t| !t.is_empty())
        .collect();

    if cue_tokens.is_empty() {
        return;
    }

    // Scan engrams in this life for verified / provenance-backed ones
    let mut matches: Vec<(&crate::engram::Engram, f32, u8)> = hippocampus
        .engrams_for_life(life_id)
        .filter_map(|e| {
            let prov = e.episode.provenance.as_ref();
            // Tier: 0=verified-ground-truth, 1=tool-grounded, 2=chat-asserted
            let tier: u8 = match prov {
                Some(p) if p.verified => 0,
                Some(p)
                    if matches!(
                        p.kind,
                        ProvenanceKind::ToolGrounded | ProvenanceKind::LedgerVerified
                    ) =>
                {
                    1
                }
                _ => 2,
            };
            // Only tiers 0 and 1 qualify for exact recall
            if tier > 1 {
                return None;
            }
            // Score: fraction of cue tokens found in engram content + context
            let text = format!("{} {}", e.episode.content, e.episode.context).to_lowercase();
            let matched = cue_tokens
                .iter()
                .filter(|t| text.contains(t.as_str()))
                .count();
            if matched == 0 {
                return None;
            }
            let score = matched as f32 / cue_tokens.len() as f32;
            if score < 0.25 {
                // At least 25% of query tokens must appear in the engram
                return None;
            }
            Some((e, score, tier))
        })
        .collect();

    if matches.is_empty() {
        return;
    }

    // Sort: tier ASC (verified first), score DESC (best match first)
    matches.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    // Inject exact results at activation 10.0 (guaranteed to win over any associative result).
    // Insert in reverse order so the best ends up at index 0.
    for (engram, score, tier) in matches.into_iter().take(3).rev() {
        let activation = 10.0 + score + (1.0 - tier as f32 * 0.3);
        if let Some(existing) = recalls.iter_mut().find(|r| r.engram_id == engram.id) {
            existing.activation = activation;
            existing.verified = tier == 0;
        } else {
            recalls.insert(
                0,
                crate::types::RecallResult {
                    engram_id: engram.id,
                    activation,
                    episode: engram.episode.clone(),
                    completion_strength: score,
                    separation_index: engram.separation_index,
                    verified: tier == 0,
                    trust_note: None,
                },
            );
        }
    }
    recalls.sort_by(|a, b| b.activation.partial_cmp(&a.activation).unwrap());
}

fn annotate_recall_trust(recalls: &mut [crate::types::RecallResult]) {
    for recall in recalls.iter_mut() {
        recall.verified = recall
            .episode
            .provenance
            .as_ref()
            .map(|p| p.verified)
            .unwrap_or(false);
        if recall.verified {
            recall.trust_note = None;
            continue;
        }
        let c = recall.episode.content.to_lowercase();
        let looks_factual = c.contains('$')
            || c.contains("balance")
            || c.contains("wallet")
            || c.contains("ledger")
            || c.contains("total")
            || c.chars().any(|ch| ch.is_ascii_digit());
        if looks_factual {
            recall.trust_note =
                Some("recalled utterance — not verified ground truth; check ledger/tools".into());
        }
    }
}

impl Drop for FluctlightBrain {
    fn drop(&mut self) {
        if self.store_path.is_some() && self.checkpoint_policy.pending_writes() > 0 {
            let _ = self.checkpoint();
        }
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ReconsolidateReport {
    pub updated: bool,
    pub engram_id: Uuid,
    pub content: String,
    pub revision: u32,
    pub labile_until_tick: u64,
    pub superseded_others: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BrainStatus {
    pub life_id: Uuid,
    pub stage: String,
    pub experiences: u64,
    /// Consolidation sleep cycles (development metric).
    pub sleep_cycles: u64,
    /// Autonomic auto-sleep count (brainstem).
    pub auto_sleeps: u64,
    pub sleeps_in_window: u32,
    pub synapses: usize,
    pub engrams: usize,
    pub core_memories: usize,
    pub semantic_engrams: usize,
    pub semantic_centroids: usize,
    pub pfc_unlocked: bool,
    pub alive: bool,
    pub autonomic_ticks: u64,
    pub ticks_since_sleep: u64,
    pub synapse_pressure: f32,
    pub wal_seq: u64,
    /// Organ health (Somnus cadence, prompt token estimates). Measurement only.
    #[serde(default)]
    pub homeostasis: crate::homeostasis::HomeostasisReport,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    #[test]
    fn brain_grows_automatically() {
        let mut brain = FluctlightBrain::new();
        assert_eq!(brain.stage(), DevStage::Newborn);
        for i in 0..3 {
            brain
                .experience(Episode {
                    content: format!("learned item {i}"),
                    context: "training".into(),
                    outcome: None,
                    salience_hint: 0.4,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .unwrap();
        }
        assert_eq!(brain.stage(), DevStage::Infant);
    }

    #[test]
    fn activate_recalls_experience() {
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode {
                content: "tool call failed timeout".into(),
                context: "api session".into(),
                outcome: Some("retry succeeded".into()),
                salience_hint: 0.8,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        let result = brain.activate("tool timeout");
        assert!(!result.recalls.is_empty());
    }

    #[test]
    fn autonomic_tick_increments_auto_sleeps_and_resets_counter() {
        let mut brain = FluctlightBrain::new();
        brain.autonomic.config.ticks_per_sleep = 3;
        brain.autonomic.config.max_auto_sleeps_per_hour = 100;
        let before = brain.autonomic.auto_sleeps;
        for _ in 0..4 {
            let _ = brain.tick();
        }
        assert!(brain.autonomic.auto_sleeps > before);
        assert!(brain.autonomic.ticks_since_sleep < 4);
    }

    #[test]
    fn separation_report_on_experience() {
        let mut brain = FluctlightBrain::new();
        let r = brain
            .experience(Episode {
                content: "dispatch timeout".into(),
                context: "prod".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        assert!(r.separation.token_count > 0);
        assert!(!r.separation.dg_neurons.is_empty());
    }

    #[test]
    fn death_preserves_core() {
        let mut brain = FluctlightBrain::new();
        let id = brain
            .experience(Episode {
                content: "user prefers concise answers".into(),
                context: "preference".into(),
                outcome: None,
                salience_hint: 0.9,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap()
            .engram_id;
        brain.mark_core(id, "user_style".into()).unwrap();
        brain.death("session reset").unwrap();
        assert!(brain
            .core_memories
            .memories
            .iter()
            .any(|m| m.key == "user_style"));
    }

    #[test]
    fn semantic_experience_registers_vector() {
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode {
                content: "database migration failed".into(),
                context: "ops".into(),
                outcome: None,
                salience_hint: 0.7,
                semantic_vector: Some(vec![0.2, 0.8, 0.1]),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        assert_eq!(brain.semantic.engram_vectors.len(), 1);
    }
}

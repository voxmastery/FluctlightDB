use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::activation::{activate_from_hybrid, cap_candidates, complete, default_candidate_cap};
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
use crate::prefrontal::Prefrontal;
use crate::raw_export::{export_raw, RawExport};
use crate::semantic::SemanticField;
use crate::sleep::{separate_and_encode, sleep_cycle};
use crate::sleep_trigger::SleepTrigger;
use crate::store;
use crate::types::Region::HippocampusCa1;
use crate::types::{
    ActivationResult, DevelopmentViz, Episode, ExperienceReport, SleepReport, VizExport,
};
use crate::wal::{self, WalEntry};

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
    pub semantic: SemanticField,
    #[serde(default)]
    pub recent_separations: Vec<SeparationResult>,
    #[serde(skip)]
    checkpoint_policy: CheckpointPolicy,
    #[serde(skip)]
    store_path: Option<PathBuf>,
    #[serde(skip)]
    recall_index: Option<RecallIndex>,
    #[serde(skip)]
    activation_cache: Mutex<ActivationCache>,
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
            semantic: SemanticField::default(),
            recent_separations: Vec::new(),
            checkpoint_policy: CheckpointPolicy::default(),
            store_path: None,
            recall_index: None,
            activation_cache: Mutex::new(ActivationCache::new()),
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
        if let Some(ref path) = self.store_path {
            store::save(self, path)?;
        }
        Ok(())
    }

    pub fn maybe_checkpoint(&mut self) -> Result<()> {
        self.checkpoint_policy.note_write();
        if self.checkpoint_policy.should_checkpoint() {
            self.checkpoint()?;
            self.checkpoint_policy.mark_checkpointed();
        }
        Ok(())
    }

    fn wal_append(&mut self, entry: WalEntry) -> Result<()> {
        if let Some(ref path) = self.store_path {
            self.wal_seq += 1;
            wal::append(path, self.wal_seq, &entry)?;
        }
        Ok(())
    }

    pub fn stage(&self) -> DevStage {
        self.development.stage
    }

    /// Encode lived experience — DG separate + CA3 wire + store engram.
    pub fn experience(&mut self, episode: Episode) -> Result<ExperienceReport> {
        if wal::wal_enabled() {
            self.wal_append(WalEntry::Experience {
                episode: episode.clone(),
            })?;
        }
        self.experience_internal(episode, true)
    }

    pub(crate) fn experience_internal(
        &mut self,
        episode: Episode,
        checkpoint: bool,
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
        let gate = self.neuromodulators.plasticity_gate(salience);
        if episode.salience_hint > 0.5 {
            self.neuromodulators.on_surprise(episode.salience_hint);
        }

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
        );

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
        self.autonomic.roll_sleep_window(self.autonomic.total_ticks);

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
        } else if checkpoint {
            self.maybe_checkpoint()?;
        }

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

    pub fn activate_with_semantic(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
    ) -> ActivationResult {
        self.activate_scoped(cue, cue_vector, None)
    }

    pub fn activate_scoped(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        agent_id: Option<&str>,
    ) -> ActivationResult {
        if let Some(cached) = self.activation_cache.lock().unwrap().get(cue, agent_id) {
            return cached;
        }

        let candidate_set: Option<HashSet<uuid::Uuid>> = self
            .recall_index
            .as_ref()
            .and_then(|idx| {
                idx.hybrid_candidates(cue, cue_vector, &self.semantic, default_candidate_cap())
                    .ok()
            })
            .map(|ids| cap_candidates(ids, default_candidate_cap()));

        let mut result = activate_from_hybrid(
            cue,
            cue_vector,
            &self.graph,
            &self.hippocampus,
            &self.semantic,
            self.life.life_id,
            4,
            self.development.stage.myelination(),
            8,
            candidate_set.as_ref(),
        );
        let cortex_boost = self.cortex.fact_boost(cue) + self.cortex.semantic_boost(cue_vector);
        let field_boost = cue_vector
            .map(|v| self.semantic.centroid_boost(v))
            .unwrap_or(0.0);
        for recall in &mut result.recalls {
            recall.activation += (cortex_boost + field_boost) * 0.1;
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
        annotate_recall_trust(&mut result.recalls);
        self.activation_cache
            .lock()
            .unwrap()
            .put(cue, agent_id, result.clone());
        result
    }

    /// Batch activate — one brain lock, many cues (production agent hot path).
    pub fn activate_batch(
        &self,
        items: &[(String, Option<Vec<f32>>, Option<String>)],
    ) -> Vec<ActivationResult> {
        items
            .iter()
            .map(|(cue, vec, agent)| self.activate_scoped(cue, vec.as_deref(), agent.as_deref()))
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
        self.development.on_sleep(report.pruned_synapses);
        self.prefrontal.unlocked = self.development.pfc_unlocked();
        report.stage_after = self.development.stage.as_str().to_string();
        report.advanced = stage_before != report.stage_after;

        match trigger {
            SleepTrigger::Autonomic | SleepTrigger::Pressure => self.autonomic.on_sleep(),
            SleepTrigger::Manual => {}
        }

        if self.development.metrics.sleep_cycles % COMPACT_EVERY_N_SLEEPS == 0 {
            let _ = self.compact_internal(false);
        }

        if checkpoint {
            self.maybe_checkpoint()?;
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
        let _ = self.checkpoint();
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

    pub fn invalidate_activation_cache(&mut self) {
        self.activation_cache.lock().unwrap().invalidate();
    }

    pub fn attach_store_path(&mut self, path: PathBuf) {
        self.store_path = Some(path.clone());
        self.recall_index = RecallIndex::open_sidecar(&path)
            .or_else(|_| RecallIndex::rebuild(self))
            .ok();
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
        }
    }

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
            semantic,
            recent_separations,
            checkpoint_policy: CheckpointPolicy::default(),
            store_path: None,
            recall_index: None,
            activation_cache: Mutex::new(ActivationCache::new()),
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
            semantic: self.semantic.clone(),
            recent_separations: self.recent_separations.clone(),
            checkpoint_policy: self.checkpoint_policy.clone(),
            store_path: self.store_path.clone(),
            recall_index: None,
            activation_cache: Mutex::new(cache),
        }
    }
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
}

fn any_contains(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
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

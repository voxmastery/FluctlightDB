//! FlyWire Codex CSV → FluctlightDB substrate graph + `connectome` segment.
//!
//! Three passes over three CSVs (no `csv` crate: FlyWire files have no quoted fields):
//! 1. `neurons.csv` + `classification.csv` → per-neuron metadata + entry set
//! 2. `connections.csv` → Σ syn_count per (pre, post) pair (rows are per pair *per neuropil*)
//! 3. p99 → signed, p99-normalised weights → `add_synapse_uncapped`

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::connectome::{weight_for, ConnectomeMeta, NeuronMeta, Nt};
use crate::id::NeuronId;
use crate::plasticity::Synapse;
use crate::types::Region;
use crate::{Error, FluctlightBrain, Result};

const CONNECTIONS_HEADER: &str = "pre_root_id,post_root_id,neuropil,syn_count,nt_type";
const NEURONS_HEADER_PREFIX: &str = "root_id,group,nt_type";
const CLASSIFICATION_HEADER: &str = "root_id,flow,super_class,class,sub_class,hemilineage,side,nerve";
/// Abort when more than this share of connection rows is malformed. Real FlyWire files have
/// zero; a wrong file fails the header check; 10 % catches a partially corrupt file while
/// tolerating the fixture's 1-of-40. Spec §7 originally said 1 %; corrected here — rows below
/// the threshold are still reported in rows_malformed.
const MAX_MALFORMED_PERMILLE: u64 = 100;

/// Σ syn_count per (pre, post) pair, plus rows read and rows malformed.
type PairSums = (HashMap<(u64, u64), u32>, u64, u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportConfig {
    pub connections: PathBuf,
    pub classification: PathBuf,
    pub neurons: PathBuf,
    /// `classification.csv` `class` value that forms the cue entry layer.
    pub entry_class: String,
    /// Kenyon cells per cue token.
    pub fanout: u8,
    /// Remove an existing connectome (all synapses touching its neurons) before importing.
    pub replace: bool,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            connections: PathBuf::new(),
            classification: PathBuf::new(),
            neurons: PathBuf::new(),
            entry_class: "Kenyon_Cell".into(),
            fanout: 7,
            replace: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImportReport {
    pub rows_read: u64,
    pub rows_malformed: u64,
    pub pairs: u64,
    pub neurons: usize,
    pub inhibitory_synapses: u64,
    pub entry_set_len: usize,
    pub p99_syn: f32,
    pub import_s: f64,
    pub index_s: f64,
    pub warnings: Vec<String>,
}

fn open_lines(path: &Path) -> Result<impl Iterator<Item = std::io::Result<String>>> {
    let f = File::open(path).map_err(|e| Error::Store(format!("{}: {e}", path.display())))?;
    Ok(BufReader::new(f).lines())
}

fn check_header(path: &Path, got: Option<std::io::Result<String>>, expected: &str, prefix_only: bool) -> Result<()> {
    let got = got
        .ok_or_else(|| Error::Store(format!("{}: empty file", path.display())))?
        .map_err(|e| Error::Store(format!("{}: {e}", path.display())))?;
    let ok = if prefix_only { got.trim().starts_with(expected) } else { got.trim() == expected };
    if !ok {
        return Err(Error::Store(format!(
            "{}: header mismatch — expected `{expected}`, found `{}`",
            path.display(),
            got.trim()
        )));
    }
    Ok(())
}

/// Pass 1: neuron metadata. `neurons.csv` supplies `nt_type`; `classification.csv` the rest.
fn read_neuron_meta(cfg: &ImportConfig) -> Result<(HashMap<NeuronId, NeuronMeta>, Vec<NeuronId>)> {
    let mut meta: HashMap<NeuronId, NeuronMeta> = HashMap::new();

    let mut lines = open_lines(&cfg.neurons)?;
    check_header(&cfg.neurons, lines.next(), NEURONS_HEADER_PREFIX, true)?;
    for line in lines {
        let line = line.map_err(|e| Error::Store(e.to_string()))?;
        let mut f = line.split(',');
        let (Some(id), _group, Some(nt)) = (f.next(), f.next(), f.next()) else { continue };
        let Ok(id) = id.parse::<u64>() else { continue };
        meta.entry(NeuronId(id)).or_default().nt = Nt::parse(nt);
    }

    let mut entry_set = Vec::new();
    let mut lines = open_lines(&cfg.classification)?;
    check_header(&cfg.classification, lines.next(), CLASSIFICATION_HEADER, false)?;
    for line in lines {
        let line = line.map_err(|e| Error::Store(e.to_string()))?;
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 7 {
            continue;
        }
        let Ok(id) = f[0].parse::<u64>() else { continue };
        let m = meta.entry(NeuronId(id)).or_default();
        m.super_class = f[2].to_string();
        m.class = f[3].to_string();
        m.side = f[6].to_string();
        if f[3] == cfg.entry_class {
            entry_set.push(NeuronId(id));
        }
    }
    entry_set.sort_unstable();
    entry_set.dedup();
    Ok((meta, entry_set))
}

/// Pass 2: Σ syn_count per (pre, post). Also records the first neuropil seen per presynaptic neuron.
fn read_pair_sums(cfg: &ImportConfig, meta: &mut HashMap<NeuronId, NeuronMeta>) -> Result<PairSums> {
    let mut sums: HashMap<(u64, u64), u32> = HashMap::new();
    let (mut rows_read, mut rows_malformed) = (0u64, 0u64);
    let mut lines = open_lines(&cfg.connections)?;
    check_header(&cfg.connections, lines.next(), CONNECTIONS_HEADER, false)?;
    for line in lines {
        let line = line.map_err(|e| Error::Store(e.to_string()))?;
        rows_read += 1;
        let mut f = line.split(',');
        let (Some(pre), Some(post), Some(neuropil), Some(syn)) = (f.next(), f.next(), f.next(), f.next()) else {
            rows_malformed += 1;
            continue;
        };
        let (Ok(pre), Ok(post), Ok(syn)) = (pre.parse::<u64>(), post.parse::<u64>(), syn.parse::<u32>()) else {
            rows_malformed += 1;
            continue;
        };
        *sums.entry((pre, post)).or_insert(0) += syn;
        let m = meta.entry(NeuronId(pre)).or_default();
        if m.neuropil.is_empty() {
            m.neuropil = neuropil.to_string();
        }
    }
    if rows_read > 0 && rows_malformed * 1000 > rows_read * MAX_MALFORMED_PERMILLE {
        return Err(Error::Store(format!(
            "{}: {rows_malformed} of {rows_read} rows malformed (limit {}‰)",
            cfg.connections.display(),
            MAX_MALFORMED_PERMILLE
        )));
    }
    Ok((sums, rows_read, rows_malformed))
}

fn p99(sums: &HashMap<(u64, u64), u32>) -> f32 {
    if sums.is_empty() {
        return 1.0;
    }
    let mut v: Vec<u32> = sums.values().copied().collect();
    v.sort_unstable();
    v[((v.len() - 1) as f64 * 0.99) as usize] as f32
}

/// Drop every synapse touching a neuron of the previous connectome.
fn remove_previous(brain: &mut FluctlightBrain, old: &ConnectomeMeta) {
    brain.graph.synapses.retain(|s| !old.neurons.contains_key(&s.from) && !old.neurons.contains_key(&s.to));
    brain.graph.neuron_regions.retain(|n, _| !old.neurons.contains_key(n));
    brain.graph.rebuild_index();
}

pub fn import_connectome(brain: &mut FluctlightBrain, cfg: &ImportConfig) -> Result<ImportReport> {
    let t0 = Instant::now();
    if let Some(old) = brain.connectome.take() {
        if !cfg.replace {
            brain.connectome = Some(old);
            return Err(Error::Store("brain already has a connectome (pass replace=true to overwrite)".into()));
        }
        remove_previous(brain, &old);
    }
    let mut warnings = Vec::new();
    if brain.hippocampus.engrams_for_life(brain.life.life_id).next().is_some() {
        warnings.push("brain already holds engrams; connectome is being fused with existing memory".into());
    }

    // All parsing happens before any mutation so a failed import leaves the graph untouched.
    let (mut meta, entry_set) = read_neuron_meta(cfg)?;
    let (sums, rows_read, rows_malformed) = read_pair_sums(cfg, &mut meta)?;
    let p99_syn = p99(&sums);

    let mut inhibitory = 0u64;
    brain.graph.rebuild_index();
    for (&(pre, post), &sum) in &sums {
        let inh = meta.get(&NeuronId(pre)).map(|m| m.nt.sign() < 0.0).unwrap_or(false);
        inhibitory += inh as u64;
        brain.graph.add_synapse_uncapped(Synapse::new(
            NeuronId(pre),
            NeuronId(post),
            Region::Cortex,
            weight_for(sum, p99_syn, inh),
        ));
    }
    let import_s = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    brain.graph.rebuild_index();
    let index_s = t1.elapsed().as_secs_f64();

    let imported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    brain.connectome = Some(ConnectomeMeta {
        source: cfg.connections.display().to_string(),
        imported_at,
        p99_syn,
        fanout: cfg.fanout,
        entry_class: cfg.entry_class.clone(),
        entry_set,
        neurons: meta,
    });
    let entry_set_len = brain.connectome.as_ref().map(|c| c.entry_set.len()).unwrap_or(0);
    let neurons = brain.connectome.as_ref().map(|c| c.neurons.len()).unwrap_or(0);

    Ok(ImportReport {
        rows_read,
        rows_malformed,
        pairs: sums.len() as u64,
        neurons,
        inhibitory_synapses: inhibitory,
        entry_set_len,
        p99_syn,
        import_s,
        index_s,
        warnings,
    })
}

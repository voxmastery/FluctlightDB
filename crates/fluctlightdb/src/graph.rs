use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::calcium::calcium_stdp_fast;
use crate::id::NeuronId;
use crate::plasticity::{hebbian_strengthen, ltd_weaken, Synapse};
use crate::types::Region;

/// The connectome — neurons linked by weighted synapses (not a graph DB API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainGraph {
    pub synapses: Vec<Synapse>,
    pub neuron_regions: HashMap<NeuronId, Region>,
    #[serde(default)]
    synapse_index: HashMap<(u64, u64), usize>,
    /// Outgoing-edge index: `from` -> positions in `synapses`.
    ///
    /// `synapse_index` is keyed by the (from, to) PAIR, so it can answer "does this exact edge
    /// exist" but not "what leaves this neuron" — which is the question spreading activation asks
    /// once per active neuron per hop. Without this, `neighbors()` scanned the whole synapse Vec
    /// every time: cost `hops * |active| * |synapses|`. Measured on a 4k-engram brain with real
    /// token overlap, activate() went 13ms -> 486ms as the graph grew, while the same brain with
    /// spreading disabled stayed flat at 4-9ms. Production (10.6k engrams / 328k synapses) was
    /// taking 5-30s and blowing ServerBrain's 6s recall cap, which is what left the bot with no
    /// memory at all.
    ///
    /// Not serialised: it is derived state, rebuilt by `rebuild_index()`. While
    /// `adjacency_ready` is false `neighbors()` falls back to the linear scan, so a path that
    /// forgets to rebuild is merely slow, never wrong.
    #[serde(skip)]
    adjacency: HashMap<u64, Vec<u32>>,
    /// Is `adjacency` a complete mirror of `synapses`?
    ///
    /// Emptiness cannot answer this: a graph with no edges has a legitimately empty adjacency,
    /// while a graph deserialised from disk has a full `synapses` Vec and an empty adjacency that
    /// must NOT be trusted. Serde skips this, so anything loaded starts `false` and uses the slow
    /// path until `rebuild_index()` runs. `Default` starts `true` because an empty graph really is
    /// fully indexed, which lets an incrementally built brain stay fast without an explicit rebuild.
    #[serde(skip)]
    adjacency_ready: bool,
}

impl Default for BrainGraph {
    fn default() -> Self {
        Self {
            synapses: Vec::new(),
            neuron_regions: HashMap::new(),
            synapse_index: HashMap::new(),
            adjacency: HashMap::new(),
            adjacency_ready: true,
        }
    }
}

impl BrainGraph {
    pub fn register_neuron(&mut self, id: NeuronId, region: Region) {
        self.neuron_regions.entry(id).or_insert(region);
    }

    /// Add or strengthen — dedup (from,to) keeping max weight.
    pub fn add_synapse(&mut self, synapse: Synapse) {
        self.register_neuron(synapse.from, synapse.region);
        self.register_neuron(synapse.to, synapse.region);
        let key = (synapse.from.0, synapse.to.0);
        if let Some(&idx) = self.synapse_index.get(&key) {
            if self.synapses[idx].weight < synapse.weight {
                self.synapses[idx].weight = synapse.weight;
            }
            return;
        }
        let idx = self.synapses.len();
        self.synapses.push(synapse);
        self.synapse_index.insert(key, idx);
        // Only extend adjacency while it is a complete mirror. Appending to a stale map (e.g. a
        // just-deserialised graph) would leave it holding ONLY the new edges, and neighbors()
        // would trust it and miss every pre-existing one.
        if self.adjacency_ready {
            self.adjacency.entry(key.0).or_default().push(idx as u32);
        }
    }

    pub fn rebuild_index(&mut self) {
        self.synapse_index.clear();
        self.adjacency.clear();
        self.adjacency.reserve(self.synapses.len());
        for (i, s) in self.synapses.iter().enumerate() {
            self.synapse_index.insert((s.from.0, s.to.0), i);
            self.adjacency.entry(s.from.0).or_default().push(i as u32);
        }
        self.adjacency_ready = true;
    }

    pub fn synapse_count(&self) -> usize {
        self.synapses.len()
    }

    /// Outgoing edges of `from`. O(degree) via `adjacency`; falls back to a full scan (O(|synapses|))
    /// when the index is not built, so correctness never depends on remembering to rebuild.
    pub fn neighbors(&self, from: NeuronId) -> Box<dyn Iterator<Item = (&Synapse, NeuronId)> + '_> {
        if self.adjacency_ready {
            match self.adjacency.get(&from.0) {
                Some(idxs) => Box::new(idxs.iter().map(move |&i| {
                    let s = &self.synapses[i as usize];
                    (s, s.to)
                })),
                None => Box::new(std::iter::empty()),
            }
        } else {
            Box::new(
                self.synapses
                    .iter()
                    .filter(move |s| s.from == from)
                    .map(|s| (s, s.to)),
            )
        }
    }

    pub fn co_activate(&mut self, active: &HashSet<NeuronId>, gate: f32) {
        // Hebbian plasticity is LOCAL — only synapses between co-active neurons change, so only
        // those need visiting. The previous full sweep cost O(|synapses|) with the brain write
        // lock held; on the production graph (~95M synapses / 3.4G) that is seconds of memory
        // traffic per experience(), which is what queued writers into "brain lock busy for 120s"
        // and starved every recall. Walking adjacency costs O(Σ degree(active)) instead —
        // typically a few thousand edges for an engram's ~10² active neurons.
        if self.adjacency_ready {
            let touched = self.edges_between(active, active);
            for i in touched {
                hebbian_strengthen(&mut self.synapses[i as usize], gate, 0.05);
            }
        } else {
            // Index not built (fresh deserialise before from_snapshot's rebuild): stay correct.
            for synapse in &mut self.synapses {
                if active.contains(&synapse.from) && active.contains(&synapse.to) {
                    hebbian_strengthen(synapse, gate, 0.05);
                }
            }
        }
    }

    /// Indices of synapses running `from ∈ sources` → `to ∈ targets`, via adjacency.
    /// Collected first so the caller can mutate `synapses` without aliasing the index.
    fn edges_between(&self, sources: &HashSet<NeuronId>, targets: &HashSet<NeuronId>) -> Vec<u32> {
        let mut out = Vec::new();
        for from in sources {
            if let Some(idxs) = self.adjacency.get(&from.0) {
                for &i in idxs {
                    if targets.contains(&self.synapses[i as usize].to) {
                        out.push(i);
                    }
                }
            }
        }
        out
    }

    pub fn prune_below(&mut self, threshold: f32) -> u32 {
        let before = self.synapses.len();
        self.synapses.retain(|s| s.weight >= threshold);
        let pruned = (before - self.synapses.len()) as u32;
        if pruned > 0 {
            self.rebuild_index();
        }
        pruned
    }

    pub fn weaken_unused(&mut self, active: &HashSet<NeuronId>, delta: f32) {
        for synapse in &mut self.synapses {
            if !active.contains(&synapse.from) && !active.contains(&synapse.to) {
                ltd_weaken(synapse, delta);
            }
        }
    }

    /// DA-gated STDP on a directed sequential pair (Frémaux & Gerstner 2016).
    ///
    /// Called during SWR replay: neurons from the EARLIER engram are pre-synaptic;
    /// neurons from the LATER engram are post-synaptic. Causal sequences that were
    /// encoded in temporal order get LTP-amplified — this is how the brain consolidates
    /// causal chains (A→B→C) during sleep instead of just strengthening co-activations.
    ///
    /// `pre_tick` / `post_tick`: encoded_at_tick values from the two engrams (used as
    /// the discrete spike-timing difference). Engrams encoded many ticks apart will
    /// fall outside the STDP window and won't be modified — only tight sequences consolidate.
    pub fn stdp_sequential(
        &mut self,
        pre_neurons: &HashSet<NeuronId>,
        post_neurons: &HashSet<NeuronId>,
        pre_tick: u64,
        post_tick: u64,
        da_gate: f32,
    ) {
        // Map brain ticks to a STDP-window-friendly scale: divide by 10 so that
        // engrams encoded 1–20 ticks apart fall inside the ±100ms calcium window.
        let pre_ms = (pre_tick / 10) as f32;
        let post_ms = (post_tick / 10) as f32;
        let delta_t_ms = post_ms - pre_ms; // positive = LTP (pre before post)
        let dw = calcium_stdp_fast(delta_t_ms, da_gate);
        if dw.abs() > 1e-6 {
            // Same locality argument as co_activate: STDP touches only pre→post synapses, and
            // SWR replay calls this once per consecutive engram pair while sleep holds the brain
            // lock — a full sweep per pair made consolidation O(pairs × |synapses|).
            if self.adjacency_ready {
                let touched = self.edges_between(pre_neurons, post_neurons);
                for i in touched {
                    let s = &mut self.synapses[i as usize];
                    s.weight = (s.weight + dw).clamp(0.001, 1.0);
                }
            } else {
                for synapse in &mut self.synapses {
                    if pre_neurons.contains(&synapse.from) && post_neurons.contains(&synapse.to) {
                        synapse.weight = (synapse.weight + dw).clamp(0.001, 1.0);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_neighbors(g: &BrainGraph, n: NeuronId) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = g.neighbors(n).map(|(s, to)| (s.from.0, to.0)).collect();
        v.sort_unstable();
        v
    }

    fn wire(g: &mut BrainGraph, from: u64, to: u64, w: f32) {
        let mut s = Synapse::new(NeuronId(from), NeuronId(to), Region::HippocampusCa3, w);
        s.weight = w;
        g.add_synapse(s);
    }

    /// The indexed fast path must return exactly what the linear scan returns — the whole point of
    /// the adjacency map is that it changes speed, never results.
    #[test]
    fn adjacency_matches_linear_scan() {
        let mut g = BrainGraph::default();
        for i in 0..40u64 {
            wire(&mut g, i % 7, i, 0.3 + (i as f32) * 0.001);
        }
        for n in 0..7u64 {
            let fast = sorted_neighbors(&g, NeuronId(n));
            // Force the slow path and compare.
            let mut slow_graph = g.clone();
            slow_graph.adjacency.clear();
            slow_graph.adjacency_ready = false;
            assert_eq!(fast, sorted_neighbors(&slow_graph, NeuronId(n)), "neuron {n}");
            assert!(!fast.is_empty(), "neuron {n} should have edges");
        }
    }

    /// A graph loaded from disk has synapses but no adjacency; it must NOT report zero neighbours.
    #[test]
    fn deserialised_graph_still_returns_neighbors() {
        let mut g = BrainGraph::default();
        wire(&mut g, 1, 2, 0.5);
        wire(&mut g, 1, 3, 0.5);
        // bincode, not JSON: neuron_regions is keyed by NeuronId and JSON demands string keys.
        // bincode is what store.rs actually persists with, so this exercises the real load path.
        let round: BrainGraph = bincode::deserialize(&bincode::serialize(&g).unwrap()).unwrap();
        assert!(!round.adjacency_ready, "serde must not resurrect the index flag");
        assert_eq!(sorted_neighbors(&round, NeuronId(1)).len(), 2);
        let mut rebuilt = round.clone();
        rebuilt.rebuild_index();
        assert_eq!(sorted_neighbors(&rebuilt, NeuronId(1)), sorted_neighbors(&round, NeuronId(1)));
    }

    /// prune_below() shifts positions; adjacency must not point at the wrong synapses afterwards.
    #[test]
    fn adjacency_survives_pruning() {
        let mut g = BrainGraph::default();
        wire(&mut g, 1, 2, 0.9);
        wire(&mut g, 1, 3, 0.05);
        wire(&mut g, 1, 4, 0.8);
        g.prune_below(0.1);
        let got = sorted_neighbors(&g, NeuronId(1));
        assert_eq!(got, vec![(1, 2), (1, 4)], "weak edge should be gone, rest intact");
        for (s, to) in g.neighbors(NeuronId(1)) {
            assert_eq!(s.to, to, "adjacency index points at a mismatched synapse");
        }
    }

    #[test]
    fn dedup_keeps_max_weight() {
        let mut g = BrainGraph::default();
        let a = NeuronId::from_token("a");
        let b = NeuronId::from_token("b");
        g.add_synapse(Synapse::new(a, b, Region::HippocampusCa3, 0.3));
        g.add_synapse(Synapse::new(a, b, Region::HippocampusCa3, 0.8));
        assert_eq!(g.synapse_count(), 1);
        assert!((g.synapses[0].weight - 0.8).abs() < 1e-5);
    }

    /// Deterministic pseudo-random graph for plasticity tests (no rand dep).
    fn lcg_graph(n_syn: u64, n_neurons: u64) -> BrainGraph {
        let mut g = BrainGraph::default();
        let mut x: u64 = 0x2545F4914F6CDD1D;
        for _ in 0..n_syn {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let from = (x >> 33) % n_neurons;
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let to = (x >> 33) % n_neurons;
            wire(&mut g, from, to, 0.30);
        }
        g
    }

    /// The indexed co_activate must produce byte-identical weights to the full sweep.
    #[test]
    fn co_activate_indexed_matches_full_sweep() {
        let active: HashSet<NeuronId> = (0..40).map(|i| NeuronId(i * 7)).collect();

        let mut fast = lcg_graph(20_000, 500);
        assert!(fast.adjacency_ready);
        fast.co_activate(&active, 0.9);

        let mut slow = lcg_graph(20_000, 500);
        slow.adjacency_ready = false; // force the legacy full-sweep path
        slow.co_activate(&active, 0.9);

        let key = |g: &BrainGraph| {
            let mut v: Vec<(u64, u64, u32)> = g
                .synapses
                .iter()
                .map(|s| (s.from.0, s.to.0, s.weight.to_bits()))
                .collect();
            v.sort_unstable();
            v
        };
        assert_eq!(key(&fast), key(&slow), "indexed plasticity diverged from sweep");
    }

    /// Same equivalence for STDP consolidation.
    #[test]
    fn stdp_indexed_matches_full_sweep() {
        let pre: HashSet<NeuronId> = (0..30).map(|i| NeuronId(i * 3)).collect();
        let post: HashSet<NeuronId> = (0..30).map(|i| NeuronId(i * 5 + 1)).collect();

        let mut fast = lcg_graph(20_000, 400);
        fast.stdp_sequential(&pre, &post, 100, 110, 0.8);

        let mut slow = lcg_graph(20_000, 400);
        slow.adjacency_ready = false;
        slow.stdp_sequential(&pre, &post, 100, 110, 0.8);

        let key = |g: &BrainGraph| {
            let mut v: Vec<(u64, u64, u32)> = g
                .synapses
                .iter()
                .map(|s| (s.from.0, s.to.0, s.weight.to_bits()))
                .collect();
            v.sort_unstable();
            v
        };
        assert_eq!(key(&fast), key(&slow), "indexed STDP diverged from sweep");
    }

    /// Locality guard: on a large graph the indexed path must beat the sweep by >10x.
    /// (Production motivation: ~95M synapses made every experience() sweep 3.4G under the
    /// brain write lock — "brain lock busy for 120s".)
    #[test]
    fn co_activate_indexed_is_local_not_global() {
        let active: HashSet<NeuronId> = (0..50).map(|i| NeuronId(i * 11)).collect();
        let mut g = lcg_graph(2_000_000, 20_000);

        let t0 = std::time::Instant::now();
        g.co_activate(&active, 0.9);
        let fast = t0.elapsed();

        g.adjacency_ready = false;
        let t1 = std::time::Instant::now();
        g.co_activate(&active, 0.9);
        let slow = t1.elapsed();

        assert!(
            slow > fast * 10,
            "expected >10x from locality; sweep={slow:?} indexed={fast:?}"
        );
    }

}

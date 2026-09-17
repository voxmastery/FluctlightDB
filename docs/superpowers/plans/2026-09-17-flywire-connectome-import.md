# FlyWire Connectome Import + Cue Fusion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import the FlyWire v783 whole-brain connectome into a FluctlightDB tenant as the substrate graph, with inhibitory synapses honoured by spread and text cues projected onto Kenyon cells.

**Architecture:** Two new modules (`connectome.rs` = metadata + projection, `import_connectome.rs` = CSV ingestion) and an additive `connectome` segment; four plasticity guard points freeze negative weights; the spread loop in `activation.rs` is extracted into a sign-aware helper and `activate_from_hybrid` gains an `extra_seeds` parameter that `FluctlightBrain::activate` fills from the token→Kenyon projection. No new env flags; behaviour switches on `brain.connectome.is_some()`.

**Tech Stack:** Rust 2021 (workspace), serde, `tempfile` (tests), no new crates. CSV parsing is manual `split(',')` — the files have no quoted fields.

**Spec:** `docs/superpowers/specs/2026-09-17-flywire-connectome-import-design.md`

## Global Constraints

- No change to `format_version` (`manifest.rs:18`). Fly metadata lives only in the additive segment `connectome`, read with `unwrap_or_default()`.
- `activate(&self)` stays `&self`. `activate_from_hybrid` keeps `#[allow(clippy::too_many_arguments)]`.
- Brains with `connectome == None` must produce activation identical (±1e-6) to before this plan.
- Real FlyWire CSVs are never committed. Only `crates/fluctlightdb/tests/fixtures/connectome/*.csv` (20 neurons) is committed.
- Every commit: `cargo clippy -p fluctlightdb` clean on new/modified files; `cargo test -p fluctlightdb --lib` green.
- Spec corrections carried by this plan (the plan follows the code, not the spec text): the spread rule is **additive** (`activation.rs:145-153`), not `max`; there are **four** plasticity guard points (spec said three) because `prune_below` would delete negative weights; the connectome HTTP route is `POST`-dispatched like every other brain route in `serve.rs`, and answers `{"present": false}` instead of 404.

---

## File Structure

| file | responsibility |
|---|---|
| `crates/fluctlightdb/src/connectome.rs` (new) | `Nt`, `NeuronMeta`, `ConnectomeMeta`, FNV projection, weight formula, JSON summary |
| `crates/fluctlightdb/src/import_connectome.rs` (new) | `ImportConfig`, `ImportReport`, `import_connectome()` — CSV → graph + meta |
| `crates/fluctlightdb/src/brain.rs` | `pub connectome` field; projection wiring in `activate` |
| `crates/fluctlightdb/src/manifest.rs` | segment list / write / read for `connectome` |
| `crates/fluctlightdb/src/graph.rs` | `add_synapse_uncapped`; guards in `co_activate`, `weaken_unused`, `prune_below` |
| `crates/fluctlightdb/src/calcium.rs` | guard in `tick` |
| `crates/fluctlightdb/src/activation.rs` | `spread()` helper; `extra_seeds` param; `connectome_seeds` result field |
| `crates/fluctlightdb/src/types.rs` | `ActivationResult.connectome_seeds` |
| `crates/fluctlightdb/src/serve.rs` | `/api/v1/connectome` |
| `crates/fluctlightdb/src/lib.rs` | `pub mod connectome; pub mod import_connectome; pub use import_connectome::{ImportConfig, ImportReport, import_connectome};` |
| `crates/fluctlight-cli/src/main.rs` | `import-connectome` subcommand + usage line |
| `crates/fluctlightdb/tests/fixtures/connectome/{connections,neurons,classification}.csv` (new) | 20-neuron fixture |
| `crates/fluctlightdb/tests/connectome_import.rs` (new) | integration tests |
| `crates/fluctlightdb/tests/connectome_perf.rs` (new) | ignored, env-gated full-v783 perf test |
| `docs/runbooks/connectome-import.md` (new), `CHANGELOG.md` | docs |

---

### Task 1: `connectome.rs` — metadata, projection, weight formula

**Files:**
- Create: `crates/fluctlightdb/src/connectome.rs`
- Modify: `crates/fluctlightdb/src/lib.rs` (after line 11 `pub mod attention_schema;`)

**Interfaces:**
- Consumes: `crate::id::NeuronId` (`pub struct NeuronId(pub u64)`).
- Produces: `Nt`, `NeuronMeta`, `ConnectomeMeta { source, imported_at, p99_syn, fanout, entry_class, entry_set, neurons }`, `ConnectomeMeta::project_tokens(&self, tokens: &[String]) -> Vec<NeuronId>`, `ConnectomeMeta::is_inhibitory(&self, pre: NeuronId) -> bool`, `ConnectomeMeta::summary(&self) -> serde_json::Value`, `pub fn weight_for(sum_syn: u32, p99_syn: f32, inhibitory: bool) -> f32`, `pub fn fnv1a64(token: &str, k: u8) -> u64`.

- [ ] **Step 1: Write the failing tests**

Create `crates/fluctlightdb/src/connectome.rs` with only the test module first:

```rust
//! Connectome metadata + cue projection (FlyWire substrate).
//!
//! Metadata about imported biological neurons lives here, beside the graph, in its own
//! additive segment — `Region` is serde-by-name with no fallback, so it is never extended.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::NeuronId;

    fn meta() -> ConnectomeMeta {
        let mut m = ConnectomeMeta {
            source: "test".into(),
            imported_at: 0,
            p99_syn: 78.0,
            fanout: 7,
            entry_class: "Kenyon_Cell".into(),
            entry_set: vec![NeuronId(1001), NeuronId(1002), NeuronId(1003), NeuronId(1004), NeuronId(1005)],
            neurons: Default::default(),
        };
        m.neurons.insert(NeuronId(4001), NeuronMeta { nt: Nt::Gaba, super_class: "central".into(), class: "APL".into(), side: "right".into(), neuropil: "MB_CA_R".into() });
        m.neurons.insert(NeuronId(2001), NeuronMeta { nt: Nt::Ach, super_class: "central".into(), class: "ALPN".into(), side: "right".into(), neuropil: "AL_R".into() });
        m
    }

    #[test]
    fn nt_sign_is_negative_only_for_gaba_and_glut() {
        assert_eq!(Nt::Gaba.sign(), -1.0);
        assert_eq!(Nt::Glut.sign(), -1.0);
        for nt in [Nt::Ach, Nt::Da, Nt::Ser, Nt::Oct, Nt::Unknown] {
            assert_eq!(nt.sign(), 1.0);
        }
    }

    #[test]
    fn nt_parses_flywire_labels_case_insensitively() {
        assert_eq!(Nt::parse("GABA"), Nt::Gaba);
        assert_eq!(Nt::parse("ach"), Nt::Ach);
        assert_eq!(Nt::parse("GLUT"), Nt::Glut);
        assert_eq!(Nt::parse(""), Nt::Unknown);
        assert_eq!(Nt::parse("weird"), Nt::Unknown);
    }

    #[test]
    fn weight_is_monotone_signed_and_clamped() {
        assert!(weight_for(1, 78.0, false) < weight_for(8, 78.0, false));
        assert!(weight_for(8, 78.0, false) < weight_for(78, 78.0, false));
        assert_eq!(weight_for(78, 78.0, false), 1.0);
        assert_eq!(weight_for(2405, 78.0, false), 1.0);
        assert_eq!(weight_for(8, 78.0, true), -weight_for(8, 78.0, false));
        assert!((weight_for(8, 78.0, false) - 0.503).abs() < 0.01);
    }

    #[test]
    fn projection_is_deterministic_and_in_range() {
        let m = meta();
        let a = m.project_tokens(&["cue".to_string(), "word".to_string()]);
        let b = m.project_tokens(&["cue".to_string(), "word".to_string()]);
        assert_eq!(a, b);
        assert_eq!(a.len(), 2 * 7);
        for n in &a {
            assert!(m.entry_set.contains(n), "{n:?} not in entry set");
        }
    }

    #[test]
    fn projection_of_no_tokens_or_empty_entry_set_is_empty() {
        let mut m = meta();
        assert!(m.project_tokens(&[]).is_empty());
        m.entry_set.clear();
        assert!(m.project_tokens(&["cue".to_string()]).is_empty());
    }

    #[test]
    fn fnv_differs_across_k_and_token() {
        assert_ne!(fnv1a64("cue", 0), fnv1a64("cue", 1));
        assert_ne!(fnv1a64("cue", 0), fnv1a64("cub", 0));
    }

    #[test]
    fn is_inhibitory_reads_pre_neuron_nt() {
        let m = meta();
        assert!(m.is_inhibitory(NeuronId(4001)));
        assert!(!m.is_inhibitory(NeuronId(2001)));
        assert!(!m.is_inhibitory(NeuronId(9999)));
    }

    #[test]
    fn summary_reports_counts() {
        let m = meta();
        let s = m.summary();
        assert_eq!(s["present"], true);
        assert_eq!(s["neurons"], 2);
        assert_eq!(s["entry_set_len"], 5);
        assert_eq!(s["inhibitory_neurons"], 1);
        assert_eq!(s["source"], "test");
    }
}
```

Add to `crates/fluctlightdb/src/lib.rs` after line 11:

```rust
pub mod connectome;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p fluctlightdb --lib connectome::`
Expected: compile error — `ConnectomeMeta`, `Nt`, `weight_for`, `fnv1a64` not found.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `connectome.rs`:

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::id::NeuronId;

/// Dominant neurotransmitter of a presynaptic neuron (FlyWire `nt_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Nt {
    Ach,
    Gaba,
    Glut,
    Da,
    Ser,
    Oct,
    #[default]
    Unknown,
}

impl Nt {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "ACH" => Nt::Ach,
            "GABA" => Nt::Gaba,
            "GLUT" => Nt::Glut,
            "DA" => Nt::Da,
            "SER" => Nt::Ser,
            "OCT" => Nt::Oct,
            _ => Nt::Unknown,
        }
    }

    /// GABA and glutamate are inhibitory in the fly central brain (Liu & Wilson 2013).
    pub fn sign(self) -> f32 {
        match self {
            Nt::Gaba | Nt::Glut => -1.0,
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NeuronMeta {
    pub nt: Nt,
    pub super_class: String,
    pub class: String,
    pub side: String,
    /// Neuropil of the neuron's strongest presynaptic site set (first seen at import).
    pub neuropil: String,
}

/// Provenance + lookup tables for an imported connectome. Persisted as segment `connectome`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConnectomeMeta {
    pub source: String,
    pub imported_at: u64,
    pub p99_syn: f32,
    pub fanout: u8,
    pub entry_class: String,
    /// Sorted neuron ids of `entry_class` (Kenyon cells) — the cue entry layer.
    pub entry_set: Vec<NeuronId>,
    pub neurons: HashMap<NeuronId, NeuronMeta>,
}

impl ConnectomeMeta {
    /// Token → Kenyon-cell random sparse projection (antennal lobe → mushroom body analogue).
    /// Deterministic, codec-independent, `fanout` cells per token, duplicates allowed.
    pub fn project_tokens(&self, tokens: &[String]) -> Vec<NeuronId> {
        if self.entry_set.is_empty() {
            return Vec::new();
        }
        let n = self.entry_set.len() as u64;
        let mut out = Vec::with_capacity(tokens.len() * self.fanout as usize);
        for t in tokens {
            for k in 0..self.fanout {
                out.push(self.entry_set[(fnv1a64(t, k) % n) as usize]);
            }
        }
        out
    }

    pub fn is_inhibitory(&self, pre: NeuronId) -> bool {
        self.neurons.get(&pre).map(|m| m.nt.sign() < 0.0).unwrap_or(false)
    }

    pub fn summary(&self) -> serde_json::Value {
        let inhibitory = self.neurons.values().filter(|m| m.nt.sign() < 0.0).count();
        serde_json::json!({
            "present": true,
            "source": self.source,
            "imported_at": self.imported_at,
            "p99_syn": self.p99_syn,
            "fanout": self.fanout,
            "entry_class": self.entry_class,
            "entry_set_len": self.entry_set.len(),
            "neurons": self.neurons.len(),
            "inhibitory_neurons": inhibitory,
        })
    }
}

/// Synapse weight from summed synapse count: `sign × min(1, ln(1+Σ) / ln(1+p99))`.
/// p99 rather than max keeps the median fly edge near 0.5 so activation survives 3 hops
/// (spec D4) and ≤1 % of edges sit at the 1.0 clamp (`graph.rs:203`).
pub fn weight_for(sum_syn: u32, p99_syn: f32, inhibitory: bool) -> f32 {
    let denom = (1.0 + p99_syn.max(1.0)).ln();
    let w = ((1.0 + sum_syn as f32).ln() / denom).min(1.0);
    if inhibitory {
        -w
    } else {
        w
    }
}

/// FNV-1a 64 over `token` bytes then `k`. Plain hash on purpose: must not depend on
/// `neuron_codec` or `life_id`, so the projection is stable across brains and codecs.
pub fn fnv1a64(token: &str, k: u8) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for b in token.bytes().chain(std::iter::once(k)) {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p fluctlightdb --lib connectome::`
Expected: `8 passed`.

- [ ] **Step 5: Clippy + commit**

Run: `cargo clippy -p fluctlightdb 2>&1 | grep -c "src/connectome.rs"` — expected `0`.

```bash
git add crates/fluctlightdb/src/connectome.rs crates/fluctlightdb/src/lib.rs
git commit -m "feat(connectome): metadata, token->Kenyon projection, weight formula"
```

---

### Task 2: `brain.connectome` field + segment persistence

**Files:**
- Modify: `crates/fluctlightdb/src/brain.rs:77` (field), `:186` and `:2032` (struct literals `global_workspace: GlobalWorkspace::default(),`)
- Modify: `crates/fluctlightdb/src/manifest.rs:54` (segment list), `:198` (write), `:310` (read)
- Create: `crates/fluctlightdb/tests/connectome_import.rs`

**Interfaces:**
- Consumes: `ConnectomeMeta` (Task 1); `segment::write_segment` / `read_segment` (`segment.rs:51,70`).
- Produces: `FluctlightBrain.connectome: Option<ConnectomeMeta>` persisted as segment `connectome`.

- [ ] **Step 1: Write the failing test**

Create `crates/fluctlightdb/tests/connectome_import.rs`:

```rust
//! Connectome segment persistence, projection wiring, and CSV import.

use fluctlightdb::connectome::{ConnectomeMeta, NeuronMeta, Nt};
use fluctlightdb::id::NeuronId;
use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::FluctlightBrain;
use tempfile::tempdir;

fn v4_env() -> EnvGuard {
    let g = EnvGuard::acquire(&["FLUCTLIGHT_STORAGE", "FLUCTLIGHT_SOMNUS"]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");
    g
}

fn tiny_meta() -> ConnectomeMeta {
    let mut m = ConnectomeMeta {
        source: "unit".into(),
        imported_at: 42,
        p99_syn: 10.0,
        fanout: 3,
        entry_class: "Kenyon_Cell".into(),
        entry_set: vec![NeuronId(1001), NeuronId(1002)],
        neurons: Default::default(),
    };
    m.neurons.insert(
        NeuronId(4001),
        NeuronMeta { nt: Nt::Gaba, super_class: "central".into(), class: "APL".into(), side: "right".into(), neuropil: "MB".into() },
    );
    m
}

#[test]
fn connectome_segment_round_trips_and_is_absent_by_default() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    {
        let brain = FluctlightBrain::open(&path).unwrap();
        assert!(brain.connectome.is_none(), "fresh brain must have no connectome");
    }
    {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        brain.connectome = Some(tiny_meta());
        brain.checkpoint().unwrap();
    }
    let brain = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain.connectome, Some(tiny_meta()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fluctlightdb --test connectome_import connectome_segment_round_trips`
Expected: compile error `no field connectome on type FluctlightBrain`.

- [ ] **Step 3: Add the field and persistence**

`brain.rs` — after line 77 (`pub global_workspace: GlobalWorkspace,`):

```rust
    /// Imported biological connectome metadata (FlyWire). `None` for ordinary brains.
    /// Segment `connectome`, additive — never bumps `format_version`.
    pub connectome: Option<crate::connectome::ConnectomeMeta>,
```

`brain.rs` — in **both** struct literals (line 186 in `new()` and line ~2032 in `from_snapshot`), directly after `global_workspace: GlobalWorkspace::default(),`:

```rust
            connectome: None,
```

`manifest.rs` — after line 54 (`"global_workspace".into(),`):

```rust
                "connectome".into(),
```

`manifest.rs` — after line 198 (`segment::write_segment(dir, "global_workspace", &brain.global_workspace)?;`):

```rust
    segment::write_segment(dir, "connectome", &brain.connectome)?;
```

`manifest.rs` — after line 310-311 (`brain.global_workspace = segment::read_segment(dir, "global_workspace").unwrap_or_default();`):

```rust
    brain.connectome = segment::read_segment(dir, "connectome").unwrap_or_default();
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p fluctlightdb --test connectome_import && cargo test -p fluctlightdb --test crash_recovery --test somnus_durability`
Expected: all pass (the two existing suites prove old brains without the segment still open).

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/brain.rs crates/fluctlightdb/src/manifest.rs crates/fluctlightdb/tests/connectome_import.rs
git commit -m "feat(brain): connectome field persisted as additive segment"
```

---

### Task 3: `BrainGraph::add_synapse_uncapped`

**Files:**
- Modify: `crates/fluctlightdb/src/graph.rs` (after `add_synapse`, which ends at line 125)
- Test: `crates/fluctlightdb/src/graph.rs` `mod tests`

**Interfaces:**
- Produces: `pub fn add_synapse_uncapped(&mut self, synapse: Synapse)` — dedup on (from,to) keeping max weight, **never** runs synaptic competition. Importer-only.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `graph.rs`:

```rust
    #[test]
    fn add_synapse_uncapped_ignores_out_degree_cap() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        let mut g = BrainGraph::default();
        g.rebuild_index();
        let hub = NeuronId(1);
        for i in 0..1000u64 {
            g.add_synapse_uncapped(Synapse::new(hub, NeuronId(10_000 + i), Region::Cortex, 0.5));
        }
        assert_eq!(g.synapse_count(), 1000, "default cap is 256; uncapped must keep all");
        assert_eq!(g.neighbors(hub).count(), 1000);
        // dedup still applies, keeping the stronger weight
        g.add_synapse_uncapped(Synapse::new(hub, NeuronId(10_000), Region::Cortex, 0.9));
        assert_eq!(g.synapse_count(), 1000);
        let w = g.neighbors(hub).find(|(_, to)| *to == NeuronId(10_000)).map(|(s, _)| s.weight).unwrap();
        assert_eq!(w, 0.9);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fluctlightdb --lib graph::tests::add_synapse_uncapped_ignores_out_degree_cap`
Expected: compile error `no method named add_synapse_uncapped`.

- [ ] **Step 3: Implement**

Insert after `add_synapse` (after graph.rs line 125):

```rust
    /// Like [`add_synapse`](Self::add_synapse) but **never** runs synaptic competition.
    /// Only for bulk-importing a biological connectome whose degree distribution is the
    /// ground truth (FlyWire hubs reach 12,896 out-edges). Normal writes stay capped.
    pub fn add_synapse_uncapped(&mut self, synapse: Synapse) {
        self.register_neuron(synapse.from, synapse.region);
        self.register_neuron(synapse.to, synapse.region);
        let key = (synapse.from.0, synapse.to.0);
        if let Some(&idx) = self.synapse_index.get(&key) {
            if self.synapses[idx].weight.abs() < synapse.weight.abs() {
                self.synapses[idx].weight = synapse.weight;
            }
            return;
        }
        let idx = self.synapses.len();
        self.synapses.push(synapse);
        self.synapse_index.insert(key, idx);
        if self.adjacency_ready {
            self.adjacency.entry(key.0).or_default().push(idx as u32);
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fluctlightdb --lib graph::tests::`
Expected: all graph tests pass, including the new one.

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/graph.rs
git commit -m "feat(graph): add_synapse_uncapped for connectome bulk import"
```

---

### Task 4: Plasticity guards — inhibitory weights are read-only

**Files:**
- Modify: `crates/fluctlightdb/src/graph.rs:162-182` (`co_activate`, both branches), `prune_below`, `weaken_unused`
- Modify: `crates/fluctlightdb/src/calcium.rs:195` (`tick`)
- Test: `mod tests` in each file

**Interfaces:** none new. Invariant: any synapse with `weight < 0.0` is untouched by `co_activate`, `weaken_unused`, `prune_below`, and `CalciumSynapse::tick`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `graph.rs`:

```rust
    #[test]
    fn plasticity_never_touches_inhibitory_synapses() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        use std::collections::HashSet;
        let mut g = BrainGraph::default();
        g.rebuild_index();
        let (a, b, c) = (NeuronId(1), NeuronId(2), NeuronId(3));
        g.add_synapse_uncapped(Synapse::new(a, b, Region::Cortex, -0.4)); // inhibitory
        g.add_synapse_uncapped(Synapse::new(a, c, Region::Cortex, 0.4));  // excitatory
        let weight = |g: &BrainGraph, to: NeuronId| g.neighbors(a).find(|(_, t)| *t == to).map(|(s, _)| s.weight).unwrap();

        let active: HashSet<NeuronId> = [a, b, c].into_iter().collect();
        g.co_activate(&active, 1.0);
        assert_eq!(weight(&g, b), -0.4, "co_activate must skip negative weights");
        assert!(weight(&g, c) > 0.4, "excitatory still strengthens");

        g.weaken_unused(&HashSet::new(), 0.1);
        assert_eq!(weight(&g, b), -0.4, "weaken_unused must skip negative weights");

        let pruned = g.prune_below(0.3);
        assert_eq!(pruned, 0);
        assert_eq!(g.synapse_count(), 2, "prune_below must not treat negative as weak");
    }
```

Append inside `mod tests` in `calcium.rs` (create the module if absent, mirroring the file's existing test style):

```rust
    #[test]
    fn calcium_tick_leaves_inhibitory_weight_unchanged() {
        let mut syn = CalciumSynapse::default();
        let mut w = -0.4f32;
        // Drive many ticks with strong input so an excitatory weight would move.
        for _ in 0..50 {
            syn.on_pre_spike();
            syn.on_post_spike();
            let dw = syn.tick(&mut w, 1.0);
            assert_eq!(dw, 0.0);
        }
        assert_eq!(w, -0.4);
    }
```

(If `CalciumSynapse`'s constructor or spike methods are named differently in `calcium.rs`, use the names that file exposes; the assertion is what matters: `tick` returns `0.0` and does not modify a negative `weight`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p fluctlightdb --lib plasticity_never_touches_inhibitory_synapses calcium_tick_leaves_inhibitory`
Expected: FAIL — weights change / synapse pruned / `w` clamped to `0.001`.

- [ ] **Step 3: Add the four guards**

`graph.rs` `co_activate` — indexed branch (line ~172):

```rust
            for i in touched {
                if self.synapses[i as usize].weight < 0.0 {
                    continue; // inhibitory synapses are read-only (spec §5.4)
                }
                hebbian_strengthen(&mut self.synapses[i as usize], gate, 0.05);
            }
```

`graph.rs` `co_activate` — fallback branch (line ~177):

```rust
            for synapse in &mut self.synapses {
                if synapse.weight < 0.0 {
                    continue;
                }
                if active.contains(&synapse.from) && active.contains(&synapse.to) {
                    hebbian_strengthen(synapse, gate, 0.05);
                }
            }
```

`graph.rs` `prune_below`:

```rust
        self.synapses.retain(|s| s.weight < 0.0 || s.weight >= threshold);
```

`graph.rs` `weaken_unused`:

```rust
        for synapse in &mut self.synapses {
            if synapse.weight < 0.0 {
                continue;
            }
            if !active.contains(&synapse.from) && !active.contains(&synapse.to) {
                ltd_weaken(synapse, delta);
            }
        }
```

`calcium.rs` `tick` — first line of the body (line 196):

```rust
        if *weight < 0.0 {
            return 0.0; // inhibitory synapses are frozen in v1 (spec §5.4)
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p fluctlightdb --lib`
Expected: all pass (298 existing + new).

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/graph.rs crates/fluctlightdb/src/calcium.rs
git commit -m "feat(plasticity): freeze inhibitory (negative) synapses at all four update sites"
```

---

### Task 5: Sign-aware spread + `extra_seeds` + `connectome_seeds`

**Files:**
- Modify: `crates/fluctlightdb/src/activation.rs:84-96` (signature), `:145-153` (spread loop), `:64` and `:315` (callers)
- Modify: `crates/fluctlightdb/src/brain.rs:875` (caller)
- Modify: `crates/fluctlightdb/src/types.rs:150-156` (`ActivationResult`)
- Test: `mod tests` in `activation.rs`

**Interfaces:**
- Produces: `pub fn spread(activation: &mut HashMap<NeuronId, f32>, graph: &BrainGraph, max_hops: u32, spread_factor: f32)`; `activate_from_hybrid(..., codec: u8, extra_seeds: &[NeuronId])` (12th param); `ActivationResult.connectome_seeds: Option<usize>`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `activation.rs`:

```rust
    /// Reference implementation of the pre-plan loop (activation.rs:145-153 before this task).
    fn legacy_spread(activation: &mut HashMap<NeuronId, f32>, graph: &BrainGraph, max_hops: u32, spread_factor: f32) {
        for _hop in 0..max_hops {
            let current: Vec<(NeuronId, f32)> = activation.iter().map(|(k, v)| (*k, *v)).collect();
            for (node, act) in current {
                for (synapse, to) in graph.neighbors(node) {
                    let delta = act * synapse.weight * spread_factor;
                    if delta > 0.001 {
                        *activation.entry(to).or_insert(0.0) += delta;
                    }
                }
            }
            activation.retain(|_, v| *v > 0.01);
        }
    }

    #[test]
    fn spread_is_identical_to_legacy_when_all_weights_positive() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        // Deterministic pseudo-random graph: 60 nodes, 300 edges, weights in (0,1].
        let mut g = BrainGraph::default();
        g.rebuild_index();
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
        for _ in 0..300 {
            let from = NeuronId(next() % 60);
            let to = NeuronId(next() % 60);
            let w = ((next() % 1000) as f32 + 1.0) / 1000.0;
            g.add_synapse_uncapped(Synapse::new(from, to, Region::Cortex, w));
        }
        let seeds: HashMap<NeuronId, f32> = (0..5u64).map(|i| (NeuronId(i), 1.0)).collect();
        let mut a = seeds.clone();
        let mut b = seeds;
        legacy_spread(&mut a, &g, 4, 0.6);
        spread(&mut b, &g, 4, 0.6);
        assert_eq!(a.len(), b.len());
        for (k, v) in &a {
            let bv = b.get(k).copied().unwrap_or(f32::NAN);
            assert!((v - bv).abs() < 1e-5, "{k:?}: legacy {v} vs new {bv}");
        }
    }

    #[test]
    fn spread_inhibition_suppresses_and_is_order_independent() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        let (a, i, b) = (NeuronId(1), NeuronId(2), NeuronId(3));
        let mut g = BrainGraph::default();
        g.rebuild_index();
        g.add_synapse_uncapped(Synapse::new(a, b, Region::Cortex, 1.0));
        g.add_synapse_uncapped(Synapse::new(i, b, Region::Cortex, -1.0));
        // Excitatory only: b lights up.
        let mut act: HashMap<NeuronId, f32> = [(a, 1.0)].into_iter().collect();
        spread(&mut act, &g, 1, 0.6);
        assert!((act[&b] - 0.6).abs() < 1e-6);
        // With the inhibitory seed equally active: net zero, b drops out — regardless of map order.
        for _ in 0..20 {
            let mut act: HashMap<NeuronId, f32> = [(a, 1.0), (i, 1.0)].into_iter().collect();
            spread(&mut act, &g, 1, 0.6);
            assert!(!act.contains_key(&b), "b must be fully suppressed");
        }
        // Partial inhibition: 1.0 - 0.5 => b = 0.3
        let mut act: HashMap<NeuronId, f32> = [(a, 1.0), (i, 0.5)].into_iter().collect();
        spread(&mut act, &g, 1, 0.6);
        assert!((act[&b] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn extra_seeds_enter_activation_at_full_strength() {
        use crate::hippocampus::Hippocampus;
        use crate::semantic::SemanticField;
        let g = BrainGraph::default();
        let h = Hippocampus::default();
        let seeds = [NeuronId(777), NeuronId(778)];
        let r = activate_from_hybrid("zz", None, &g, &h, &SemanticField::default(), Uuid::nil(), 0, 1.0, 8, None, crate::id::CURRENT_CODEC, &seeds);
        assert_eq!(r.connectome_seeds, Some(2));
        let r0 = activate_from_hybrid("zz", None, &g, &h, &SemanticField::default(), Uuid::nil(), 0, 1.0, 8, None, crate::id::CURRENT_CODEC, &[]);
        assert_eq!(r0.connectome_seeds, None);
        assert_eq!(r.active_neurons, r0.active_neurons + 2);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p fluctlightdb --lib activation::tests::`
Expected: compile errors — `spread` not found, `activate_from_hybrid` takes 11 args, no field `connectome_seeds`.

- [ ] **Step 3: Implement**

`types.rs` — inside `ActivationResult` after `pub myelinated: bool,`:

```rust
    /// Number of Kenyon-cell seeds injected by the connectome projection. `None` when the
    /// brain has no connectome (ordinary brains never see this field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectome_seeds: Option<usize>,
```

Then fix every struct-literal construction of `ActivationResult` in the crate to add `connectome_seeds: None,` (run `cargo build` — the compiler lists each site; there are constructions in `activation.rs` and in Cursor's `brain.rs` additions).

`activation.rs` — signature (line 84-96): add the final parameter

```rust
    codec: u8,
    extra_seeds: &[NeuronId],
) -> ActivationResult {
```

`activation.rs` — immediately before `let spread_factor = 0.6 * myelination.max(0.1);` (line 145):

```rust
    for n in extra_seeds {
        let e = activation.entry(*n).or_insert(0.0);
        *e = e.max(1.0);
    }
    let connectome_seeds = if extra_seeds.is_empty() { None } else { Some(extra_seeds.len()) };
```

`activation.rs` — replace the loop at lines 146-153 (`for _hop in 0..max_hops { ... activation.retain(|_, v| *v > 0.01); }`) with:

```rust
    spread(&mut activation, graph, max_hops, spread_factor);
```

`activation.rs` — where the function builds its `ActivationResult { recalls, active_neurons, hops, myelinated, ... }`, add `connectome_seeds,`.

`activation.rs` — add the helper as a free function (above `activate_from_hybrid`):

```rust
/// Spreading activation over the synapse graph. Additive per hop; a node's incoming deltas
/// are summed before being applied so that inhibitory (negative-weight) edges subtract
/// deterministically regardless of `HashMap` iteration order. For graphs with no negative
/// weights this is arithmetically identical to the previous inline loop.
pub fn spread(activation: &mut HashMap<NeuronId, f32>, graph: &BrainGraph, max_hops: u32, spread_factor: f32) {
    for _hop in 0..max_hops {
        let current: Vec<(NeuronId, f32)> = activation.iter().map(|(k, v)| (*k, *v)).collect();
        let mut incoming: HashMap<NeuronId, f32> = HashMap::new();
        for (node, act) in current {
            for (synapse, to) in graph.neighbors(node) {
                let delta = act * synapse.weight * spread_factor;
                if delta.abs() > 0.001 {
                    *incoming.entry(to).or_insert(0.0) += delta;
                }
            }
        }
        for (to, sum) in incoming {
            let e = activation.entry(to).or_insert(0.0);
            *e = (*e + sum).max(0.0);
        }
        activation.retain(|_, v| *v > 0.01);
    }
}
```

Callers — append `&[]` as the last argument at `activation.rs:64` (inside `activate_from`) and `activation.rs:315` (test); at `brain.rs:875` append `&[]` **for now** (Task 6 replaces it with the projection).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p fluctlightdb --lib && cargo test -p fluctlightdb --test homeostasis_organ --test serve_integration --test cab_e2e_scenarios`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/activation.rs crates/fluctlightdb/src/types.rs crates/fluctlightdb/src/brain.rs
git commit -m "feat(activation): sign-aware spread helper, extra_seeds, connectome_seeds"
```

---

### Task 6: Wire the token→Kenyon projection into `FluctlightBrain::activate`

**Files:**
- Modify: `crates/fluctlightdb/src/brain.rs:875` (the `activate_from_hybrid(` call inside `activate_scoped`/`activate`)
- Test: `crates/fluctlightdb/tests/connectome_import.rs`

**Interfaces:**
- Consumes: `ConnectomeMeta::project_tokens`, `crate::tokenize::tokenize(text) -> Vec<String>` (`tokenize.rs:73`).
- Produces: `activate()` on a brain with a connectome returns `connectome_seeds == Some(tokens × fanout)`.

- [ ] **Step 1: Write the failing test**

Append to `tests/connectome_import.rs`:

```rust
#[test]
fn activate_projects_tokens_onto_entry_set_only_when_connectome_present() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    assert_eq!(brain.activate("hello world").connectome_seeds, None);

    brain.connectome = Some(tiny_meta()); // fanout 3
    let r = brain.activate("hello world");
    assert_eq!(r.connectome_seeds, Some(2 * 3), "2 tokens × fanout 3");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fluctlightdb --test connectome_import activate_projects_tokens`
Expected: FAIL — `assertion left: None, right: Some(6)`.

- [ ] **Step 3: Implement**

`brain.rs` — just before the `let mut result = activate_from_hybrid(` at line 875:

```rust
        let projected: Vec<crate::id::NeuronId> = match &self.connectome {
            Some(c) => c.project_tokens(&crate::tokenize::tokenize(cue)),
            None => Vec::new(),
        };
```

and replace the trailing `&[]` argument (added in Task 5) with `&projected`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p fluctlightdb --test connectome_import && cargo test -p fluctlightdb --lib`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/brain.rs crates/fluctlightdb/tests/connectome_import.rs
git commit -m "feat(brain): project cue tokens onto connectome entry set in activate()"
```

---

### Task 7: `import_connectome.rs` + fixtures + integration tests

**Files:**
- Create: `crates/fluctlightdb/src/import_connectome.rs`
- Create: `crates/fluctlightdb/tests/fixtures/connectome/connections.csv`, `neurons.csv`, `classification.csv`
- Modify: `crates/fluctlightdb/src/lib.rs` (`pub mod import_connectome;` and `pub use import_connectome::{import_connectome, ImportConfig, ImportReport};` next to line 134 `pub use error::{Error, Result};`)
- Test: `crates/fluctlightdb/tests/connectome_import.rs`

**Interfaces:**
- Consumes: Tasks 1–3 (`ConnectomeMeta`, `weight_for`, `Nt::parse`, `add_synapse_uncapped`), `crate::{Error, Result}`.
- Produces: `ImportConfig { connections, classification, neurons: PathBuf, entry_class: String, fanout: u8, replace: bool }`, `ImportReport { rows_read, rows_malformed, pairs, neurons, inhibitory_synapses, entry_set_len, p99_syn, import_s, index_s, warnings: Vec<String> }`, `pub fn import_connectome(brain: &mut FluctlightBrain, cfg: &ImportConfig) -> Result<ImportReport>`.

- [ ] **Step 1: Create the fixture (20 neurons, 40 connection rows)**

`tests/fixtures/connectome/neurons.csv`:

```csv
root_id,group,nt_type,nt_type_score,da_avg,ser_avg,gaba_avg,glut_avg,ach_avg,oct_avg
1001,MB,ACH,0.9,0,0,0,0,0.9,0
1002,MB,ACH,0.9,0,0,0,0,0.9,0
1003,MB,ACH,0.9,0,0,0,0,0.9,0
1004,MB,ACH,0.9,0,0,0,0,0.9,0
1005,MB,ACH,0.9,0,0,0,0,0.9,0
2001,AL,ACH,0.8,0,0,0,0,0.8,0
2002,AL,ACH,0.8,0,0,0,0,0.8,0
2003,AL,ACH,0.8,0,0,0,0,0.8,0
2004,AL,ACH,0.8,0,0,0,0,0.8,0
2005,AL,ACH,0.8,0,0,0,0,0.8,0
3001,MBON,ACH,0.7,0,0,0,0,0.7,0
3002,MBON,GLUT,0.7,0,0,0,0.7,0,0
3003,MBON,ACH,0.7,0,0,0,0,0.7,0
3004,MBON,ACH,0.7,0,0,0,0,0.7,0
3005,MBON,ACH,0.7,0,0,0,0,0.7,0
4001,APL,GABA,0.95,0,0,0.95,0,0,0
5001,MISC,DA,0.6,0.6,0,0,0,0,0
5002,MISC,SER,0.6,0,0.6,0,0,0,0
5003,MISC,OCT,0.6,0,0,0,0,0,0.6
5004,MISC,,0,0,0,0,0,0,0
```

`tests/fixtures/connectome/classification.csv`:

```csv
root_id,flow,super_class,class,sub_class,hemilineage,side,nerve
1001,intrinsic,central,Kenyon_Cell,KCg,MBp1,right,
1002,intrinsic,central,Kenyon_Cell,KCg,MBp1,right,
1003,intrinsic,central,Kenyon_Cell,KCab,MBp2,left,
1004,intrinsic,central,Kenyon_Cell,KCab,MBp2,left,
1005,intrinsic,central,Kenyon_Cell,KCapbp,MBp3,right,
2001,intrinsic,central,ALPN,,ALad1,right,
2002,intrinsic,central,ALPN,,ALad1,right,
2003,intrinsic,central,ALPN,,ALad1,left,
2004,intrinsic,central,ALPN,,ALad1,left,
2005,intrinsic,central,ALPN,,ALad1,right,
3001,intrinsic,central,MBON,,,right,
3002,intrinsic,central,MBON,,,right,
3003,intrinsic,central,MBON,,,left,
3004,intrinsic,central,MBON,,,left,
3005,intrinsic,central,MBON,,,right,
4001,intrinsic,central,APL,,,right,
5001,intrinsic,central,DAN,,,right,
5002,afferent,sensory,olfactory,,,right,
5003,efferent,descending,DN,,,left,
5004,intrinsic,optic,optic_lobe_intrinsic,,,left,
```

`tests/fixtures/connectome/connections.csv` — note rows 1–2 are the **same pair in two neuropils** (aggregation test) and row 21 carries a malformed `syn_count`:

```csv
pre_root_id,post_root_id,neuropil,syn_count,nt_type
2001,1001,AL_R,6,ACH
2001,1001,MB_CA_R,4,ACH
2001,1002,MB_CA_R,3,ACH
2002,1002,MB_CA_R,9,ACH
2002,1003,MB_CA_L,2,ACH
2003,1003,MB_CA_L,12,ACH
2003,1004,MB_CA_L,7,ACH
2004,1004,MB_CA_L,5,ACH
2004,1005,MB_CA_R,8,ACH
2005,1005,MB_CA_R,30,ACH
2005,1001,MB_CA_R,1,ACH
1001,3001,MB_ML_R,15,ACH
1002,3001,MB_ML_R,11,ACH
1003,3002,MB_ML_L,20,ACH
1004,3002,MB_ML_L,3,ACH
1005,3003,MB_ML_R,9,ACH
1001,3004,MB_VL_R,6,ACH
1002,3005,MB_VL_R,4,ACH
1003,3005,MB_VL_L,2,ACH
1004,3003,MB_VL_L,1,ACH
1005,3004,MB_VL_R,notanumber,ACH
1001,4001,MB_CA_R,25,ACH
1002,4001,MB_CA_R,18,ACH
1003,4001,MB_CA_L,22,ACH
1004,4001,MB_CA_L,14,ACH
1005,4001,MB_CA_R,19,ACH
4001,1001,MB_CA_R,40,GABA
4001,1002,MB_CA_R,35,GABA
4001,1003,MB_CA_L,38,GABA
4001,1004,MB_CA_L,33,GABA
4001,1005,MB_CA_R,41,GABA
3002,3001,MB_ML_R,5,GLUT
3002,3003,MB_ML_R,6,GLUT
5001,1001,MB_CA_R,10,DA
5001,1003,MB_CA_L,8,DA
5002,2001,AL_R,13,SER
5003,3001,MB_ML_R,2,OCT
5004,5003,LO_L,1,
1001,1002,MB_PED_R,2,ACH
3001,5003,SMP_R,3,ACH
```

- [ ] **Step 2: Write the failing tests**

Append to `tests/connectome_import.rs`:

```rust
use fluctlightdb::{import_connectome, ImportConfig};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/connectome").join(name)
}

fn fixture_cfg(replace: bool) -> ImportConfig {
    ImportConfig {
        connections: fixture("connections.csv"),
        classification: fixture("classification.csv"),
        neurons: fixture("neurons.csv"),
        entry_class: "Kenyon_Cell".into(),
        fanout: 7,
        replace,
    }
}

#[test]
fn import_fixture_aggregates_pairs_signs_edges_and_builds_entry_set() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    let report = import_connectome(&mut brain, &fixture_cfg(false)).unwrap();

    assert_eq!(report.rows_read, 40);
    assert_eq!(report.rows_malformed, 1, "one 'notanumber' row");
    assert_eq!(report.pairs, 38, "39 good rows, one duplicate pair (2001->1001)");
    assert_eq!(brain.graph.synapse_count(), 38);
    assert_eq!(report.neurons, 20);
    assert_eq!(report.entry_set_len, 5);
    assert_eq!(report.inhibitory_synapses, 7, "5 GABA from 4001 + 2 GLUT from 3002");
    assert!(report.warnings.is_empty());

    let meta = brain.connectome.as_ref().expect("connectome set");
    assert_eq!(meta.entry_set, vec![NeuronId(1001), NeuronId(1002), NeuronId(1003), NeuronId(1004), NeuronId(1005)]);
    assert_eq!(meta.neurons[&NeuronId(4001)].nt, Nt::Gaba);
    assert_eq!(meta.neurons[&NeuronId(5004)].nt, Nt::Unknown);
    assert_eq!(meta.neurons[&NeuronId(1001)].class, "Kenyon_Cell");
    assert_eq!(meta.neurons[&NeuronId(1001)].side, "right");

    // Aggregated pair 2001->1001 has Σsyn = 10, which is > the single-row 2002->1002 (9).
    let w = |from: u64, to: u64| brain.graph.neighbors(NeuronId(from)).find(|(_, t)| *t == NeuronId(to)).map(|(s, _)| s.weight).unwrap();
    assert!(w(2001, 1001) > w(2002, 1002));
    assert!(w(4001, 1001) < 0.0, "GABA edge is negative");
    assert!(w(3002, 3001) < 0.0, "GLUT edge is negative");
    assert!(w(5001, 1001) > 0.0, "DA edge is positive");
    assert!(w(2005, 1005) <= 1.0);
}

#[test]
fn import_is_persisted_and_recall_reaches_fly_circuitry() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
        brain.checkpoint().unwrap();
    }
    let brain = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain.graph.synapse_count(), 38);
    let r = brain.activate("odor");
    assert_eq!(r.connectome_seeds, Some(7));
    // Seeds land on Kenyon cells, which fan out to MBONs / APL — more than the 7 seeds stay active.
    assert!(r.active_neurons > 7, "spread through fly edges expected, got {}", r.active_neurons);
}

#[test]
fn import_refuses_second_import_unless_replace() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
    let err = import_connectome(&mut brain, &fixture_cfg(false)).unwrap_err();
    assert!(err.to_string().contains("already"), "{err}");
    let report = import_connectome(&mut brain, &fixture_cfg(true)).unwrap();
    assert_eq!(report.pairs, 38);
    assert_eq!(brain.graph.synapse_count(), 38, "replace must not double the graph");
}

#[test]
fn import_warns_when_brain_already_holds_engrams() {
    use fluctlightdb::Episode;
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    brain.experience(Episode::new("the harbor was quiet", "ctx", 0.5)).unwrap();
    let report = import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("engram"));
}

#[test]
fn import_rejects_header_mismatch_and_malformed_majority() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad.csv");
    std::fs::write(&bad, "a,b,c,d,e\n1,2,x,3,ACH\n").unwrap();
    let mut cfg = fixture_cfg(false);
    cfg.connections = bad.clone();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    let err = import_connectome(&mut brain, &cfg).unwrap_err();
    assert!(err.to_string().contains("pre_root_id"), "must name expected columns: {err}");

    std::fs::write(&bad, "pre_root_id,post_root_id,neuropil,syn_count,nt_type\n1,2,x,bad,ACH\n1,3,x,bad,ACH\n1,4,x,5,ACH\n").unwrap();
    let err = import_connectome(&mut brain, &cfg).unwrap_err();
    assert!(err.to_string().contains("malformed"), "{err}");
    assert!(brain.connectome.is_none(), "failed import must leave no partial state");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p fluctlightdb --test connectome_import`
Expected: compile error — `import_connectome`, `ImportConfig` not found.

- [ ] **Step 4: Implement `import_connectome.rs`**

```rust
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
/// Abort when more than this share of connection rows is malformed.
const MAX_MALFORMED_PERMILLE: u64 = 10;

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
fn read_pair_sums(
    cfg: &ImportConfig,
    meta: &mut HashMap<NeuronId, NeuronMeta>,
) -> Result<(HashMap<(u64, u64), u32>, u64, u64)> {
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
```

`lib.rs` — add `pub mod import_connectome;` in the module list and, next to line 134:

```rust
pub use import_connectome::{import_connectome, ImportConfig, ImportReport};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p fluctlightdb --test connectome_import`
Expected: `7 passed` (2 from Tasks 2/6 + 5 new). If `import_fixture_aggregates_pairs…` reports `pairs` ≠ 38, recount: 40 rows − 1 malformed = 39 good rows, minus 1 duplicate pair = 38.

- [ ] **Step 6: Clippy + commit**

Run: `cargo clippy -p fluctlightdb 2>&1 | grep -c "import_connectome.rs"` — expected `0`.

```bash
git add crates/fluctlightdb/src/import_connectome.rs crates/fluctlightdb/src/lib.rs crates/fluctlightdb/tests/fixtures/connectome crates/fluctlightdb/tests/connectome_import.rs
git commit -m "feat(import): FlyWire connectome CSV importer with fixtures and integration tests"
```

---

### Task 8: CLI `import-connectome`

**Files:**
- Modify: `crates/fluctlight-cli/src/main.rs` — new block before `if args[1] == "migrate-v4"` (line 243); usage line after line 746.

**Interfaces:**
- Consumes: `fluctlightdb::{import_connectome, ImportConfig, FluctlightBrain}`, `parse_flag_path` (line 178).
- Produces: `fluctlight import-connectome --path DIR --connections F --classification F --neurons F [--entry-class C] [--fanout N] [--replace]` printing `ImportReport` JSON.

- [ ] **Step 1: Add a string-flag helper next to `parse_flag_path` (line 178)**

```rust
fn parse_flag_str(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
```

- [ ] **Step 2: Add the subcommand block (before `if args[1] == "migrate-v4"`)**

```rust
    if args[1] == "import-connectome" {
        let need = |flag: &str| {
            parse_flag_path(&args, flag).unwrap_or_else(|| {
                eprintln!("import-connectome: {flag} FILE is required");
                std::process::exit(2);
            })
        };
        let path = need("--path");
        let cfg = fluctlightdb::ImportConfig {
            connections: need("--connections"),
            classification: need("--classification"),
            neurons: need("--neurons"),
            entry_class: parse_flag_str(&args, "--entry-class").unwrap_or_else(|| "Kenyon_Cell".into()),
            fanout: parse_flag_str(&args, "--fanout").and_then(|v| v.parse().ok()).unwrap_or(7),
            replace: args.iter().any(|a| a == "--replace"),
        };
        let mut brain = FluctlightBrain::open(&path).expect("open brain (is it being served? see docs/runbooks/connectome-import.md)");
        match fluctlightdb::import_connectome(&mut brain, &cfg) {
            Ok(report) => {
                brain.checkpoint().expect("checkpoint after import");
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            }
            Err(e) => {
                eprintln!("import-connectome: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
```

- [ ] **Step 3: Add the usage line after the `migrate-v4` usage line (line 746)**

```rust
         fluctlight import-connectome --path DIR --connections F --classification F --neurons F [--entry-class Kenyon_Cell] [--fanout 7] [--replace]\n\
                                                      import a FlyWire Codex connectome as the substrate graph\n\
```

- [ ] **Step 4: Verify against the fixture**

Run:
```bash
cargo build -p fluctlight-cli
D=$(mktemp -d)
F=crates/fluctlightdb/tests/fixtures/connectome
FLUCTLIGHT_STORAGE=v4 ./target/debug/fluctlight import-connectome --path $D/brain --connections $F/connections.csv --classification $F/classification.csv --neurons $F/neurons.csv
```
Expected: JSON with `"pairs": 38`, `"entry_set_len": 5`, `"inhibitory_synapses": 7`. Re-running without `--replace` exits 1 with `already has a connectome`.

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlight-cli/src/main.rs
git commit -m "feat(cli): import-connectome subcommand"
```

---

### Task 9: `POST /api/v1/connectome`

**Files:**
- Modify: `crates/fluctlightdb/src/serve.rs` — add an arm to the API `match` next to `"/api/v1/preplay" | "/preplay"` (line 2555).

**Interfaces:**
- Consumes: `ConnectomeMeta::summary()`, `server.with_brain_read`, `require_role(auth, Role::Read)`.
- Produces: `{"present": false}` or the summary object.

- [ ] **Step 1: Add the route arm**

```rust
        "/api/v1/connectome" | "/connectome" => {
            require_role(auth, Role::Read)?;
            let summary = server.with_brain_read(tenant_id, |b| {
                Ok(b.connectome.as_ref().map(|c| c.summary()))
            })?;
            Ok(summary.unwrap_or_else(|| serde_json::json!({"present": false})))
        }
```

- [ ] **Step 2: Build and verify manually**

Run:
```bash
cargo build -p fluctlight-cli
D=$(mktemp -d); F=crates/fluctlightdb/tests/fixtures/connectome
FLUCTLIGHT_STORAGE=v4 ./target/debug/fluctlight import-connectome --path $D/brain --connections $F/connections.csv --classification $F/classification.csv --neurons $F/neurons.csv >/dev/null
FLUCTLIGHT_STORAGE=v4 ./target/debug/fluctlight serve --addr 127.0.0.1:8799 --path $D/brain &
sleep 2; curl -s -X POST http://127.0.0.1:8799/api/v1/connectome -H 'content-type: application/json' -d '{}'; kill %1
```
Expected: `{"present":true,"entry_set_len":5,"neurons":20,"inhibitory_neurons":2,...}`. (If the server requires an auth header in this configuration, add the same header `serve_integration.rs` uses for read calls.)

- [ ] **Step 3: Run the serve suite + commit**

Run: `cargo test -p fluctlightdb --test serve_integration` — expected pass.

```bash
git add crates/fluctlightdb/src/serve.rs
git commit -m "feat(serve): /api/v1/connectome summary route"
```

---

### Task 10: Full-v783 perf test (ignored, env-gated)

**Files:**
- Create: `crates/fluctlightdb/tests/connectome_perf.rs`

**Interfaces:** Consumes `import_connectome`, `ImportConfig`. Gated by `FLUCTLIGHT_CONNECTOME_DIR` (directory containing the three real CSVs — never in the repo).

- [ ] **Step 1: Write the test**

```rust
//! Whole-brain FlyWire v783 perf gate. Skipped unless FLUCTLIGHT_CONNECTOME_DIR points at a
//! directory holding connections.csv / neurons.csv / classification.csv (Codex v783, CC-BY-4.0):
//!   B=https://storage.googleapis.com/flywire-data/codex/data/fafb/783
//!   for f in connections neurons classification; do curl -sO $B/$f.csv.gz && gunzip -f $f.csv.gz; done
//! Run: FLUCTLIGHT_CONNECTOME_DIR=/path cargo test -p fluctlightdb --release --test connectome_perf -- --ignored --nocapture

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{import_connectome, FluctlightBrain, ImportConfig};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::tempdir;

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()))
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

#[test]
#[ignore]
fn whole_brain_v783_within_budget() {
    let Some(dir) = std::env::var_os("FLUCTLIGHT_CONNECTOME_DIR") else {
        eprintln!("FLUCTLIGHT_CONNECTOME_DIR unset — skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let _g = EnvGuard::acquire(&["FLUCTLIGHT_STORAGE", "FLUCTLIGHT_SOMNUS"]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    let tmp = tempdir().unwrap();
    let path = tmp.path().join("fly-783");
    let cfg = ImportConfig {
        connections: dir.join("connections.csv"),
        classification: dir.join("classification.csv"),
        neurons: dir.join("neurons.csv"),
        ..Default::default()
    };

    let rss0 = rss_mb();
    let t = Instant::now();
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let report = import_connectome(&mut brain, &cfg).unwrap();
    brain.checkpoint().unwrap();
    let total_s = t.elapsed().as_secs_f64();
    let peak_mb = rss_mb() - rss0;

    let t = Instant::now();
    let r = brain.activate("odor sugar reward");
    let recall_ms = t.elapsed().as_secs_f64() * 1000.0;

    eprintln!("{report:#?}\ntotal_s={total_s:.2} peak_mb={peak_mb} recall_ms={recall_ms:.2} active={} seeds={:?}", r.active_neurons, r.connectome_seeds);
    assert_eq!(report.entry_set_len, 5177, "Kenyon cells in v783");
    assert!(report.pairs > 2_600_000 && report.pairs < 2_800_000, "{}", report.pairs);
    assert!(total_s <= 20.0, "import+checkpoint {total_s:.1}s > 20s");
    assert!(peak_mb <= 1024, "peak RSS {peak_mb} MB > 1 GB");
    assert!(recall_ms <= 20.0, "4-hop recall {recall_ms:.1} ms > 20 ms");
    assert_eq!(r.connectome_seeds, Some(3 * 7));
}
```

- [ ] **Step 2: Run it skipped, then for real**

Run: `cargo test -p fluctlightdb --test connectome_perf -- --ignored` — expected: prints "skipping", passes.
Run with the scratch CSVs from the spike: `FLUCTLIGHT_CONNECTOME_DIR=/tmp/claude-1000/-home-voxmastery-FluctlightDB/3299c79e-aebe-4c4c-943e-d83abbb640dd/scratchpad/fly783 cargo test -p fluctlightdb --release --test connectome_perf -- --ignored --nocapture`
Expected: passes; numbers in the same range as the spike (≈10 s, ≈500 MB, < 10 ms).

- [ ] **Step 3: Commit**

```bash
git add crates/fluctlightdb/tests/connectome_perf.rs
git commit -m "test(perf): env-gated whole-brain FlyWire v783 budget test"
```

---

### Task 11: Docs — CHANGELOG + runbook

**Files:**
- Modify: `CHANGELOG.md` (under `## [Unreleased]`, before `### Changed`)
- Create: `docs/runbooks/connectome-import.md`

- [ ] **Step 1: CHANGELOG**

Insert under `## [Unreleased]`:

```markdown
### Added

- **FlyWire connectome import.** `fluctlight import-connectome` ingests the FlyWire FAFB v783
  whole-brain connectome (139k neurons, 2.7M connections) as the substrate graph. Metadata lives
  in a new additive `connectome` segment — no `format_version` change; brains without it are
  unaffected. GABA/glutamate edges are imported with negative weights, honoured by a sign-aware
  spread, and frozen from plasticity. Cue tokens project onto Kenyon cells (fanout 7), so text
  recall runs through fly circuitry. `POST /api/v1/connectome` reports the summary.
  Spec: `docs/superpowers/specs/2026-09-17-flywire-connectome-import-design.md`.
```

- [ ] **Step 2: Runbook `docs/runbooks/connectome-import.md`**

```markdown
# Connectome import (FlyWire v783)

## Download (≈53 MB, CC-BY-4.0, no token)

    B=https://storage.googleapis.com/flywire-data/codex/data/fafb/783
    mkdir -p /var/lib/fluctlight/flywire-783 && cd $_
    for f in connections neurons classification; do curl -sO $B/$f.csv.gz && gunzip -f $f.csv.gz; done

Cite: Dorkenwald et al., *Nature* 2024, doi:10.5281/zenodo.10676866.

## Lock rule

`import-connectome` opens the brain with the **exclusive** store lock. It must not run against a
tenant that `fluctlight-serve` has open — stop serve first, or import into a fresh path and swap
(hermes-style-agent-upgrade.md §3). The CLI exits non-zero if the lock is held.

## Run

    export FLUCTLIGHT_STORAGE=v4
    fluctlight import-connectome --path ~/.fluctlight/tenants/fly-783/brain \
      --connections /var/lib/fluctlight/flywire-783/connections.csv \
      --classification /var/lib/fluctlight/flywire-783/classification.csv \
      --neurons /var/lib/fluctlight/flywire-783/neurons.csv

Expect ≈10 s, ≈500 MB RSS, ≈145 MB on disk, and a JSON report with `pairs ≈ 2,700,513`,
`entry_set_len = 5177`. The importer parses everything before touching the graph: a failed
import leaves the brain exactly as it was.

## Verify

    curl -s -X POST http://127.0.0.1:8792/api/v1/connectome -d '{}'   # {"present":true,...}
    curl -s -X POST http://127.0.0.1:8792/api/v1/activate -d '{"cue":"odor sugar"}' | jq .connectome_seeds   # 14

## Replace / roll back

`--replace` removes every synapse touching a neuron of the previous connectome, then imports.
To roll back entirely, restore the tenant from `~/.fluctlight/backups/` (backup-restore.md) —
there is no partial undo.
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md docs/runbooks/connectome-import.md
git commit -m "docs: connectome import changelog entry and runbook"
```

---

## Self-Review

**Spec coverage:** §3 inputs → Task 7 (headers, aggregation, malformed rule). D1 side segment → Task 2. D2 inhibition → Tasks 4, 5, 7. D3 projection → Tasks 1, 6. D4 weight → Task 1 `weight_for`. D5 uncapped → Task 3. D6 no flags / provenance → `ConnectomeMeta{source, imported_at,…}` in Tasks 1, 7. §5.5 CLI → Task 8, HTTP → Task 9. §7 errors → Task 7 (header, malformed, replace, warn, parse-before-mutate). §8 tests → unit (1, 3, 4, 5), property (5), integration (2, 6, 7), perf (10). §9 exit → Task 11.

**Placeholder scan:** none. Every code step is complete as written.

**Type consistency:** `ConnectomeMeta` fields are identical in Tasks 1, 2, 7. `project_tokens(&[String])` in Tasks 1 and 6 (`tokenize` returns `Vec<String>`). `activate_from_hybrid` gains exactly one trailing `extra_seeds: &[NeuronId]` in Task 5; Task 6 passes `&projected`. `ImportReport` field names match between Task 7's struct and Task 7/10 tests. `connectome_seeds: Option<usize>` in Tasks 5, 6, 7, 10.

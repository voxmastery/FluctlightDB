# FlyWire connectome import + cue fusion — design

**Status:** approved for planning · **Branch:** `feat/synthetic-mind-core` · **Date:** 2026-09-17

## 1. Goal

Ingest the FlyWire FAFB v783 whole-brain connectome (139,255 neurons, 2,700,513
unique directed connections) into a FluctlightDB tenant as the substrate graph,
and let ordinary text cues reach that circuitry, so FluctlightDB's plasticity
learns on real wiring. The connectome supplies the wiring; FluctlightDB supplies
memory, activation and learning.

Spike evidence (2026-09-17, this machine): full import + index + save + reopen in
9.7 s, 506 MB peak RSS, 145 MB on disk, 4-hop spread from 10 Kenyon cells in
7.7 ms. Feasibility is not in question.

## 2. Non-goals (v1)

- Neural dynamics simulation (spiking, membrane models). This is graph substrate + activation, nothing more.
- The 9.5 GB per-synapse-site file. Only the pair-aggregated Codex CSVs are used.
- Plasticity on inhibitory synapses. They are imported and honoured by spread but **frozen** (§5.4).
- Mapping the 79 fly neuropils onto the 7 mammalian `Region` variants. Neuropil is kept as metadata.
- Import over HTTP. Import is a CLI action against a tenant that is not being served (runbook: no second holder of the live store lock).

## 3. Inputs

Public bucket, CC-BY-4.0, no token: `https://storage.googleapis.com/flywire-data/codex/data/fafb/783/`

| file | rows | columns used |
|---|---|---|
| `connections.csv.gz` (50 MB) | 3,869,878 | `pre_root_id, post_root_id, neuropil, syn_count, nt_type` |
| `neurons.csv.gz` (1.7 MB) | 139,255 | `root_id, nt_type` |
| `classification.csv.gz` (0.9 MB) | 139,255 | `root_id, flow, super_class, class, sub_class, side` |

`connections.csv` is one row per **(pre, post, neuropil)**; the same pair appears
in up to 6 neuropils. The graph is pair-unique, so rows are aggregated per pair
(measured: 2,700,513 distinct pairs). Root IDs are u64 and map directly onto
`NeuronId(u64)`. Real data is never committed; tests use tiny fixtures (§8).

## 4. Decisions (first principles)

| # | decision | why |
|---|---|---|
| D1 | Fly metadata lives in a **new additive segment `connectome`**, all fly synapses use `Region::Cortex`. | Neuropil/nt/class describe neurons, not the graph's shape. `Region` is serde-by-name with no fallback and the manifest hard-checks `format_version`; a new variant is a one-way format break (hermes runbook). Additive segments read with `unwrap_or_default` are the established pattern (attention_schema, predictive_loop, …). Old binaries open the brain unchanged. |
| D2 | **Inhibition is real.** Pre-neuron `nt_type ∈ {GABA, GLUT}` ⇒ negative synapse weight; spread becomes sign-aware. | The mushroom body only does pattern separation because GABAergic feedback (APL) sparsifies Kenyon cells. Without inhibition the circuit is not the circuit. Existing brains have zero negative weights, so the sign-aware path is a proven no-op for them (§8, property test). |
| D3 | **Fusion = token→Kenyon random projection.** Each cue token deterministically projects onto `fanout` Kenyon cells. | This *is* the antennal-lobe→mushroom-body wiring: a random sparse projection, each Kenyon cell sampling ~7 inputs. Hashing tokens onto the entry set is the same construction. Gated by the presence of a connectome; no env flag. |
| D4 | Weight = `sign × min(1, ln(1+Σsyn) / ln(1+p99))`, p99 computed at import. | Σsyn per pair: median 8, p99 78, max 2,405. Normalising by max drops the median edge to 0.25 and activation dies by hop 2; by p99 the median edge is 0.50 and survives 3 hops at spread 0.55 / floor 0.02. `graph.rs:203` records 2.6M production synapses parked at the 1.0 clamp — p99 keeps ≤1 % of fly edges there. |
| D5 | Import bypasses synaptic competition via a new `BrainGraph::add_synapse_uncapped`. | `max_out_degree` (default 256) is an env-backed `OnceLock`; fly hubs reach 12,896 out-edges. Requiring `FLUCTLIGHT_MAX_OUT_DEGREE=0` at import (as the spike did) is a usability trap. The importer is the only caller; normal writes stay capped. |
| D6 | No new env flags. Provenance is durable state. | Spread sign-awareness and projection activate on `brain.connectome.is_some()`, not on env. `ConnectomeMeta{source, imported_at, p99_syn, fanout}` is the on-disk record of how the brain was built — the provenance stamp argued for on 2026-09-15. |

## 5. Components

### 5.1 `connectome.rs` (new, ~200 lines)

```rust
pub enum Nt { Ach, Gaba, Glut, Da, Ser, Oct, Unknown }   // sign(): Gaba|Glut => -1.0, else 1.0
pub struct NeuronMeta { nt: Nt, super_class: String, class: String, side: Side, neuropil: String }
pub struct ConnectomeMeta {
    source: String, imported_at: u64, p99_syn: f32, fanout: u8,
    entry_class: String,                       // "Kenyon_Cell"
    entry_set: Vec<NeuronId>,                  // sorted, 5,177 for v783
    neurons: HashMap<NeuronId, NeuronMeta>,
}
impl ConnectomeMeta {
    pub fn project_tokens(&self, tokens: &[&str]) -> Vec<NeuronId>;   // §5.3
    pub fn is_inhibitory(&self, pre: NeuronId) -> bool;
}
```

Persisted as segment `connectome` via `segment::write_segment` / `read_segment(..).unwrap_or_default()`,
listed in the manifest segment list exactly as the four cognitive segments are.
`FluctlightBrain` gains `pub connectome: Option<ConnectomeMeta>`.

### 5.2 `import_connectome.rs` (new, ~250 lines)

```rust
pub struct ImportConfig { connections: PathBuf, classification: PathBuf, neurons: PathBuf,
                          entry_class: String /* Kenyon_Cell */, fanout: u8 /* 7 */, replace: bool }
pub struct ImportReport { rows_read, rows_malformed, pairs, neurons, inhibitory_synapses,
                          entry_set_len, p99_syn, import_s, index_s }
pub fn import_connectome(brain: &mut FluctlightBrain, cfg: &ImportConfig) -> Result<ImportReport>;
```

Passes: (1) `neurons.csv` → `nt` per neuron; `classification.csv` → class/side, collect
`entry_set`. (2) `connections.csv` → `HashMap<(u64,u64), u32>` summing `syn_count` per pair
(~2.7 M entries, ~100 MB, dropped after pass 3). (3) compute p99; for each pair
`add_synapse_uncapped(Synapse::new(pre, post, Region::Cortex, weight))`. (4) `rebuild_index()`,
set `brain.connectome`, `checkpoint()`. Refuses (`Error::Store`) if a connectome is already present
and `!cfg.replace`. Warns (not error) if the brain already holds engrams — that is the fusion case.

### 5.3 Cue projection (edit: `dentate.rs` / `activation.rs`)

After `cue_to_dg_neurons` produces the hashed DG seeds, if `brain.connectome` is present:

```
for token t in tokens, for k in 0..fanout:
    seeds.push(entry_set[ fnv1a64(t, k) % entry_set.len() ])
```

Plain FNV-1a over `(token, k)`; **not** `NeuronId::from_seeds_with` (that yields DG-space ids and is
codec-dependent — the projection must be stable across codecs). Deterministic, in-range, ~7 lookups
per token. Brains without a connectome execute the identical pre-existing path.

### 5.4 Sign-aware spread (edit: `activation.rs`, `preplay.rs`)

Current: `v = a * w * spread; if v > floor { act[to] = max(act[to], v) }`.
New: `if w < 0 { act[to] = (act[to] - |v|).max(0) } else { unchanged }`; a node driven to 0 leaves
the frontier. Excitatory behaviour is bit-identical to today.

**Plasticity freeze** (three guard points, all `if synapse.weight < 0.0 { continue }`):
`graph.rs` `co_activate` (Hebbian, l.162–178) · `graph.rs` LTD/prune (l.238) · `calcium.rs` l.224
(which otherwise clamps to `[0.001, 1.0]` and would flip the sign). Inhibitory weights are read-only
in v1; lifting this is a follow-up spec.

### 5.5 CLI + HTTP

`fluctlight import-connectome --path DIR --connections F --classification F --neurons F
[--entry-class Kenyon_Cell] [--fanout 7] [--replace]` → prints `ImportReport` as JSON. Follows the
`migrate-v4` / `parse_flag_path` pattern in `fluctlight-cli/src/main.rs`.
`GET /api/v1/connectome` → `{source, imported_at, neurons, entry_set_len, inhibitory_synapses}` or 404.
No import endpoint (§2).

## 6. Data flow

```
CSVs ──import_connectome──▶ graph (2.7M synapses, Region::Cortex, signed weights)
                          └▶ segment "connectome" (meta, entry_set)          ── checkpoint ──▶ disk
activate(cue) ──cue_to_dg_neurons──▶ DG seeds ──+ project_tokens──▶ Kenyon seeds
              ──spread (sign-aware)──▶ ActivationResult (unchanged type)
sleep / co_activate ──plasticity──▶ excitatory synapses only
```

## 7. Error handling

- Header mismatch on any CSV → abort naming expected vs found columns.
- Malformed row → counted in `rows_malformed`, skipped; abort if > 1 % of rows.
- Existing connectome without `--replace` → abort. Existing engrams → warn, proceed.
- Serving tenant → the store lock refuses the open; error message points at the runbook.
- Partial failure after pass 3 → nothing checkpointed; reopening yields the pre-import brain.

## 8. Testing

- **Unit** (`connectome.rs`): `project_tokens` deterministic, in-range, independent of `neuron_codec`; `Nt::sign`; weight fn monotone in Σsyn and clamped at 1.0.
- **Property** (`activation.rs`): for any brain with `connectome == None`, `activate()` output is identical before and after this change (drives the "no-op for existing brains" claim in D2/D3).
- **Integration** (`tests/connectome_import.rs`, fixtures in `tests/fixtures/connectome/` — 20 neurons, 40 rows, committed): import → `activate("cue")` reaches at least one Kenyon-projected neuron; an inhibitory edge suppresses a target that an excitatory-only run activates; `co_activate` + `sleep` leave negative weights unchanged; save/reopen preserves `connectome` and synapse count; `--replace` semantics; malformed-row threshold.
- **Perf** (ignored, env-gated like `scale_bench`): full v783 from scratch — import ≤ 20 s, peak RSS ≤ 1 GB, 4-hop spread from 10 Kenyon cells ≤ 20 ms. Real data is downloaded into scratch by the test if absent, never committed.

## 9. Exit criteria

Ships when §8 is green, `cargo clippy` is clean on new files, CHANGELOG has an entry under
`[Unreleased]`, and `docs/runbooks/` gains a short `connectome-import.md` (download, lock, replace).
Projection / inhibition graduate from "present when data is present" to documented behaviour in
`README.md` once LongMemEval-S on a fused brain is measured and reported.

## 10. Risks

| risk | mitigation |
|---|---|
| Fly graph dominates recall on a fused brain (2.7 M fly edges vs thousands of engram edges) | Projection fan-out is small (7) and fly weights are p99-normalised; measure on LongMemEval before making projection default-on for text tenants. |
| Sign-aware spread regresses excitatory recall | Property test (§8) is the gate; the branch is a no-op when no negative weights exist. |
| Aggregation `HashMap` memory | ~100 MB for 2.7 M pairs; dropped before checkpoint. Spike peak was 506 MB total. |
| Someone commits the CSVs | `tests/fixtures/connectome/` holds only the 20-neuron fixture; perf test downloads to scratch. |

## 11. Corrections after implementation (2026-09-18)

Recorded so the spec matches the code. Each was ruled during execution; see the plan's SDD ledger.

- **§5.4 guard points are eight, not three.** Inhibitory (negative) weights are skipped at `graph.rs` `co_activate` (both branches), `weaken_unused`, `prune_below`, `homeostatic_downscale`, `stdp_sequential` (both branches), and `calcium.rs` `CalciumSpine::tick`. `prune_below` would otherwise delete every inhibitory synapse as "weak"; `homeostatic_downscale` would rewrite them to `0.001`.
- **§5.4 spread rule is additive, not `max`.** The pre-existing loop summed deltas; the sign-aware helper sums each hop's incoming deltas per target and applies `(*e + sum).max(0.0)`, so inhibition is iteration-order independent. Brains with no negative weight (`BrainGraph::has_inhibitory == false`) run the original loop verbatim, so the no-connectome path is bit-identical (tested with exact equality); only graphs carrying inhibition use the per-hop summed path.
- **§5.4 `preplay.rs` is deferred.** Its walk uses `max` accumulation with a `> 0.02` floor, so negative edges are simply never traversed (safe, not subtractive). Making a max-walk subtractive needs its own design; follow-up.
- **§7 malformed-row threshold is 10 %, not 1 %.** The 20-neuron fixture deliberately carries 1 malformed row of 40 (2.5 %); real FlyWire files carry none; a wrong file fails the header check. Rows below the threshold are still reported in `ImportReport.rows_malformed`.
- **§5.5 route is `POST /api/v1/connectome`** (path-dispatched like every other brain route) and answers `{"present": false}` rather than 404 when no connectome is present.
- **Activation cache.** `ActivationCache` keys on `(cue, agent_id, top_k)` only, so a brain carrying a connectome bypasses the cache (lookup and insert) in v1; the no-connectome path is unchanged. Follow-up: include a connectome fingerprint in the key.

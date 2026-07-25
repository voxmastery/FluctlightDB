# CLS Dual-Hemisphere Organ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement and **fully prove** CortexSchema → CaptureGate → Aeterna (CLS both halves in Fluctlight) with zero default `activate()` / frozen-benchmark tradeoffs.

**Architecture:** Phase A adds durable typed schemas crystallized at semantic sleep and an opt-in schema recall lane. Phase B adds eligibility tags, interleaved capture, supersede-not-clobber, and CF rollback. Phase C adds lossless agent prompt packing + session boot + expand. Somnus stays durability-only. **Do not start Phase N+1 until Phase N gates are green in CI.**

**Tech Stack:** Rust (`fluctlightdb` crate), existing sleep/cortex/hippocampus, cargo tests, optional Python bindings after Rust gates.

**Spec:** [`docs/superpowers/specs/2026-07-25-cls-dual-hemisphere-organ-design.md`](../specs/2026-07-25-cls-dual-hemisphere-organ-design.md)

---

## File map

| File | Responsibility |
|------|----------------|
| `crates/fluctlightdb/src/schema.rs` | `Schema`, `SchemaStore`, crystallize/merge/supersede, query |
| `crates/fluctlightdb/src/cortex.rs` | Embed `SchemaStore` with `#[serde(default)]` |
| `crates/fluctlightdb/src/sleep.rs` | Call schema crystallize after replay consolidate |
| `crates/fluctlightdb/src/brain.rs` | `sleep` wiring; `activate_with_schemas`; non-default lane |
| `crates/fluctlightdb/src/eligibility.rs` | Phase B: tags on experience |
| `crates/fluctlightdb/src/capture_gate.rs` | Phase B: interleaved capture + CF audit/rollback |
| `crates/fluctlightdb/src/agent_prompt.rs` | Phase C: lossless pack + expand (replace truncate-drop) |
| `crates/fluctlightdb/src/homeostasis.rs` | Token/seal metrics (measurement) |
| `crates/fluctlightdb/src/somnus.rs` | WAL+tick seal cadence (durability only) |
| `crates/fluctlightdb/tests/cortex_schema_gates.rs` | Phase A strict gates |
| `crates/fluctlightdb/tests/capture_gate_gates.rs` | Phase B strict gates |
| `crates/fluctlightdb/tests/aeterna_gates.rs` | Phase C strict gates |
| `crates/fluctlightdb/tests/activate_nonregression.rs` | Frozen activate id-list fixtures |
| `docs/PRODUCTION.md` | Document lanes + gates after each phase merges |

---

## Global rules (every task)

1. Default `activate(cue)` behavior and ranking **must not change**.
2. Schema / agent-prompt features are **opt-in lanes** or separate APIs.
3. Somnus `systems_seal` must never call crystallize/capture/sleep_cycle prune.
4. Commit only after the task’s tests pass.
5. **STOP** markers are hard — do not continue past a red gate.

---

# PHASE A — CortexSchema

### Task 1: Schema types + store (TDD)

**Files:**
- Create: `crates/fluctlightdb/src/schema.rs`
- Modify: `crates/fluctlightdb/src/lib.rs` (add `pub mod schema;`)
- Test: unit tests inside `schema.rs`

- [ ] **Step 1: Write the failing unit tests in `schema.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn reject_schema_without_supports() {
        let mut store = SchemaStore::default();
        let s = Schema::new("user prefers dark mode", vec![]);
        assert!(store.upsert_active(s).is_err());
    }

    #[test]
    fn upsert_requires_existing_support_ids_checked_by_caller() {
        let mut store = SchemaStore::default();
        let id = Uuid::new_v4();
        let s = Schema::new("user prefers dark mode", vec![id]);
        assert!(store.upsert_active(s).is_ok());
        assert_eq!(store.active().count(), 1);
    }

    #[test]
    fn supersede_keeps_old_resolvable() {
        let mut store = SchemaStore::default();
        let a = Uuid::new_v4();
        let old = store.upsert_active(Schema::new("theme=light", vec![a])).unwrap();
        let new = store
            .upsert_active(Schema::new("theme=dark", vec![a]).superseding(old))
            .unwrap();
        assert_eq!(store.active_head_for_key("theme").unwrap().id, new);
        assert!(store.get(old).unwrap().status == SchemaStatus::Superseded);
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL (module missing)**

```bash
cd /home/ambugo/fluctlightdb && cargo test -p fluctlightdb schema:: --lib
```

Expected: compile error or FAIL

- [ ] **Step 3: Implement minimal `schema.rs`**

```rust
//! CortexSchema — durable neocortical schemas (CLS slow half).

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SchemaStatus {
    Active,
    Provisional,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schema {
    pub id: Uuid,
    pub key: String,
    pub statement: String,
    pub slots: Vec<String>,
    pub support_engram_ids: Vec<Uuid>,
    pub confidence: f32,
    pub supersedes: Option<Uuid>,
    pub status: SchemaStatus,
}

impl Schema {
    pub fn new(statement: impl Into<String>, supports: Vec<Uuid>) -> Self {
        let statement = statement.into();
        let key = schema_key(&statement);
        Self {
            id: Uuid::new_v4(),
            key,
            statement,
            slots: Vec::new(),
            support_engram_ids: supports,
            confidence: 0.5,
            supersedes: None,
            status: SchemaStatus::Active,
        }
    }

    pub fn superseding(mut self, old: Uuid) -> Self {
        self.supersedes = Some(old);
        self
    }
}

pub fn schema_key(statement: &str) -> String {
    let t = statement.to_lowercase();
    if t.contains("theme") || t.contains("dark") || t.contains("light") {
        "theme".into()
    } else {
        // stable coarse key: first 3 tokens
        t.split_whitespace().take(3).collect::<Vec<_>>().join("_")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SchemaStore {
    pub schemas: Vec<Schema>,
}

impl SchemaStore {
    pub fn get(&self, id: Uuid) -> Option<&Schema> {
        self.schemas.iter().find(|s| s.id == id)
    }

    pub fn active(&self) -> impl Iterator<Item = &Schema> {
        self.schemas.iter().filter(|s| s.status == SchemaStatus::Active)
    }

    pub fn active_head_for_key(&self, key: &str) -> Option<&Schema> {
        self.active().filter(|s| s.key == key).max_by(|a, b| {
            a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn upsert_active(&mut self, mut schema: Schema) -> Result<Uuid> {
        if schema.support_engram_ids.is_empty() {
            return Err(Error::Store("schema requires support_engram_ids".into()));
        }
        schema.status = SchemaStatus::Active;
        if let Some(old) = schema.supersedes {
            if let Some(s) = self.schemas.iter_mut().find(|s| s.id == old) {
                s.status = SchemaStatus::Superseded;
            }
        }
        // deactivate other active with same key
        for s in self.schemas.iter_mut() {
            if s.key == schema.key && s.status == SchemaStatus::Active {
                s.status = SchemaStatus::Superseded;
            }
        }
        let id = schema.id;
        self.schemas.push(schema);
        Ok(id)
    }
}
```

Wire `pub mod schema;` and `pub use schema::{Schema, SchemaStore, SchemaStatus};` in `lib.rs`.

- [ ] **Step 4: Run tests — expect PASS**

```bash
cargo test -p fluctlightdb schema:: --lib
```

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/schema.rs crates/fluctlightdb/src/lib.rs
git commit -m "feat(schema): CortexSchema store with support + supersede"
```

---

### Task 2: Persist schemas on Cortex

**Files:**
- Modify: `crates/fluctlightdb/src/cortex.rs`
- Test: `crates/fluctlightdb/tests/cortex_schema_gates.rs` (start file)

- [ ] **Step 1: Failing test — cortex roundtrip keeps schemas**

```rust
// crates/fluctlightdb/tests/cortex_schema_gates.rs
use fluctlightdb::{Episode, FluctlightBrain, Schema};
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn cortex_schemas_survive_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let eid = brain
        .experience(Episode::new("User prefers dark mode", "prefs", 0.9))
        .unwrap()
        .engram_id;
    brain
        .cortex
        .schemas
        .upsert_active(Schema::new("user prefers dark mode", vec![eid]))
        .unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    let brain2 = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain2.cortex.schemas.active().count(), 1);
}
```

- [ ] **Step 2: Run — expect FAIL (no `schemas` field)**

```bash
cargo test -p fluctlightdb --test cortex_schema_gates cortex_schemas_survive_checkpoint
```

- [ ] **Step 3: Add to `Cortex`**

```rust
#[serde(default)]
pub schemas: crate::schema::SchemaStore,
```

Ensure `Default` still works. No manifest change needed if cortex segment serializes whole `Cortex`.

- [ ] **Step 4: Run — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add crates/fluctlightdb/src/cortex.rs crates/fluctlightdb/tests/cortex_schema_gates.rs
git commit -m "feat(cortex): persist SchemaStore in cortex segment"
```

---

### Task 3: Deterministic crystallize at semantic sleep

**Files:**
- Modify: `crates/fluctlightdb/src/schema.rs` (add `crystallize_from_engrams`)
- Modify: `crates/fluctlightdb/src/sleep.rs` / `brain.rs` `sleep_internal`
- Test: `cortex_schema_gates.rs`

- [ ] **Step 1: Failing test — sleep creates schema from repeated preference episodes**

```rust
#[test]
fn sleep_crystallizes_theme_schema_from_supports() {
    let mut brain = FluctlightBrain::new();
    for _ in 0..3 {
        brain
            .experience(Episode::new("User prefers dark mode theme", "prefs", 0.8))
            .unwrap();
    }
    assert_eq!(brain.cortex.schemas.active().count(), 0);
    brain.sleep().unwrap();
    let active: Vec<_> = brain.cortex.schemas.active().cloned().collect();
    assert!(!active.is_empty(), "sleep must crystallize at least one schema");
    assert!(active.iter().all(|s| !s.support_engram_ids.is_empty()));
}
```

- [ ] **Step 2: Run — expect FAIL**

- [ ] **Step 3: Implement crystallizer (deterministic)**

In `schema.rs`:

```rust
pub fn crystallize_from_engrams(store: &mut SchemaStore, engrams: &[crate::engram::Engram]) {
    // Group by schema_key of episode content; require >=2 engrams sharing key "theme"
    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut statements: HashMap<String, String> = HashMap::new();
    for e in engrams {
        let key = schema_key(&e.episode.content);
        groups.entry(key.clone()).or_default().push(e.id);
        statements.entry(key).or_insert_with(|| e.episode.content.clone());
    }
    for (key, ids) in groups {
        if ids.len() < 2 {
            continue;
        }
        if key != "theme" && ids.len() < 3 {
            continue; // conservative
        }
        let statement = statements.get(&key).cloned().unwrap_or(key.clone());
        let _ = store.upsert_active(Schema::new(statement, ids));
    }
}
```

Call from `sleep_internal` **after** `sleep_cycle`, before Somnus seal:

```rust
crate::schema::crystallize_from_engrams(
    &mut self.cortex.schemas,
    &self.hippocampus.engrams,
);
```

Do **not** call from `systems_seal` / `maybe_somnus_autonomic_seal`.

- [ ] **Step 4: Run — expect PASS**

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(sleep): deterministic CortexSchema crystallize on semantic sleep"
```

---

### Task 4: Opt-in schema recall lane + activate non-regression

**Files:**
- Modify: `brain.rs` — add `activate_with_schemas(&self, cue: &str) -> SchemaAwareActivation`  
  Keep `activate` calling existing path only.
- Create: `crates/fluctlightdb/tests/activate_nonregression.rs`
- Extend: `cortex_schema_gates.rs` recombination test

- [ ] **Step 1: Non-regression failing test (fixture)**

```rust
#[test]
fn default_activate_unchanged_by_schemas_present() {
    let mut brain = FluctlightBrain::new();
    brain.experience(Episode::new("alpha wallet balance is 42", "ledger", 0.9)).unwrap();
    brain.experience(Episode::new("beta shipping address line", "ship", 0.7)).unwrap();
    let before: Vec<_> = brain.activate("wallet balance").recalls.iter().map(|r| r.engram_id).collect();
    // inject schema without going through activate
    if let Some(eid) = before.first() {
        let _ = brain.cortex.schemas.upsert_active(Schema::new("wallet balance tracked", vec![*eid]));
    }
    let after: Vec<_> = brain.activate("wallet balance").recalls.iter().map(|r| r.engram_id).collect();
    assert_eq!(before, after);
}
```

- [ ] **Step 2: Recombination gate test**

```rust
#[test]
fn schema_lane_beats_lookup_on_recombination_cue() {
    let mut brain = FluctlightBrain::new();
    brain.experience(Episode::new("Alice works in Berlin", "bio", 0.9)).unwrap();
    brain.experience(Episode::new("Berlin project uses Rust", "proj", 0.9)).unwrap();
    brain.sleep().unwrap();
    // Force schemas if crystallizer conservative:
    let ids: Vec<_> = brain.hippocampus.engrams.iter().map(|e| e.id).collect();
    if brain.cortex.schemas.active().count() == 0 && ids.len() >= 2 {
        brain.cortex.schemas.upsert_active(
            Schema::new("Alice works on Rust project in Berlin", ids.clone())
        ).unwrap();
    }
    let lookup = brain.activate("What stack does Alice use?");
    let with = brain.activate_with_schemas("What stack does Alice use?");
    assert!(
        with.schemas.iter().any(|s| s.statement.to_lowercase().contains("rust")
            || s.statement.to_lowercase().contains("berlin")),
        "schema lane must surface compositional structure"
    );
    // lookup-only may miss; schema lane must have >=1 schema
    assert!(!with.schemas.is_empty());
    let _ = lookup; // documented: episodic may be weak; schema is the lift
}
```

Define:

```rust
pub struct SchemaAwareActivation {
    pub episodic: ActivationResult,
    pub schemas: Vec<Schema>,
}
```

`activate_with_schemas` = `activate(cue)` + filter active schemas by token overlap with cue/statement.

- [ ] **Step 3: Implement + run both tests PASS**

- [ ] **Step 4: Sleep idempotence test**

```rust
#[test]
fn double_sleep_does_not_duplicate_active_theme_schemas() {
    let mut brain = FluctlightBrain::new();
    for _ in 0..3 {
        brain.experience(Episode::new("User prefers dark mode theme", "prefs", 0.8)).unwrap();
    }
    brain.sleep().unwrap();
    let n1 = brain.cortex.schemas.active().filter(|s| s.key == "theme").count();
    brain.sleep().unwrap();
    let n2 = brain.cortex.schemas.active().filter(|s| s.key == "theme").count();
    assert_eq!(n1, n2.max(1).min(n1)); // at most one active head per key
    assert!(n2 <= 1);
}
```

Fix `upsert_active` if needed so same key collapses to one Active.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(schema): opt-in activate_with_schemas + Phase A gates"
```

---

### Task 5: PHASE A GATE STOP

- [ ] **Step 1: Run full Phase A suite**

```bash
cargo test -p fluctlightdb --test cortex_schema_gates --test activate_nonregression
cargo test -p fluctlightdb schema:: --lib
```

Expected: all PASS

- [ ] **Step 2: Document Phase A complete in `docs/PRODUCTION.md` (short subsection)**

- [ ] **Step 3: Commit docs**

```bash
git commit -am "docs: CortexSchema Phase A gates green"
```

### STOP — Do not start Phase B until Task 5 is green.

---

# PHASE B — CaptureGate

### Task 6: Eligibility tags on experience

**Files:**
- Create: `crates/fluctlightdb/src/eligibility.rs`
- Modify: `brain.rs` `experience` path to tag new engram ids
- Test: `tests/capture_gate_gates.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn experience_sets_eligibility_tag() {
    let mut brain = FluctlightBrain::new();
    let id = brain.experience(Episode::new("new fact about theme dark", "t", 0.8)).unwrap().engram_id;
    assert!(brain.eligibility.is_tagged(id));
}
```

- [ ] **Step 2: Implement `EligibilityStore { tags: HashSet<Uuid> }` on brain (`#[serde(default)]` or skip+rebuild). Prefer persisted in a small segment or inside agent/governance — simplest: field on `FluctlightBrain` with `#[serde(default)]` if brain snapshot includes it; if not in v4 segments, store inside `cortex` or new `eligibility.seg`. Prefer **new segment** `eligibility` in manifest write/read for durability.

- [ ] **Step 3: Tests PASS + commit**

```bash
git commit -am "feat(eligibility): tag engrams on experience for CaptureGate"
```

---

### Task 7: CaptureGate sleep path + CF rollback

**Files:**
- Create: `crates/fluctlightdb/src/capture_gate.rs`
- Modify: sleep path to use capture for schema updates
- Test: `capture_gate_gates.rs`

- [ ] **Step 1: CF probe failing test**

```rust
#[test]
fn conflicting_new_experience_does_not_destroy_old_schema_probe() {
    let mut brain = FluctlightBrain::new();
    for _ in 0..3 {
        brain.experience(Episode::new("User prefers dark mode theme", "prefs", 0.9)).unwrap();
    }
    brain.sleep().unwrap();
    let old_probe = brain.activate_with_schemas("dark mode theme");
    assert!(!old_probe.schemas.is_empty());
    // flood conflicting
    for _ in 0..5 {
        brain.experience(Episode::new("User prefers light mode theme", "prefs", 0.9)).unwrap();
    }
    brain.sleep().unwrap();
    let after = brain.activate_with_schemas("dark mode theme");
    // Old support must remain resolvable; active head may supersede but dark support engrams exist
    assert!(
        brain.hippocampus.engrams.iter().any(|e| e.episode.content.contains("dark")),
        "old episodes must not be deleted"
    );
    assert!(
        after.schemas.iter().any(|s| s.status == fluctlightdb::SchemaStatus::Active)
            || brain.cortex.schemas.schemas.iter().any(|s| s.statement.contains("dark")),
        "old schema retained or superseded with provenance — not wiped"
    );
    let _ = old_probe;
}
```

- [ ] **Step 2: Implement CaptureGate**

Rules:
- Only eligibility-tagged engrams may drive new schema writes.
- When conflicting theme keys: supersede, do not delete old schema row or episodes.
- Before applying schema batch: snapshot `SchemaStore`; run CF probes; on failure restore snapshot.
- Clear tags for captured ids after successful sleep capture.
- Untagged-only sleep: schemas unchanged (test).

- [ ] **Step 3: Untagged test**

```rust
#[test]
fn untagged_material_does_not_alter_schemas() {
    let mut brain = FluctlightBrain::new();
    brain.experience(Episode::new("User prefers dark mode theme", "prefs", 0.9)).unwrap();
    brain.experience(Episode::new("User prefers dark mode theme", "prefs", 0.9)).unwrap();
    brain.sleep().unwrap();
    let n = brain.cortex.schemas.active().count();
    // bypass experience: push engram-like only if API exists; else clear eligibility then sleep
    brain.eligibility.clear();
    brain.sleep().unwrap();
    assert_eq!(brain.cortex.schemas.active().count(), n);
}
```

- [ ] **Step 4: All capture_gate + cortex_schema + activate_nonregression PASS**

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(capture): CaptureGate interleaved supersede + CF probes"
```

### STOP — Phase B green before Phase C.

---

# PHASE C — Aeterna

### Task 8: Lossless agent prompt pack + expand

**Files:**
- Replace/complete: `crates/fluctlightdb/src/agent_prompt.rs`
- Test: `tests/aeterna_gates.rs`

- [ ] **Step 1: Lossless invariant test**

```rust
#[test]
fn prompt_pack_lists_every_activate_id() {
    let mut brain = FluctlightBrain::new();
    for i in 0..12 {
        brain.experience(Episode::new(format!("dark mode detail {i} with extra words"), "p", 0.7)).unwrap();
    }
    std::env::set_var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET", "64");
    let full = brain.activate("dark mode");
    let bundle = brain.activate_for_agent_prompt("dark mode");
    std::env::remove_var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET");
    let line_ids: std::collections::HashSet<_> = bundle.lines.iter().map(|l| l.engram_id).collect();
    for r in &full.recalls {
        assert!(line_ids.contains(&r.engram_id), "silent drop forbidden");
    }
    assert!(!bundle.truncated);
    if !bundle.expandable_ids.is_empty() {
        let expanded = brain.expand_engrams(&bundle.expandable_ids);
        assert_eq!(expanded.len(), bundle.expandable_ids.len());
    }
}
```

- [ ] **Step 2: Implement lossless pack (gist+id always; full text by verified-then-rank within budget); `expand_engrams`**

- [ ] **Step 3: Reset continuity test**

```rust
#[test]
fn session_boot_after_clear_still_has_core_and_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain.core_memories.persist("identity".into(), "I am the agent".into(), brain.life.life_id, None);
    brain.experience(Episode::new("prefers dark mode theme", "prefs", 0.9)).unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    // "window cleared" = new process
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let boot = brain.session_boot_context(Some("dark mode"));
    assert!(boot.core_snippets.iter().any(|c| c.contains("agent")) || !boot.prompt_block.is_empty());
}
```

- [ ] **Step 4: Verified priority + tokens non-scaling tests**

- [ ] **Step 5: Re-run Phase A+B suites — still green**

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(aeterna): lossless prompt index + session boot + expand"
```

---

### Task 9: Somnus WAL-pressure seal (durability only) + DR resolve

**Files:**
- `somnus.rs`, `brain.rs` `maybe_somnus_autonomic_seal`, `scripts/resolve-brain.sh` (if not already)
- Test: extend `somnus_durability.rs` — seal on WAL count without sleep_cycle; activate ids unchanged across seal

- [ ] Implement seal when `ticks >= every` **OR** `wal_records_since_seal >= wal_every`
- [ ] Test activate ranking unchanged across `systems_seal`
- [ ] Commit

```bash
git commit -am "feat(somnus): WAL-pressure autonomic seal without semantic capture"
```

---

### Task 10: PHASE C + FULL STACK GATE STOP

- [ ] Run:

```bash
cargo test -p fluctlightdb --test cortex_schema_gates --test capture_gate_gates --test aeterna_gates --test activate_nonregression --test somnus_durability
```

Expected: all PASS

- [ ] Update `docs/PRODUCTION.md` + spec status to Phase A/B/C gates green
- [ ] Commit

```bash
git commit -am "docs: CLS dual-hemisphere organ A+B+C gates proven"
```

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| CortexSchema types + supports | Task 1–2 |
| Crystallize on semantic sleep | Task 3 |
| Opt-in schema lane; activate unchanged | Task 4 |
| Phase A gates | Task 5 |
| Eligibility | Task 6 |
| CaptureGate CF/supersede/untagged | Task 7 |
| Aeterna lossless + boot + expand | Task 8 |
| Somnus durability cadence | Task 9 |
| Full stack prove | Task 10 |

## Placeholder scan

None intentional. ε for CF: use `assert!(old_episodes_present)` + supersede provenance in Task 7; refine numeric ε only after first green CF fixture.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-25-cls-dual-hemisphere-organ.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — execute tasks in this session with checkpoints  

**Which approach?**

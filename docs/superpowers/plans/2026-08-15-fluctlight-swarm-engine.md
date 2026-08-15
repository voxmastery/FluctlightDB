# Fluctlight swarm engine implementation plan

> Execute test-first on `codex_hackathon`. This plan owns the Rust state machine, persistence, allocation, evidence, and HTTP contract. It does not own Codex packaging.

**Goal:** Add a durable, single-owner swarm coordinator to FluctlightDB that allocates disjoint episodic memories to a complete worker roster, records evidence-bound outcomes, and applies feedback only to cited memories.

**Architecture:** `SwarmState` is a versioned segment of `FluctlightBrain`. Every mutation is a validated, idempotent `SwarmTransaction` written to the existing WAL before it is applied. `BrainServer` remains the only writer and exposes capability-separated worker and verifier endpoints.

**Tech stack:** Rust, serde, UUID, existing v4 segments/WAL/HTTP server, Cargo tests.

---

## Task 1: Define the durable domain model

**Files:**

- Create: `crates/fluctlightdb/src/swarm.rs`
- Modify: `crates/fluctlightdb/src/lib.rs`
- Test: `crates/fluctlightdb/src/swarm.rs`

1. Write failing unit tests for a complete immutable roster, duplicate worker rejection, state transitions, and stable serialization.
2. Run `cargo test -p fluctlightdb swarm::tests -- --nocapture` and confirm the new tests fail because the model is absent.
3. Implement these public types with `Serialize`, `Deserialize`, `Clone`, `Debug`, `PartialEq` where appropriate:

```rust
pub struct SwarmState {
    pub schema_version: u32,
    pub runs: HashMap<Uuid, SwarmRun>,
    pub feedback: HashMap<Uuid, EngramFeedback>,
    pub truth_revisions: HashMap<String, Vec<TruthRevision>>,
    pub applied_transactions: HashSet<Uuid>,
}

pub struct SwarmRun {
    pub id: Uuid,
    pub project_id: String,
    pub objective_digest: String,
    pub base_commit: String,
    pub policy_version: String,
    pub roster: Vec<WorkerSlot>,
    pub allocations: HashMap<String, MemoryBundle>,
    pub attempts: HashMap<String, Attempt>,
    pub status: SwarmStatus,
}

pub enum EvidenceResult { Success, Failure, Inconclusive, ReproducedFailure }
```

4. Keep `WorkerSlot` identity coordinator-assigned; request payloads must not be able to replace an already-bound agent/worktree.
5. Run the focused tests and commit: `feat(swarm): add durable swarm domain model`.

## Task 2: Implement deterministic global allocation

**Files:**

- Modify: `crates/fluctlightdb/src/swarm.rs`
- Test: `crates/fluctlightdb/src/swarm.rs`

1. Add failing tests proving:
   - shared truth and mandatory warnings are identical for all slots;
   - episodic engram IDs never overlap while enough candidates exist;
   - candidate shortage sets `diversity_degraded=true` rather than silently duplicating;
   - equal inputs produce equal assignments;
   - failed/reproduced-failure memories enter the warning lane, not the advice lane.
2. Implement a deterministic greedy allocator. Sort candidates by adjusted score and stable UUID, then apply an overlap penalty across already-assigned bundles.

```rust
pub fn allocate_roster(
    roster: &[WorkerSlot],
    truth: &[MemoryCandidate],
    warnings: &[MemoryCandidate],
    episodes: &[MemoryCandidate],
    per_worker: usize,
) -> Result<HashMap<String, MemoryBundle>>;
```

3. Store exact exposed engram IDs and scores in each bundle so later credit cannot be inferred from mutable recall.
4. Run focused tests and commit: `feat(swarm): allocate disjoint worker memory bundles`.

## Task 3: Add atomic idempotent swarm transactions to the WAL

**Files:**

- Modify: `crates/fluctlightdb/src/swarm.rs`
- Modify: `crates/fluctlightdb/src/wal.rs`
- Modify: `crates/fluctlightdb/src/brain.rs`
- Test: `crates/fluctlightdb/src/wal.rs`
- Test: `crates/fluctlightdb/tests/crash_recovery.rs`

1. Add failing tests for duplicate transaction replay, torn-process recovery, and invalid transitions that must not reach the WAL.
2. Add one WAL variant:

```rust
WalEntry::SwarmTransaction { transaction: SwarmTransaction }
```

3. Add `pub swarm: SwarmState` to `FluctlightBrain`, including `new`, `clone`, and `from_snapshot` initialization.
4. Implement `apply_swarm_transaction`. Validate against a cloned state first, append the validated transaction to WAL, then replace live state. Replay calls the same idempotent apply path without appending.
5. Ensure transaction UUIDs survive serialization and are recorded in `applied_transactions`.
6. Run WAL and crash-recovery tests and commit: `feat(swarm): persist atomic swarm transactions in wal`.

## Task 4: Persist an optional v4 swarm segment

**Files:**

- Modify: `crates/fluctlightdb/src/manifest.rs`
- Test: `crates/fluctlightdb/src/manifest.rs`
- Add fixture if needed: `crates/fluctlightdb/tests/fixtures/v4_without_swarm/`

1. Write failing tests that checkpoint/reopen all swarm data and load a pre-swarm v4 store as empty state.
2. Write `swarm.seg` during v4 checkpoint and add `"swarm"` to new manifests.
3. Load it with `unwrap_or_default()` so existing stores remain readable.
4. Verify WAL replay after the snapshot watermark does not double-apply a transaction.
5. Run manifest and crash tests and commit: `feat(swarm): checkpoint swarm state in v4 stores`.

## Task 5: Implement coordinator operations and targeted feedback

**Files:**

- Modify: `crates/fluctlightdb/src/swarm.rs`
- Modify: `crates/fluctlightdb/src/brain.rs`
- Test: `crates/fluctlightdb/src/swarm.rs`
- Test: `crates/fluctlightdb/tests/integration.rs`

1. Add failing tests for begin, claim, cite, report, verify, finish, and feedback recall behavior.
2. Implement operations as transactions:

```rust
begin_swarm(request) -> SwarmRun
claim_slot(swarm_id, slot_id, agent_id, worktree) -> MemoryBundle
cite_memories(swarm_id, slot_id, memory_ids) -> CitationReceipt
report_attempt(swarm_id, slot_id, tree_hash, summary) -> PendingAttempt
record_evidence(verifier_capability, EvidenceReceipt) -> VerifiedOutcome
finish_swarm(swarm_id, accepted_attempt) -> SwarmSummary
```

3. Reject citations not present in that slot's exposure set.
4. Update feedback only for cited memories after trusted evidence. Preserve separate counters for success, failure, inconclusive, and reproduced failure.
5. Expose a recall-adjustment helper that routes reproduced failures to mandatory warnings and boosts only supported successes; do not call global `reward()`.
6. Run focused tests and commit: `feat(swarm): add evidence-bound targeted learning`.

## Task 6: Expose capability-separated HTTP endpoints

**Files:**

- Modify: `crates/fluctlightdb/src/serve.rs`
- Test: `crates/fluctlightdb/tests/serve_integration.rs`

1. Add failing HTTP integration tests for all operations, idempotency across restart, tenant isolation, spoofed identity, worker self-verification, and concurrent slot claims.
2. Add request/response structs instead of expanding the catch-all `ApiRequest` for complex swarm bodies.
3. Add endpoints under `/api/v1/swarm/*`:

```text
POST /begin
POST /claim
POST /cite
POST /attempt
POST /evidence
POST /finish
POST /get
```

4. Worker tokens may claim/cite/report only. A distinct verifier secret from environment may submit evidence. Do not expose raw `/experience` or `verified=true` through the plugin worker capability.
5. Map domain conflicts to 409, invalid input to 400, missing IDs to 404, and capability failures to 403.
6. Run server integration tests and commit: `feat(swarm): expose secure coordinator http api`.

## Task 7: Engine verification gate

1. Run `cargo fmt --all -- --check`.
2. Run `cargo clippy -p fluctlightdb --all-targets -- -D warnings`.
3. Run `cargo test -p fluctlightdb`.
4. Run crash/restart tests three consecutive times with `FLUCTLIGHT_WAL_FSYNC=always`.
5. Confirm `git diff --check` and that `.fluctlight/handoffs.jsonl` remains uncommitted.
6. Commit only verification-driven fixes.


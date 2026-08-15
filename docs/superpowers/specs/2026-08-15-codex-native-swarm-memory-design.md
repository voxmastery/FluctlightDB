# Codex-native swarm memory — design

**Date:** 2026-08-15  
**Status:** Approved for planning  
**Repositories:** FluctlightDB plus an OpenAI Codex fork  
**Working branch:** `codex_hackathon`  
**Product name:** Fluctlight Swarm Memory  
**Principle:** Parallel agents should share verified truth without inheriting the same search path.

## Goal

Upgrade the open-source Codex CLI and App Server with a native swarm-memory lifecycle backed by FluctlightDB. The system must give every parallel worker the same pinned, verified project truth while allocating distinct episodic memories, preserving exploration diversity, recording evidence-bound outcomes, and learning which approaches should be reused or avoided.

The hackathon deliverable must be a working vertical slice, not a mock:

1. Codex starts multiple real worktree workers for one objective.
2. FluctlightDB creates a pinned swarm run and allocates memory bundles globally.
3. Workers receive shared truth, mandatory warnings, and distinct strategy memories.
4. Workers cite memories they used.
5. Trusted Git and test evidence records the result.
6. Targeted feedback changes later recall without allowing workers to verify themselves.
7. A comparative evaluation shows whether the system reduces repeated approaches and failures.

## Why this is a strong Codex hackathon project

This project addresses a failure mode created by Codex's most ambitious workflow: parallel agents operating in separate worktrees. Ordinary shared memory improves continuity, but it can also make every worker retrieve the same prior solution and collapse a parallel search into several copies of one attempt.

The project is particularly suited to a Codex event because it combines:

- Native parallel-agent and worktree orchestration
- A Rust change to the open-source Codex CLI/App Server
- A reusable memory-provider contract rather than a one-off prompt
- A Codex Skill and MCP compatibility path for unmodified installations
- Evidence-based evaluation of agent behavior
- A live, understandable before/after demo

The claim is not that no other database could implement a coordinator. The defensible claim is that FluctlightDB already provides the episodic representation, associative recall, provenance, agent/tenant scoping, consolidation, and durable experience path needed to make the coordinator cognitive rather than a static task queue.

## Problem definition

Parallel coding agents currently face six connected memory problems:

1. **Convergence:** workers receive the same highly ranked memories and attempt the same solution.
2. **Repeated failure:** failed approaches are stored as text but can later be recalled as advice.
3. **Untrusted promotion:** a worker can mistake its own assertion or a passing test for general truth.
4. **Incorrect credit:** a global reward cannot identify which recalled memory helped or harmed an attempt.
5. **Concurrency:** independently opened embedded brains can retain stale snapshots and overwrite one another.
6. **No reproducible audit:** there is no durable record tying a worker, memory exposure, Git tree, verification result, and promotion decision together.

The system cannot guarantee that agents think differently. It can guarantee that the memory layer does not force premature convergence, measure the remaining semantic overlap, and make the provenance of learned behavior auditable.

## Chosen architecture

Use a **single-owner, Rust-native Fluctlight Swarm Coordinator**. Codex workers never open the underlying brain and never call verification or promotion primitives directly.

```text
Codex CLI / App Server
  parallel-run scheduler
          |
          | SwarmMemoryProvider lifecycle
          v
Fluctlight Swarm Coordinator (single owner)
  |-- durable swarm transactions and capability validation
  |-- global memory allocation and exposure tracking
  |-- trusted evidence and promotion policy
  `-- FluctlightDB brain: experience, recall, provenance, consolidation
```

### Repository boundary

The delivery uses two repositories rather than vendoring Codex into FluctlightDB:

| Repository | Responsibility |
|---|---|
| FluctlightDB, branch `codex_hackathon` | Coordinator, durable swarm state, targeted feedback, allocation policy, APIs, MCP compatibility, Skill, evaluation fixtures |
| OpenAI Codex fork, branch `fluctlight-swarm-memory` | Provider interface, scheduler hooks, worktree identity binding, evidence callbacks, configuration, integration tests |

The Codex patch must remain small enough to review independently. Product policy belongs in FluctlightDB; Codex supplies lifecycle facts and enforces the worker boundary.

## Codex integration contract

Add an asynchronous `SwarmMemoryProvider` boundary to Codex core. Exact file placement will be determined after auditing the current Codex source, but the semantic interface is fixed:

```rust
trait SwarmMemoryProvider {
    async fn begin_swarm(&self, request: BeginSwarm) -> Result<SwarmHandle>;
    async fn worker_context(&self, request: WorkerContextRequest) -> Result<WorkerContext>;
    async fn cite_memories(&self, request: MemoryCitation) -> Result<()>;
    async fn report_attempt(&self, request: AttemptReport) -> Result<PendingAttempt>;
    async fn record_evidence(&self, request: EvidenceReceipt) -> Result<VerifiedOutcome>;
    async fn finish_swarm(&self, request: FinishSwarm) -> Result<SwarmSummary>;
}
```

### Required scheduler hooks

1. **Before workers launch:** Codex supplies the complete immutable worker roster, objective, repository identity, base commit, worktree identities, and policy version.
2. **Before each worker's first turn:** Codex fetches that slot's typed context bundle.
3. **During execution:** workers can cite memory IDs, but citations do not alter authority.
4. **At attempt completion:** Codex reports result commit/tree and claimed status as pending.
5. **After trusted checks:** Codex submits an evidence receipt produced by its verifier path.
6. **At swarm completion:** Codex records the accepted result and closes outstanding leases.

Codex must derive worker and worktree identity from internal run state. Request bodies cannot override identity.

### Compatibility path

For Codex versions without the native provider interface, FluctlightDB exposes the same contracts through MCP tools and a Skill. This is a compatibility and adoption path, not the strongest security boundary.

## Typed context lanes

Worker context is never one concatenated prompt blob. It contains separately typed lanes:

```json
{
  "snapshot": {
    "swarm_id": "swarm-...",
    "base_commit": "...",
    "truth_revision": 12,
    "policy_version": "v1"
  },
  "verified_truth": [],
  "mandatory_warnings": [],
  "episodic_memories": [],
  "allocation": {
    "strict_id_disjoint": true,
    "semantic_overlap_score": 0.18,
    "diversity_degraded": false
  }
}
```

- **Verified truth** is identical for every worker and pinned for the swarm lifetime.
- **Mandatory warnings** contain verified safety constraints and reproduced failure patterns. They may overlap across workers intentionally.
- **Episodic memories** contain potentially useful approaches and are allocated for diversity. They are explicitly untrusted data, not instructions.
- **Private worker state** never becomes shared truth merely because another worker can retrieve it.

Prompt-injection-resistant formatting must label episodic content as quoted historical data and preserve provenance metadata outside the text.

## Durable data model

### Swarm run

```text
SwarmRun
  id
  project_id / tenant_id
  objective_digest
  repository_identity
  base_commit
  truth_revision
  policy_version
  worker_roster
  allocation_epoch
  status
  created_at / closed_at
```

The roster is known before initial allocation. V1 does not allow silent dynamic membership.

### Memory catalog entry

```text
MemoryRecord
  stable_memory_id
  engram_id
  kind: truth | warning | procedure | observation | inference
  lifecycle: candidate | active | superseded | rejected
  source_revision
  validity_scope
  provenance
  utility_posterior
```

The stable memory ID is the coordination identity. The engram ID points to FluctlightDB's episodic representation.

### Allocation and exposure

```text
MemoryAllocation
  swarm_id
  epoch
  worker_slot
  memory_ids
  candidate_set_digest
  ranking_policy_digest
  semantic_overlap_score
  lease_status
```

Initial episodic allocations are exactly memory-ID-disjoint. If the candidate set is insufficient, the coordinator returns fewer memories and sets `diversity_degraded=true`; it never silently duplicates them.

### Attempt and citation

```text
Attempt
  attempt_id
  swarm_id / worker_slot
  base_commit / result_commit / result_tree
  delivered_memory_ids
  cited_memory_ids
  claimed_verdict
  verification_state
  idempotency_key
```

Delivered memory is not assumed to have influenced an attempt. Targeted utility updates require an explicit citation recorded before the outcome is finalized.

### Evidence receipt

```text
EvidenceReceipt
  attempt_id
  verifier_identity
  repository_identity
  result_tree
  check_id
  command_digest
  exit_code
  output_digest
  redacted_summary
  verdict
```

Only trusted verifier capabilities can create evidence receipts. Commands come from an allowlisted project configuration, never worker-provided shell text.

### Targeted feedback

```text
EngramFeedback
  feedback_id
  memory_id / engram_id
  attempt_id / evidence_receipt_id
  result: success | failure | inconclusive | reproduced_failure
  effect: utility_update | warning_strength | supersession_candidate
  policy_version
  idempotency_key
```

Procedural utility uses a versioned Beta posterior over verified successes and failures. A failure does not simply make a memory disappear: reproduced failures become stronger warnings and are ranked into the `mandatory_warnings` lane.

Epistemic authority is separate from operational utility. Passing tests can increase the utility of a cited procedure; they cannot verify unrelated factual claims in that procedure's text.

### Truth revisions

Truth is append-only:

```text
TruthRevision
  revision
  parent_revision
  assertions
  evidence_receipts
  promotion_policy
  supersedes
```

Corrections create a later revision. Existing swarms keep their pinned revision; new swarms receive the latest eligible revision.

## Persistence and recovery

The final architecture uses FluctlightDB's Rust persistence path as the sole authority. SQLite is not part of the permanent design.

Add a versioned swarm transaction to the existing WAL so coordination and memory effects replay together:

```text
WalEntry::SwarmTransaction {
    transaction_id,
    idempotency_key,
    events: Vec<SwarmEvent>,
}
```

`SwarmEvent` covers begin, allocation, citation, attempt, evidence, feedback, promotion, supersession, and finish.

Requirements:

- Exactly-once mutation through persistent idempotency keys
- CRC/integrity behavior matching the existing WAL
- Replay produces the same swarm state and targeted feedback
- Crash between WAL append and response is safe to retry
- A new optional snapshot segment stores materialized swarm state
- Old snapshots open with empty swarm state
- Migration and crash-recovery tests cover both legacy and new formats
- Existing `experience()` durability remains intact

Do not add unversioned fields directly to persisted bincode structs without explicit migration fixtures.

## Allocation algorithm

At `begin_swarm`, the coordinator retrieves an oversized candidate pool once for the pinned project state. Allocation is computed for the full roster in one deterministic transaction, preventing first-caller advantage.

1. Retrieve globally and over-fetch because current post-recall agent filtering can starve candidates.
2. Separate verified truth, warnings, and procedural/episodic candidates.
3. Remove records invalid for the pinned commit or tenant.
4. Score each episodic candidate using:
   - FluctlightDB relevance
   - Evidence-derived operational utility
   - recency/validity
   - previous exposure pressure
   - warning/supersession state
5. Allocate round-robin with maximal-marginal-relevance against memories already assigned to other slots.
6. Guarantee exact ID disjointness for the initial epoch.
7. Report semantic overlap as a metric rather than claiming a perfect semantic guarantee.

All weights, embedding choices, thresholds, and tie-breaking are part of a named policy version. Stable ordering and seeded tie-breaking make allocations reproducible.

V1 supports one initial epoch and one explicit follow-up epoch. Follow-up recall must account for every prior exposure in the swarm.

## Promotion policy

Workers can propose memories but cannot mark them verified, promote them, or invoke reconsolidation.

Promotion requires:

1. An accepted attempt bound to a Git tree
2. A trusted evidence receipt for configured checks
3. An assertion type eligible for automatic promotion
4. No unresolved contradiction with the pinned or current truth lattice
5. A policy decision recorded in the WAL

Repository facts require Git/blob evidence. User decisions require trusted user or administrator attestation. LLM inferences are never automatically promoted. A successful procedure may become an active procedural memory after one verified result, but project-wide factual truth has a higher gate.

Promotion creates a new immutable memory/truth revision. It does not mutate an old trace through the current non-WAL `verify_fact()` or `reconsolidate()` paths.

## Security model

- Coordinator issues short-lived capabilities bound to swarm, worker slot, worktree, and operation.
- Workers receive context/citation/report capabilities only.
- The trusted verifier receives evidence capability only.
- Promotion is an internal policy action with no worker-facing tool.
- Agent identity is derived from the capability, not request JSON.
- Every query applies project and tenant filters before ranking.
- Raw verifier output is redacted; digests are retained by default.
- Repository test execution remains sandboxed because tests are executable code.
- Episodic text is data-only and cannot override Codex instructions.
- Direct access to unsafe verification, reconsolidation, consensus, or arbitrary file-read endpoints is unavailable to workers.

## Public APIs and tools

Native provider calls map to coordinator endpoints. Compatibility MCP exposes:

| Tool | Caller | Purpose |
|---|---|---|
| `fluctlight_swarm_begin` | Codex orchestrator | Register complete roster and pin state |
| `fluctlight_swarm_context` | Worker capability | Retrieve typed assigned bundle |
| `fluctlight_swarm_cite` | Worker capability | Declare memories used |
| `fluctlight_swarm_report` | Worker capability | Submit pending attempt |
| `fluctlight_swarm_status` | Orchestrator | Inspect allocation, evidence, and outcomes |

Verification and promotion remain internal coordinator operations. They are deliberately absent from worker MCP tools.

## Error handling

- A stale base commit or truth revision rejects `begin_swarm` unless the caller explicitly starts a new run.
- Duplicate idempotency keys return the original result.
- Unknown or uncited memory IDs reject targeted feedback.
- Evidence with a mismatched tree, worktree, attempt, or verifier capability is rejected.
- Candidate shortage degrades bundle size and reports it; it does not duplicate hidden assignments.
- FluctlightDB recall/index failure prevents new allocation but does not corrupt durable swarm state.
- A crashed worker's lease can be closed by the orchestrator; its allocation remains in the exposure audit.
- WAL replay must fail closed on corrupt swarm transactions using the existing recovery policy.

## Hackathon evaluation

The demo compares the same Codex model, task, repository state, tools, and worker count under two policies:

1. **Baseline:** ordinary shared recall, independently selecting the highest-ranked memories.
2. **Fluctlight Swarm:** pinned truth plus globally allocated distinct episodic bundles and targeted warnings.

Use a fixture repository with a real failing task and several plausible approaches, including at least one historically failed approach. Run enough repeated trials to avoid presenting one lucky trace.

### Primary metrics

- Pairwise memory-ID overlap between workers
- Semantic overlap between assigned bundles
- Distinct approach count
- Repeated known-failure count
- Verified task success rate
- Time and tool calls to first verified success
- Percentage of outcomes with valid memory citations and evidence receipts
- WAL restart/replay consistency

### Demo sequence

1. Show three baseline agents retrieving substantially overlapping memories and repeating an approach.
2. Start a Fluctlight swarm on the same pinned commit.
3. Inspect common truth, common warnings, and three distinct strategy bundles.
4. Run real workers, show memory citations and test/Git evidence.
5. Restart the coordinator and show recovered allocations/outcomes.
6. Start a second swarm and show that verified success and reproduced failure changed the correct memory lanes.

The presentation must distinguish guarantees from measurements: ID disjointness is guaranteed; semantic and behavioral diversity are measured.

## Testing strategy

### FluctlightDB unit tests

- Swarm transaction serialization and replay
- Idempotent begin/allocation/citation/outcome
- Deterministic full-roster allocation
- Strict episodic ID disjointness
- Candidate-shortage degradation
- Targeted success/failure posterior updates
- Reproduced failure enters warning lane
- Authority remains unchanged by procedural success
- Capability and identity rejection

### FluctlightDB integration tests

- Crash after WAL append and retry
- Legacy snapshot opens with empty swarm state
- New snapshot/WAL restart round-trip
- Multi-client calls serialize through one coordinator
- Evidence/tree mismatch rejection
- Promotion and supersession across truth revisions
- MCP compatibility parity with native API

### Codex integration tests

- Complete worker roster registered before launch
- Worktree/agent identity cannot be spoofed
- Typed context is injected into the correct worker only
- Citations and attempt reports are bound to the run
- Trusted verifier callback uses configured checks
- Provider outage produces a clear policy-controlled failure/fallback
- Parallel task behavior remains unchanged when the provider is disabled

### Evaluation tests

- Baseline and treatment use identical frozen task inputs
- Metrics are computed from event logs, not presentation annotations
- Repeated runs emit a machine-readable comparison artifact
- Demo claims are gated on the frozen comparison artifact

## Success gates

- [ ] Two or more real Codex worktree workers use the native provider lifecycle
- [ ] Shared verified truth is byte-for-byte identical for every worker in a swarm
- [ ] Initial episodic allocations have zero memory-ID overlap
- [ ] Candidate shortage is explicit and deterministic
- [ ] Workers cannot forge identity, evidence, verification, or promotion
- [ ] A verified outcome updates only cited memories
- [ ] Reproduced failures are recalled as warnings, never ordinary advice
- [ ] Swarm state and feedback survive restart through WAL replay
- [ ] Legacy FluctlightDB stores remain readable
- [ ] Provider-disabled Codex behavior has no regression
- [ ] Comparative evaluation reports diversity, repeated failure, and verified success
- [ ] A short live demo completes without manual database editing

## V1 scope

Include:

- One local repository and one machine
- Two to four fixed Codex workers/worktrees
- One coordinator process and one embedded FluctlightDB owner
- Native Codex provider plus MCP/Skill compatibility
- One initial and one follow-up allocation epoch
- Trusted Git/tree and allowlisted-test evidence
- Persistent targeted feedback and truth revisions
- CLI/status output and machine-readable evaluation artifacts

Defer:

- Multi-host consensus, high availability, and replication
- Arbitrary dynamic worker membership
- Automatic code merging
- General causal credit assignment
- Automatic promotion of LLM summaries or inferences
- Guaranteed semantic non-overlap
- Full graphical dashboard
- Agent-supplied verifier commands

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Codex fork becomes too large for a hackathon | Keep the provider contract thin; put policy and persistence in FluctlightDB |
| Rust persistence migration regresses existing stores | Optional versioned segment, legacy fixtures, crash/replay tests |
| The demo looks like ordinary RAG | Show scheduler hooks, global allocation transaction, citations, evidence, targeted feedback, and restart recovery |
| Agents ignore assigned memories | Require citations for credit; measure behavior rather than claiming compliance |
| Diversity hurts answer quality | Keep verified truth and mandatory warnings common; compare success and time as well as overlap |
| A passing test over-promotes bad facts | Separate operational utility from epistemic authority |
| Workers poison memory | Proposal-only worker role; trusted evidence and promotion capabilities |
| Prompt injection through historical episodes | Typed data lanes, provenance, quoting, and no instruction authority |
| Upstream Codex APIs change | Isolate integration behind one provider trait and small scheduler hooks |

## Decision log

- Product target: upgrade open-source Codex CLI/App Server, not closed Codex surfaces.
- Memory engine: FluctlightDB remains reusable and owns swarm semantics.
- Coordination: single-owner Rust coordinator; workers never open embedded brains.
- Persistence target: native versioned WAL transactions; no permanent SQLite authority.
- Truth: shared and pinned; strategies: diverse; warnings: shared when mandatory.
- Feedback: targeted, evidence-bound, and citation-dependent.
- Promotion: append-only revisions, never worker self-verification.
- Delivery: native Codex provider plus MCP and Skill compatibility.
- Evaluation: guaranteed ID disjointness plus measured semantic/behavioral diversity.
- Upstream posture: maintain a small reviewable Codex patch suitable for a public PR.

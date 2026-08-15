# Codex swarm-memory source audit

**Audit date:** 2026-08-15  
**Codex commit:** `85fc4def358b7df21883e72ae8dda43a0f572f32`  
**FluctlightDB branch:** `codex_hackathon`  
**Verdict:** A fully functioning integration can ship as a FluctlightDB Codex plugin on stock Codex. One small Codex enhancement is still needed to make complete-roster declaration scheduler-enforced instead of protocol-enforced.

## What Codex already provides

### Parallel-agent control

Codex shares one `AgentControl` across a root session tree. It owns the session-scoped agent registry, execution limit, thread spawning, messaging, and agent metadata.

Relevant current source:

- `codex-rs/core/src/agent/control.rs`
- `codex-rs/core/src/agent/control/spawn.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs`
- `codex-rs/core/src/thread_manager.rs`

The current `spawn_agent` flow reserves one agent slot, creates one child thread, registers metadata, and immediately sends that child's first communication. Agents are spawned one call at a time; no API registers an entire planned roster before the first worker starts.

Implication: a native guarantee of fair global allocation cannot be attached only after individual spawns. The roster must either be declared through the Fluctlight Skill before spawning or supported by a new generic batch operation in Codex.

### Lifecycle hooks

Codex already has the exact adapter surface needed for a stock installation:

- `SubagentStart` runs before a child turn and can inject model context.
- `SubagentStop` runs when a child attempts to finish and can report the final message/transcript identity.
- `PostToolUse` can observe tool completion where needed.
- Hook handlers can be command, prompt, agent, or MCP-tool handlers.
- MCP hook inputs support `${field.path}` templates and preserve JSON value types.

Relevant current source:

- `codex-rs/hooks/src/events/session_start.rs`
- `codex-rs/hooks/src/events/stop.rs`
- `codex-rs/hooks/src/events/post_tool_use.rs`
- `codex-rs/hooks/src/engine/mcp_runner.rs`
- `codex-rs/config/src/hook_config.rs`

`SubagentStart` supplies:

```text
session_id, turn_id, cwd, model, permission_mode, agent_id, agent_type
```

`SubagentStop` additionally supplies:

```text
agent_transcript_path, last_assistant_message, stop_hook_active
```

This is sufficient to bind an allocated memory bundle and a pending attempt to a real Codex agent and worktree without trusting model-supplied identity fields.

### Bounded context injection

Codex turns hook output into separate contextual model fragments and applies per-hook token limits/spilling. Its repository rules require all injected context to be bounded and represented as contextual fragments.

Relevant current source:

- `codex-rs/core/src/hook_runtime.rs`
- `codex-rs/context-fragments/src/fragment.rs`
- `codex-rs/context-fragments/src/additional_context.rs`

The Fluctlight hook response should therefore inject one typed, bounded block containing:

- pinned snapshot metadata
- verified truth
- mandatory warnings
- assigned episodic memories and stable IDs
- citation/report instructions

Historical episodic text remains labeled as untrusted data.

### Plugin packaging

Codex plugin manifests can package a Skill, an MCP server declaration, and hook configuration. Hook configuration may be inline or path-based.

Relevant current source:

- `codex-rs/plugin/src/manifest.rs`
- `codex-rs/core-plugins/src/loader.rs`
- `codex-rs/hooks/src/declarations.rs`

This lets FluctlightDB ship the complete stock-Codex adapter under its MIT license:

```text
plugins/fluctlight-swarm/
  .codex-plugin/plugin.json
  .mcp.json
  hooks/hooks.json
  skills/fluctlight-swarm/SKILL.md
```

No Codex source modification is required for this release path.

## What FluctlightDB already provides

- WAL-backed immutable `Episode` experience
- Stable engram IDs
- Agent and tenant fields
- Outcome text
- RAG reference fields usable for run/attempt idempotency
- Provenance kind, source URI, confidence, and verified flag
- HTTP serialization through one server-owned brain
- Recall, conflict resolution, consolidation, and project-memory adapters

Relevant current source:

- `crates/fluctlightdb/src/types.rs`
- `crates/fluctlightdb/src/brain.rs`
- `crates/fluctlightdb/src/wal.rs`
- `crates/fluctlightdb/src/serve.rs`
- `sdks/python/fluctlightdb/mcp_server.py`

## Gaps that must be implemented

### Targeted feedback

Current `reward()` changes global dopamine rather than one memory. It cannot mean "these cited memories helped this verified attempt." Add durable `EngramFeedback` keyed by stable memory/engram ID, with success, failure, inconclusive, and reproduced-failure results.

### Negative-memory semantics

Current outcomes are free text and normal recall can return a failed episode as advice. Add a separate warning lane and never encode failure as a simple negative ranking scalar.

### Durable swarm control state

Current WAL covers experience, sleep, ticks, global reward, core marking, death, and compact. It does not persist swarm runs, allocations, citations, evidence, feedback, or truth revisions. Add versioned swarm transaction entries plus an optional v4 snapshot segment.

### Atomic allocation

Current project-brain Python objects can hold stale snapshots across processes. All workers must call one coordinator/server owner. The coordinator allocates the full declared roster in one transaction and serves immutable slot bundles afterward.

### Evidence authority

Current HTTP write callers can provide `verified=true` and provenance fields. Worker capabilities must not expose those paths. Only the trusted verifier may create evidence receipts or trigger promotion.

## Does it solve the issue?

Yes, under a precise contract:

1. The root declares the full roster before worker spawn.
2. One coordinator owns FluctlightDB.
3. Each start hook atomically claims one precomputed slot.
4. Shared truth and mandatory warnings are identical for the swarm.
5. Episodic assignment IDs are strictly disjoint.
6. Agents cite memory IDs before outcome finalization.
7. Only coordinator-run allowlisted checks create evidence.
8. Feedback applies only to cited memories and survives restart.

The integration cannot guarantee different reasoning. It guarantees non-duplicated episodic inputs, records remaining semantic overlap, prevents reproduced failures from masquerading as advice, and measures behavioral diversity and task success.

On stock Codex, roster declaration is enforced by the Skill/plugin protocol and the coordinator fails closed if it is missing. The proposed Codex change makes that same rule scheduler-enforced.

## Licensing and contribution constraints

| Component | License / policy | Consequence |
|---|---|---|
| FluctlightDB | MIT | The coordinator, plugin, Skill, and adapters can remain MIT-licensed; preserve the copyright and license notice in distributions. |
| OpenAI Codex | Apache-2.0 | A distributed Codex fork must retain the Apache license and NOTICE requirements. Keep MIT FluctlightDB code separable rather than relicensing the fork. |
| Codex contributions | Invitation only | Start with an issue containing reproduction, source analysis, design, and benchmark evidence. An unsolicited PR will be closed without review. If invited, sign the Codex CLA and submit the narrow generic change. |

The licenses are compatible for separate components communicating through MCP/HTTP. The FluctlightDB MIT license does not convert modified Codex files to MIT; Codex modifications remain under the Codex project's Apache-2.0 contribution terms.

## Recommended delivery order

1. Implement and verify native Fluctlight swarm state, allocator, feedback, evidence, and persistence.
2. Ship the MIT Codex plugin using existing MCP-backed lifecycle hooks.
3. Run a frozen baseline-vs-swarm evaluation on real worktree agents.
4. Publish the working fork/plugin and evidence.
5. Open a Codex enhancement issue requesting a generic batch roster/swarm-start lifecycle.
6. Prepare the narrow Codex patch only for the fork and for upstream submission if invited.

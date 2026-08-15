---
name: fluctlight-swarm
description: Coordinate parallel Codex agents with shared verified truth, mandatory failure warnings, and disjoint episodic memories. Use when spawning two or more agents on one objective.
---

# Fluctlight Swarm

1. Determine the complete roster before spawning. Every `slot_id` must be unique and equal its Codex `agent_type`.
2. Prepare one allocation per slot. Verified truth and mandatory warnings must be identical; episodic memory IDs must not overlap.
3. Call `fluctlight_swarm_begin` exactly once with the full roster and allocations. Stop if it fails.
4. Only after begin succeeds, spawn the workers. `SubagentStart` claims each slot and injects bounded context.
5. Require workers to call `fluctlight_swarm_cite` with only memory IDs they actually used.
6. Let `SubagentStop` record the pending attempt. Never treat a worker's own success claim as verified evidence.
7. Inspect the durable run with `fluctlight_swarm_get` and run trusted repository checks before accepting a result.

Do not spawn duplicate agent types in one swarm. Define distinct types such as `backend-a` and `backend-b` first.

# Fluctlight Swarm Memory — 61-second demo narration

Parallel coding agents move fast, but their coordination is mostly ephemeral. Workers can receive the same strategy, repeat a failure that another worker already discovered, or promote a success claim without trusted evidence.

Fluctlight Swarm Memory gives Codex agents one durable coordinator backed by FluctlightDB. Every worker receives verified project truth and known failure warnings, while episodic strategies are allocated without overlap. That means the swarm shares what must be consistent without forcing every agent to think the same way.

Each attempt is bound to a real agent and worktree. A worker cannot cite memory assigned to a peer, and it cannot verify its own outcome. Only evidence accepted by a trusted verifier can apply targeted credit—or turn a reproduced failure into a warning.

The one-command demo proves all four behaviors: disjoint allocation, citation isolation, evidence-gated feedback, and durable recovery after a full restart. The Rust coordinator, Python MCP bridge, Codex plugin, tests, and demo are public and MIT licensed.

Fluctlight Swarm Memory: parallel Codex agents that remember together without thinking the same way.

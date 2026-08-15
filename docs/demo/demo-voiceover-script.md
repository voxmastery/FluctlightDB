# Fluctlight Swarm Memory — 54-second terminal demo narration

This scripted visualization shows Fluctlight Swarm Memory as a Codex workflow. The root starts one durable coordinator run, and two illustrated workers act against the same project state.

Both workers receive the same verified truth and failure warning, but FluctlightDB assigns each a different episodic strategy. The API worker gets the transaction-boundary memory. The test worker gets the crash-recovery memory. Their allocation overlap is zero.

When one worker tries to cite its peer’s memory, the coordinator rejects it. A worker also cannot verify its own success. Trusted test evidence applies credit only to the memory actually cited.

Finally, the public one-command demo proves all four behaviors, stops the coordinator, restarts it, and recovers the completed run from the WAL and version-four checkpoint.

Parallel Codex agents can now remember together without thinking the same way.

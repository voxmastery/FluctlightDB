# Somnus — CLS-native durability (always-on, no quality tradeoff)

**Status:** invented 2026-07-25 · autonomic seal 2026-07-25 · FluctlightDB native  
**Problem:** v4 generations treated *every* amortized write (including brainstem ticks) as a full neocortical reprint → disk bomb under continuous agents.  
**Claim:** durability should obey **Complementary Learning Systems**, not a flat checkpoint counter — **without** changing activate / CHORUS / benchmark ranking.

## Always on

Somnus is **default ON**. Users do **not** turn it on. `FLUCTLIGHT_SOMNUS=0` is debug-only (legacy wake checkpoints).

## Neuroscience grounding

| Brain | Pre-Somnus (broken) | Somnus |
|-------|---------------------|--------|
| Brainstem / autonomic ticks | `tick` → `maybe_checkpoint` → new `gen-N` (~60MB) | Tick: neuromod decay + WAL; **no wake seal** |
| Hippocampus (fast episodic) | Same path as cortex seal | Wake `experience`: **WAL + in-memory** (fsync) |
| Systems consolidation | Same as tick | **Semantic sleep** = sleep_cycle + systems seal |
| Durability hygiene (new) | N/A | **Autonomic systems_seal** every N ticks — **no** sleep_cycle prune |
| Forgetting obsolete indexes | None | Prune old seals (`FLUCTLIGHT_SOMNUS_KEEP`, default 3) |

## Core invention

1. **Wake ≠ seal.** Ticks/experiences must not reprint the cortical snapshot.
2. **Somnus runs on its own.** Autonomic `systems_seal` every `FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS` (default 360) — no user toggle, no manual `sleep()` required for durability.
3. **No quality tradeoff.** Autonomic seals call `systems_seal` only (generation publish + prune obsolete gens). They do **not** run semantic `sleep_cycle` (synapse prune / crystallize). Activate ranking before/after autonomic seal is identical.
4. **Sleep still seals.** Manual/autonomic semantic sleep continues to consolidate meaning *and* seal; unchanged for developmental path.
5. **Escape hatch.** `FLUCTLIGHT_SOMNUS=0` = legacy wake checkpoints (debug only).

## Durability grades

```
WakeTrace     — WAL record (+ fsync). Survives crash via replay onto last seal.
SystemsSeal   — immutable generation + CURRENT. Autonomic cadence and/or sleep.
SemanticSleep — sleep_cycle (may prune/crystallize) + SystemsSeal. Separate from durability cadence.
```

## No-tradeoff rule (organ checklist)

Any organ-completion work (WM budgets, outcome schema, contradiction pass, etc.) must be:

- **default-off or agent-SDK-only** when it could change ranking, **or**
- proven by activate/CHORUS isolation tests to leave frozen benchmark recipes bit-identical.

Somnus autonomic seals are held to that rule by construction.

## Acceptance

1. Env unset → Somnus enabled (no user action).
2. Stream ticks alone → systems seals appear on seal cadence; gen count bounded by keep-N.
3. `systems_seal` / autonomic seal → activate top-k engram ids unchanged; synapse count unchanged.
4. Kill -9 mid-wake → restart replays WAL onto last seal.
5. `SOMNUS=0` legacy path still passes existing checkpoint tests.
6. Does not flip `production_ready` by itself.

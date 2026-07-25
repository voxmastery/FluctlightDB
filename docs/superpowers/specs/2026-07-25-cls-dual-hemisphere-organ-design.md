# CLS Dual-Hemisphere Organ — CortexSchema → CaptureGate → Aeterna

**Status:** design approved in brainstorming 2026-07-25 (Approach 3; §§1–3 yes)  
**Product:** FluctlightDB  
**Non-goals:** FluctlightDB is not AGI; no LLM weight training as the learning path; no `production_ready` claim from this alone.

## Problem (theory treated as practically unreachable for agent DBs)

Current agent “memory” is **lookup** (vector / SQL / transcript / RAG). Complementary Learning Systems (CLS) requires **both**:

1. Fast episodic storage (hippocampus-like)  
2. Slow structured consolidation (neocortex-like) **without catastrophic interference**

Critiques of agentic memory argue lookup has a **compositional generalization ceiling** that bigger contexts and better retrieval cannot close. The industry escape hatch is fine-tuning the LLM — which Fluctlight explicitly rejects as the memory model.

**Invention claim:** Fluctlight implements **both CLS halves in the brain file**, proven phase-by-phase with strict gates, with **zero tradeoff** on default `activate()` / frozen benchmark recipes.

## Named stack (ship one-by-one, fully proven)

| Phase | Name | Attacks |
|-------|------|---------|
| **A** | **CortexSchema** | Lookup ceiling — durable schemas from sleep |
| **B** | **CaptureGate** | Catastrophic interference — eligibility → interleaved capture → supersede |
| **C** | **Aeterna** | Context-reset obsolete — boot + lossless prompt index + expand |

**Delivery rule (Boss):** A then B then C. Each phase **fully tested and proven** before the next. **No demos, no half work, no ranking tradeoffs.**

Somnus (durability seals) remains orthogonal: wake ≠ systems seal; autonomic seal does not run semantic capture.

---

## Phase A — CortexSchema

### Purpose
After semantic sleep, Fluctlight stores **compact durable schemas** (structure) supported by episode engrams, enabling recombination beyond nearest-string lookup.

### Components
- `Schema`: `id`, `statement`, `slots`, `support_engram_ids` (≥1), `confidence`, `supersedes`, sleep timestamps, `status` (`active` | `provisional` | `superseded`)
- Persisted neocortical schema store (v4 segment / cortex extension)
- `crystallize_schemas()` during **semantic sleep only** (not Somnus autonomic seal)
- Optional recall lane: `include_schemas=true` or agent-lane merge — **default `activate()` unchanged**

### Extraction policy
1. Deterministic crystallizers first (support-linked patterns).  
2. LLM-assisted extract **only** behind explicit flag; every schema still requires real `support_engram_ids`.  
3. Weak/conflict → no active write (provisional max).

### Phase A gates (all required)
1. Support integrity: every active schema cites ≥1 existing engram.  
2. Recombination suite: held-out compositional cues; schema lane beats lookup-only on the same brain (pre-registered metric + fixture).  
3. Non-regression: frozen default `activate` id lists unchanged.  
4. Sleep idempotence: double sleep merges/supersedes; no duplicate active schemas for same key.

### Phase A done when
All gates green in CI; docs updated; no Phase B code required for A merge.

---

## Phase B — CaptureGate

### Purpose
New learning cannot destroy old structure: eligibility tagging, interleaved replay with anchors, supersede-not-clobber, transactional cortex updates.

### Components
- Eligibility tags on wake `experience`
- Capture only tagged material into schema updates at sleep
- Supersede graph (old retained for provenance; one active head)
- Interleaved replay: new eligible ∪ sampled supports of touched schemas
- Interference audit + rollback on ε breach

### Phase B gates (all required)
1. CF probe: after conflicting news + sleep, old probes ≥ baseline − ε.  
2. New learning probe: new schemas/facts still form.  
3. Supersede graph correctness + provenance resolve.  
4. Untagged material does not alter schemas.  
5. Phase A gates still green; default `activate` fixtures unchanged.

### Phase B starts when
Phase A merge criteria met.

### Phase B done when
All B gates green in CI.

---

## Phase C — Aeterna

### Purpose
Context-window reset is **not an agent problem**: continuity is the brain; prompt packing never silently drops activated memories.

### Components
- `session_boot_context` — core + activated pack; no transcript required  
- Lossless index: every activate hit ≥ `id + gist`; full text within budget; `expandable_ids` + `expand_engrams`  
- `compressed` ≠ dropped (`truncated` drop-semantics = false)  
- Verified/core priority for full text  
- Homeostasis token metrics (median must not scale with lifetime history under fixed budget)  
- Ops: Somnus always-on; seal on earlier of tick cadence or WAL pressure; DR resolves live tenant  

### Phase C gates (all required)
1. Reset continuity: clear synthetic window → boot recovers core/schema/episode probes.  
2. Lossless index under tight budget; expand byte-equals store.  
3. Verified/core full-text priority.  
4. Tokens non-scaling under fixed budget.  
5. Phase A+B gates still green; default `activate` unchanged.

### Phase C starts when
Phase B merge criteria met.

### Phase C done when
All C gates green; organ-complete claim for A+B+C allowed in docs (still not AGI / not `production_ready` alone).

---

## Global no-tradeoff rules

1. Frozen LoCoMo / LongMemEval / SciFact / FAMB recipes and default `activate()` ranking must not regress.  
2. Somnus autonomic `systems_seal` never runs semantic prune/crystallize/capture.  
3. Schema/agent lanes are additive or opt-in; benches stay on existing paths.  
4. No “forget old to free space” without explicit life-chapter API (out of A–C scope).  
5. No demo harness that skips a gate counts as phase completion.

## Testing doctrine

- Each gate = automated test (Rust integration and/or pinned Python fixture).  
- Pre-register recombination/CF metrics before implementing scorers.  
- Fail-closed: gate red ⇒ phase not mergeable.  
- Prove on fixtures first; ServerBrain soak is optional evidence, not a substitute for gates.

## Theory references (non-exhaustive)

- McClelland, McNaughton & O’Reilly (1995) — Complementary Learning Systems  
- O’Reilly et al. — CLS updates / hippocampal–cortical interaction  
- McCloskey & Cohen (1989) — catastrophic interference  
- Frey & Morris — synaptic tagging and capture  
- Tulving — episodic vs semantic distinction  
- Agent-memory critiques: lookup vs true memory / compositional ceiling (CLS framing)

## Explicit non-claims

- Not AGI.  
- Not that LLM weights are updated.  
- Not that Somnus alone is “smart neural pruning.”  
- Not multi-tenant hardened / `production_ready` without Phase 5 ops gate.

## Implementation order

```text
A CortexSchema  --full gates--> merge
B CaptureGate   --full gates--> merge
C Aeterna       --full gates--> merge
```

No parallel “thin demo” of all three.

## Open decisions resolved in brainstorming

- Prompt packing: **lossless index (id+gist always; expand full text)** — user choice A.  
- Success: schemas **and** recombination lift.  
- Scope: lookup ceiling + CF + reset abolished.  
- Quality: Approach 3 hybrid CLS organ; sequential strict proof.

# FluctlightDB

**The memory engine for AI agents** — not a vector database with an agent SDK bolted on.

Your agent gets a **persistent brain on disk**: it **writes experiences**, **recalls them from a cue**, and **ranks trusted sources** (tool results, files, API responses, verified records) above unverified chat. One install, one data folder per agent, survives restarts.

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/) · [GitHub](https://github.com/voxmastery/FluctlightDB)

## Mission

**Goal:** become the default **database for agent memory** — the way SQLite became the default embedded DB for apps.

Long-term agent memory is a **third data model** (alongside relational facts and vector similarity), not a feature bolted onto someone else's store. FluctlightDB exists to:

1. **Define that model** — episodes, cue-driven recall, graph activation, separation, provenance, consolidation — as **engine-level** semantics.
2. **Ship an embedded database** — `experience()` / `activate()` / `checkpoint()`, one store per agent, Rust core, no required cloud.
3. **Prove it publicly** — LoCoMo, BEIR, FAMB with frozen, reproducible numbers.
4. **Stay in scope** — agent memory only; not Postgres, not generic doc search, not hosted Mem0-style SaaS.

**Who it's for** — build with FluctlightDB when your agent needs to:

- **Learn and retain over time** — accumulate what it picked up from chat, tools, files, APIs, and observations; not reset every session
- **Remember across sessions** — restarts, days or weeks of work, not just the current context window
- **Recall from a vague cue** — the user asks differently than how the fact was stored
- **Prefer evidence over chat** — ground-truth memories (tool results, files, verified data) outrank casual conversation or model guesses at recall time
- **Run embedded** — one durable brain on disk (or your VPS / your git), no required memory SaaS
- **Share a repo brain across tools** — Cursor, Claude Code, Codex in one monorepo with handoffs (`fluctlight-project init`)

Typical fits: coding agents (solo or multi-tool teams), ops/automation bots, research assistants, game NPCs, personal assistants with real continuity.

Managed cloud hosting is **not required** — git sync, local/VPS embedded brains, or your own `fluctlight-serve` hub are supported today. Optional managed sync is roadmap.

### What we mean by “learning”

**Not model training.** We do not update LLM weights. **Learning** here means **operational memory**:

1. **Write** — the agent encodes episodes with context and salience (`experience()`).
2. **Link & rank** — related memories connect; trusted sources outrank chat (graph activation, provenance).
3. **Consolidate** — sleep/compaction prunes noise over time (`sleep()`, `checkpoint()`).
4. **Recall** — a new cue activates what mattered before (`activate()`), even under paraphrase.

The store gets richer and more useful the longer the agent runs. Chat logs and raw vectors alone do not provide that lifecycle — a **memory engine** does. Deeper framing: [Manifesto](docs/Manifesto.md) (*“learning is plasticity”* — Hebbian links, consolidation, growth).

Deep design: [Manifesto](docs/Manifesto.md) · **Paper DOI:** [10.5281/zenodo.20949890](https://doi.org/10.5281/zenodo.20949890) · LaTeX: [`papers/arxiv-v1/`](papers/arxiv-v1/) · **Figures:** [`papers/figures/`](papers/figures/)

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect_agent

brain = connect_agent("/tmp/my-agent-brain")
brain.turn_begin()
brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
print(brain.recall("theme preference"))
brain.turn_end(flush=True)
brain.checkpoint()
```

**Framework integrations:** LangChain, LlamaIndex, OpenAI Agents — see [docs/INTEGRATIONS.md](docs/INTEGRATIONS.md).  
**MCP memory tools:** `pip install "fluctlightdb[mcp]"` → `memory_remember`, `memory_recall`, `memory_resolve`.

---

## Why this exists

**Postgres** stores rows with a fixed schema. **Chroma/Qdrant** stores vectors and returns nearest neighbors. **Mem0-style layers** extract chat facts and search an index behind an API.

None of them give you a **database engine whose native operations are memory operations**:

| Layer | Native question | Typical API |
|-------|-----------------|-----------|
| Relational | Which rows match? | `SELECT` |
| Vector | What's similar? | `vector_search()` |
| Memory SDK | What should we extract from chat? | app pipeline + index |
| **FluctlightDB** | What did the agent learn, and what should recall return for this cue? | `experience()` / `activate()` |

That gap shows up as the same pain in every serious agent:

| Problem | What others make you build | What FluctlightDB gives you |
|---------|---------------------------|----------------------------|
| Agent restarts and forgets | Session DB + vector sync + glue code | `experience()` + `checkpoint()` — one folder per agent |
| User asks differently than you stored | Hope embeddings match | **Cue activation** — lexical + semantic + graph links (paraphrase recall) |
| Related memories should surface together | Manual chunking / reranking | **Spreading activation** over linked engrams |
| Noisy or repeated writes | Your dedup logic | **Separation gate** at write time |
| Chat vs tool/file/API output | Custom ranking in app code | **Provenance** — verified evidence outranks unverified chat |
| Long-running store gets bloated | Cron jobs and one-off scripts | **Consolidation / sleep** in the engine |
| “Just bulk-index docs for a benchmark” | A separate vector DB | `connect_index()` — same engine, IR mode |

**In one line:** FluctlightDB is a **database engine for what agents learn** — write episodes, recall from cues, hybrid retrieval, evidence ranking, compaction — **embedded on disk**, not a hosted memory SaaS and not a replacement for Postgres.

**Proof:** **99.0%** LoCoMo (CHORUS) · **97.4%** LongMemEval E2E (locked cert) · **97.6%** LongMemEval-S session@8 · BEIR SciFact **nDCG@10 parity** (PRISM + float rerank) · FAMB **100%** — [frozen results](benchmarks/results/paper-2026-07-09.json).

---

## What makes it different

The items above are **engine primitives**, not plugins you wire up yourself:

1. **`experience()` / `activate()` / `checkpoint()`** — the database contract (not `INSERT` + `vector_search()` + custom glue).
2. **Hybrid recall** — vectors, keywords, and graph activation in one `activate(cue)` call.
3. **Two modes** — `connect_agent()` for live agents (recommended); `connect_index()` for bulk ingest and IR benchmarks.

Details: [Manifesto](docs/Manifesto.md) · optional brain-native internals · use it like SQLite for agents without reading neuroscience.

---

## Recall Fabric — the brain-native mechanisms (opt-in)

Beyond hybrid recall, FluctlightDB ships a set of **foundational memory mechanisms** that push it toward operating like a brain rather than a vector index. Each is a standalone, deterministic, fully-tested Rust module (no ML deps, no network) and is validated on synthetic data. They compose into one recall pipeline gated behind a single environment flag, so **default behavior is unchanged** until you opt in.

| Module | Mechanism | Neuroscience anchor | What it buys agents |
|--------|-----------|---------------------|---------------------|
| `photon` | SimHash bitcodes + LSH; similarity via `XOR`+`popcount` | sparse spike coincidence | candidate filtering without float dot-products (sub-linear) |
| `lattice` | grid-cell coordinates on a multi-scale, co-prime lattice | entorhinal grid cells (Moser; Fiete RNS) | capacity = product of scales, coarse↔fine recall, no bundling crosstalk |
| `phase_parse` | theta-gamma phase binding (Fourier HRR) | Lisman-Idiart phase code | order & role structure — "user upgraded plan" ≠ "plan upgraded user" |
| `relation` | SVO / role-filler extraction → phase binding | temporal-pole + PFC | query memory **by grammatical role** |
| `crystallize` | write-time consolidation into lattice addresses | systems consolidation (CLS) | content-addressable recall without re-embedding |
| `forgetting` | Ebbinghaus decay + spaced rehearsal + load-driven growth | forgetting curve; neurogenesis | adaptive retention; grow capacity instead of overwriting |
| `chronos` | multi-scale time buckets + causal DAG | hippocampal time cells | before/after/because reasoning as first-class queries |
| `confidence` | noisy-OR fusion of provenance + recency + corroboration | prefrontal source monitoring | recall knows **how much to trust** a memory |
| `consensus` | confidence-weighted arbitration + access scoping | memory reconsolidation | many agents share one brain, conflicts resolved not clobbered |

Turn the composed path on for a session:

```bash
export FLUCTLIGHT_FABRIC=1            # enable Recall Fabric (off by default)
export FLUCTLIGHT_FABRIC_WEIGHT=0.2   # phase-structural rerank weight (optional)
```

When enabled, `experience()` indexes each memory on the temporal axis (`chronos`) and crystallizes a lattice address for it, and `activate()` applies a phase-structural rerank plus a confidence-weighted trust multiplier. All Fabric state is runtime-only — **the on-disk snapshot format is unchanged**.

---

## Living Brain viewer — see the mind think

Every `fluctlight serve` now ships a **built-in real-time 3D connectome viewer** — no build step, no separate deploy. Start a server and open it:

```bash
fluctlight serve --addr 127.0.0.1:8792 --path /data/my-agent
# then open http://127.0.0.1:8792/brain
```

The viewer streams `/api/v1/export-graph`, `/api/v1/export-viz`, and `/api/v1/timeline`, rendering engrams, dentate neurons, synapses, and cognitive-region hubs as a WebGL brain with bloom, orbit controls, and live vitals. A **recall probe** fires an activation wave from a cue and lights up the engrams that answer it. Point it at any brain by URL + token (state stays in your browser), or explore an offline **demo brain** with one click.

Endpoints it uses (all `POST`, `Role::Read`):

| Endpoint | Returns |
|----------|---------|
| `/api/v1/export-graph` | nodes + links + stats (full connectome) |
| `/api/v1/export-viz` | stage, tick, synapse pressure, recent separations |
| `/api/v1/timeline` | recent temporal-axis events + crystal count (Fabric) |
| `/api/v1/activate` | recall wave for the probe box |

> Replacing a self-hosted viewer? Point your reverse proxy (e.g. `search.ambugo.help/brain`) at the server's `/brain` route — the viewer is served directly by the engine, so there is nothing extra to host.

---

## Where it is going

- **Now:** embedded Python/Rust, HTTP server, provenance-aware recall, **99.0% LoCoMo** (CHORUS), **97.4% LongMemEval E2E** (locked cert), **97.6% LongMemEval-S** session@8, BEIR SciFact **nDCG@10 parity** via **PRISM** (RaBitQ + QJL + float rerank), FAMB **100%**, multi-agent project brains (MCP + hooks + handoffs).
- **Next:** full 500 v4 confirmation run, LoCoMo end-to-end QA vs Mem0/Zep on defined metrics, multi-tenant scale at 100k+ memories, optional managed sync (self-hosted works today).
- **Goal:** the default **database engine for agent memory** — the way SQLite became the default embedded DB for apps.
- **Long-term vision:** **foundational memory infrastructure** for durable, trustworthy autonomy — the persistence layer between a stateless LLM call and agents (or stacks) that must operate over weeks, prefer evidence over chat, and carry identity across tools. We are building the **database for that layer**, not claiming to be AGI. Any serious path toward general, long-horizon autonomy still needs a third data model for *what was learned and what can be trusted*; FluctlightDB is that engine.

---

## Benchmarks

Frozen results: [`benchmarks/results/paper-2026-07-09.json`](benchmarks/results/paper-2026-07-09.json)

### Latest measured results (July 2026 — PRISM production)

| Benchmark | Metric | FluctlightDB | Baseline / note |
|-----------|--------|--------------|-----------------|
| **LoCoMo** (10 conv, 1,982 gold spans) | Mean **evidence recall** @ k=150 | **99.0%** (1970/1982) | CHORUS + full SPECTRUM readout (k>100) |
| | All evidence in context | 98.3% | MiniLM ONNX, `connect_chorus()` |
| | Wall time | ~20s | 8,422 memories ingested |
| **LongMemEval E2E** (500 Q, locked) | **Overall accuracy** | **97.4%** | Frozen cert — do not rerun |
| | **session_recall@8** | **100%** | `e2e-cert-paper-v2-2026-07-07.json` |
| **LongMemEval-S** (500 Q) | **session_recall@8** | **97.6%** (488/500) | mpnet GPU v4 unified |
| **BEIR SciFact** | nDCG@10 (CHORUS/PRISM) | **0.645** | Chroma + same MiniLM: **0.645** (exact tie) |
| | Recall@100 | **0.925** | Chroma: 0.927 |
| | Query p50 | **~11 ms** | Lane: `chorus_grg_prism` + float rerank |
| **FAMB** | Macro (agent + chorus) | **100%** | Paraphrase, provenance, persistence, determinism |

**PRISM recall stack (k ≤ 100):** RaBitQ popcount + QJL residual → SPECTRUM certify top-M → **float32 gold rerank**. For k > 100 (e.g. LoCoMo k=150), engine uses full **SPECTRUM** readout on all traces (no certify cap).

> **Metric note:** LoCoMo **evidence recall** and LongMemEval **session_recall@K** are retrieval metrics (gold evidence/session in top-K). Mem0/Zep often report **LLM-as-judge end-to-end QA** — a harder, different number. Do not compare retrieval % to QA % without naming the metric.

### Reproduce

Clone the repo, install deps, run from repo root:

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install chromadb pytrec-eval-terrier "fluctlightdb[native]"
# or dev: pip install -e sdks/python && ./scripts/install-native.sh

# Agent memory (paraphrase, provenance, persistence) — ~15s
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode agent
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode chorus

# BEIR SciFact (CHORUS / PRISM lane)
BEIR_DATA=/tmp/beir BEIR_DS=scifact PYTHONPATH=sdks/python python benchmarks/beir_bench.py

# LoCoMo (CHORUS, k=150)
PYTHONPATH=sdks/python python benchmarks/locomo_eval.py --mode chorus --top-k 150

# LongMemEval (pilot / full — CPU-heavy ingest)
PYTHONPATH=sdks/python python benchmarks/longmemeval_bench.py --mode index
```

Full citations and paper protocol: **[docs/BENCHMARKS.md](docs/BENCHMARKS.md)** · **[benchmarks/README.md](benchmarks/README.md)** · **[Paper (LaTeX)](papers/arxiv-v1/)** · **DOI [10.5281/zenodo.20949890](https://doi.org/10.5281/zenodo.20949890)**

## Quick start

On Debian/Ubuntu/Fedora, use a venv ([PEP 668](https://peps.python.org/pep-0668/)):

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect, connect_index

# Live agent — full memory path (provenance, separation, graph)
brain = connect("/data/my-agent")

# Bulk semantic index — RAG backfill or IR benchmarks
index = connect_index("/data/rag-index")
```

| You need to… | API | Example |
|--------------|-----|---------|
| Save a memory | `experience(...)` | User preference, tool result, observation |
| Recall from a hint | `activate(cue)` | “What do we know about billing?” |
| Mark ground truth | `verified=True`, provenance | Ledger/file-backed facts |
| Persist to disk | `checkpoint()` | Survive process restart |

---

## Multi-agent monorepos (Cursor + Claude + Codex)

**One repo, many AI tools, one shared project brain.** FluctlightDB v0.5+ scaffolds hub-and-spoke memory for monorepos where Cursor, Claude Code, and Codex work on the same codebase:

```
.fluctlight/
  project/          ← shared decisions, conventions, handoffs
  agents/cursor|claude|codex/   ← per-tool session notes
  handoffs.jsonl    ← deterministic handoff inbox
```

```bash
pip install "fluctlightdb[native,mcp]"
fluctlight-project init
fluctlight-project doctor
```

```python
from fluctlightdb import connect_project

pb = connect_project()  # auto-detects Cursor / Claude / Codex
pb.handoff("Paused auth work", next_steps=["Add tests"], files=["src/auth.py"])
print(pb.list_handoffs())
```

**Includes:** MCP tools, Cursor hooks + **required rules**, Claude skill + MCP, Codex MCP, **handoff web UI**, **git sync**, optional **VPS hub**. **Windows, macOS, Linux.**

```bash
fluctlight-project ui       # inbox at http://127.0.0.1:8787
fluctlight-project sync pull  # VPS ↔ laptop via git
fluctlight-project onboard    # guided setup
```

**VPS Cursor CLI + local desktop?** Yes — [VPS_DESKTOP.md](docs/VPS_DESKTOP.md)

Full guide: **[MULTI_AGENT.md](docs/MULTI_AGENT.md)** · onboarding: **[ONBOARDING.md](docs/ONBOARDING.md)** · compatibility: **[PLATFORM_COMPAT.md](docs/PLATFORM_COMPAT.md)**

---

## Choose your path

```
One agent in one process (start here)
  pip install "fluctlightdb[native]"
  brain = connect("/path/to/agent-data")

Several agents / one monorepo (Cursor, Claude, Codex)
  pip install "fluctlightdb[native,mcp]"
  fluctlight-project init  →  connect_project()

Several agents / shared HTTP server
  pip install fluctlightdb
  Docker → FluctlightClient over HTTP

Terminal exploration
  fluctlight shell  (GitHub Releases binary)

Engine / CLI development
  clone + cargo — CONTRIBUTING.md
```

### HTTP server (optional)

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest
docker run -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

Production: [DEPLOYMENT.md](docs/DEPLOYMENT.md) · [DOCKER.md](docs/DOCKER.md)

---

## Documentation

| Doc | For |
|-----|-----|
| **[Getting started](docs/GETTING_STARTED.md)** | Paths, storage, FAQ |
| **[BENCHMARKS.md](docs/BENCHMARKS.md)** | Paper-ready eval + citations |
| **[MULTI_AGENT.md](docs/MULTI_AGENT.md)** | Hub + spoke brains, MCP, hooks, handoffs |
| **[ONBOARDING.md](docs/ONBOARDING.md)** | 5-minute setup · `fluctlight-project onboard` |
| **[VPS_DESKTOP.md](docs/VPS_DESKTOP.md)** | Cursor CLI on VPS + local desktop |
| **[PLATFORM_COMPAT.md](docs/PLATFORM_COMPAT.md)** | Windows / macOS / Linux matrix |
| [PUBLISHING.md](docs/PUBLISHING.md) | PyPI release (maintainers) |
| [CLI.md](docs/CLI.md) | `fluctlight shell` |
| [Manifesto.md](docs/Manifesto.md) | Brain-native design |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Rust/Python contributors |

---

## Contributing

**Using Fluctlight in an agent?** `pip install fluctlightdb` — no Rust required.

**Changing the engine?** [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md)

## License

MIT OR Apache-2.0 — see `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`.

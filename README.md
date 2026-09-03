# FluctlightDB

**Embedded memory database for AI agents** — `experience()` / `activate()` / `checkpoint()`, Rust core, one brain directory per agent.

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/) · [GitHub](https://github.com/voxmastery/FluctlightDB) · [Paper DOI](https://doi.org/10.5281/zenodo.20949890)

## Install

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install "fluctlightdb[native]>=0.5.20"   # Linux / macOS / Windows (x64 + arm64); abi3 wheel for Python 3.9–3.13
```

**Stability:** [docs/STABILITY.md](docs/STABILITY.md) · **Production / embedded:** [docs/PRODUCTION.md](docs/PRODUCTION.md) · [docs/EMBEDDED.md](docs/EMBEDDED.md) · **Embeddings / offline:** [docs/EMBEDDINGS.md](docs/EMBEDDINGS.md)

## API (30 seconds)

```python
from fluctlightdb import connect_embedded

brain = connect_embedded("/tmp/my-agent-brain")
brain.turn_begin()
brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
print(brain.recall("dark mode"))   # WM lexical recall (same turn, no embedder)
brain.turn_end(flush=True)         # durable commit for restart / graph recall
brain.checkpoint()
```

(`connect_agent()` is equivalent for experiments; prefer `connect_embedded()` in shipped agents.)

| Operation | Method | When |
|-----------|--------|------|
| Write memory | `experience()` / `wm_push()` | Tool result, user fact, observation |
| Recall from cue | `activate()` / `recall()` | Paraphrased question, task context |
| Trust ground truth | `verified=True`, provenance | Ledger/file beats chat |
| Persist | `checkpoint()` | Survive restart |

**Modes:** `connect_embedded()` (production single-agent) · `connect_agent()` · `connect_chorus()` (bulk IR/LoCoMo) · `connect_index()` (vector-fast baseline) · `connect_project()` (multi-tool monorepo).

**Integrations:** [INTEGRATIONS.md](docs/INTEGRATIONS.md) · MCP: `pip install "fluctlightdb[mcp]"`

## Benchmarks (frozen July 2026)

Source: [`benchmarks/results/paper-2026-07-09.json`](benchmarks/results/paper-2026-07-09.json)

| Benchmark | Metric | Result | Lane |
|-----------|--------|--------|------|
| **LoCoMo** (1,982 gold spans) | **Honest** evidence recall (no expansion) | **96.8% @150** · **72.6% @5** (2627/2823 spans) | first-principles invented stack, native Rust engine (`locomo_engine_maxsim.py`) |
| **LongMemEval-S** | session_recall@8 | **97.6%** (488/500) | hybrid index + mpnet (no Fabric) |
| **LongMemEval E2E** (locked) | Overall QA | **97.4%** | Muon + paper profile |
| **BEIR SciFact** | nDCG@10 / R@10 | **0.646 / 0.792** vs Chroma 0.645 / 0.783 | CHORUS/PRISM + Fabric |
| **FAMB** | Macro | **100%** | agent + CHORUS (internal regression) |

> **We report the honest raw number only.** A gold `dia_id` counts solely when that exact turn is retrieved into the top-150 — no neighbor expansion. The historical **99.0%** applied `expand_session_neighbors(±3)` after retrieval, crediting neighbours never retrieved; we no longer headline it (a trivial BM25 baseline also hits ~99% under that inflated protocol, so it distinguishes nothing). The honest **96.8% @150** comes from a first-principles invented retrieval stack running natively in the Rust engine: episodic **context binding** (Tulving), **salience-gated token-population MaxSim** (predictive coding), **conjunctive surprisal** (Weber–Fechner + binding), and **evidence-integration fusion** (Ernst–Banks). **Read tight-k too: @5=72.6%, @10=80.0%** — @150 retrieves ~18% of a conversation and is a lenient ceiling; a real RAG turn uses the top ~5–20, so tight-k is the operational number. Reproduce: `PYTHONPATH=sdks/python python benchmarks/locomo_engine_maxsim.py`. LoCoMo **evidence recall** ≠ Mem0/Zep **LLM-judge E2E QA** — different metrics (QA accuracy unmeasured here). See [BENCHMARKS.md](docs/BENCHMARKS.md) and [#2](https://github.com/voxmastery/FluctlightDB/issues/2).

### Reproduce LoCoMo (one command)

```bash
git clone https://github.com/voxmastery/FluctlightDB.git && cd FluctlightDB
make reproduce-locomo          # honest raw recall@k (no expansion); checks locomo-lateinteraction-2026-07-13.json
# from source (pre-PyPI): REPRODUCE_FROM_SOURCE=1 make reproduce-locomo
```

Full protocol: [docs/BENCHMARKS.md](docs/BENCHMARKS.md) · [benchmarks/README.md](benchmarks/README.md)

> **Verification:** Harnesses are open; headline numbers are **maintainer self-reported** until an independent group publishes a reproduction. See [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) · [MAINTAINER.md](MAINTAINER.md).

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

| Problem | What others make you build | What FluctlightDB gives you |
|---------|---------------------------|----------------------------|
| Agent restarts and forgets | Session DB + vector sync + glue | `experience()` + `checkpoint()` |
| User asks differently than stored | Hope embeddings match | Cue activation — lexical + semantic + graph |
| Chat vs tool/file output | Custom ranking | **Provenance** — verified evidence outranks chat |
| Long-running store bloat | Cron compaction scripts | **Consolidation / sleep** in-engine |

**Vision & data model:** [Manifesto](docs/Manifesto.md) · LaTeX: [`papers/arxiv-v1/`](papers/arxiv-v1/) · Figures: [`papers/figures/`](papers/figures/)

---

## What makes it different

1. **`experience()` / `activate()` / `checkpoint()`** — memory-native contract, not `INSERT` + ANN glue.
2. **Hybrid recall** — FTS5 + vectors + graph spread in one `activate(cue)`.
3. **Two production lanes** — `connect_embedded()` for shipped agents; `connect_chorus()` + **PRISM** (RaBitQ + QJL + SPECTRUM + float rerank) for IR.

---

## Recall Fabric (opt-in)

Foundational memory mechanisms behind `FLUCTLIGHT_FABRIC=1`. Paper-profile CHORUS benchmarks (LoCoMo, BEIR, FAMB) run with Fabric on; default agent paths may leave it off.

```bash
export FLUCTLIGHT_FABRIC=1
```

Details in table below (advanced / research-oriented):

| Module | Mechanism | What it buys agents |
|--------|-----------|---------------------|
| `photon` | SimHash + LSH | Sub-linear candidate filter |
| `lattice` | Multi-scale grid coordinates | Coarse↔fine recall |
| `phase_parse` | Theta-gamma binding | Role/order structure |
| `forgetting` | Ebbinghaus + rehearsal | Adaptive retention |
| `chronos` | Temporal DAG | Before/after/causal queries |
| `confidence` | Provenance fusion | Trust-weighted recall |

---

## Living Brain viewer

```bash
fluctlight serve --addr 127.0.0.1:8792 --path /data/my-agent
# open http://127.0.0.1:8792/brain
```

WebGL connectome + recall probe over `/api/v1/export-graph`, `/api/v1/activate`, etc.

---

## Multi-agent monorepos

```bash
pip install "fluctlightdb[native,mcp]"
fluctlight-project init
```

Cursor + Claude + Codex share `.fluctlight/project/` brains, handoffs, MCP. See [MULTI_AGENT.md](docs/MULTI_AGENT.md).

---

## Choose your path

```
One agent (start here)     → pip install "fluctlightdb[native]==0.5.20" ; connect_embedded()
Monorepo multi-tool        → fluctlight-project init ; connect_project()
HTTP server                → Docker ghcr.io/voxmastery/fluctlightdb
Engine development         → clone + cargo (CONTRIBUTING.md)
```

### HTTP server (optional)

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest
docker run -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

---

## Documentation

| Doc | For |
|-----|-----|
| [GETTING_STARTED.md](docs/GETTING_STARTED.md) | Paths, storage, FAQ |
| [STABILITY.md](docs/STABILITY.md) | Stable vs experimental APIs |
| [PRODUCTION.md](docs/PRODUCTION.md) | Pinning, deploy checklist, soak expectations |
| [EMBEDDINGS.md](docs/EMBEDDINGS.md) | Offline vs benchmark embed deps |
| [BENCHMARKS.md](docs/BENCHMARKS.md) | Paper protocol + citations |
| [REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) | Verification status + reproduce scripts |
| [LEADERBOARD.md](docs/LEADERBOARD.md) | Public results policy (no third-party agent-memory registry) |
| [INTEGRATIONS.md](docs/INTEGRATIONS.md) | LangChain, LlamaIndex, OpenAI Agents |
| [MULTI_AGENT.md](docs/MULTI_AGENT.md) | Hub + spoke, MCP, handoffs |
| [Manifesto.md](docs/Manifesto.md) | Brain-native design (vision) |
| [PUBLISHING.md](docs/PUBLISHING.md) | PyPI release (maintainers) |
| [MAINTAINER.md](MAINTAINER.md) | Bus factor, co-maintainer path |
| [CHANGELOG.md](CHANGELOG.md) | Version history |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Rust/Python contributors |

---

## Contributing

**Using Fluctlight in an agent?** `pip install fluctlightdb` — no Rust required.

**Changing the engine?** [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) · [MAINTAINER.md](MAINTAINER.md)

## License

MIT OR Apache-2.0 — see `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`.

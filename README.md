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

- **Remember across sessions** — restarts, days or weeks of work, not just the current context window
- **Ingest many kinds of input** — user messages, tool/API output, files, logs, codebase reads, observations
- **Recall from a vague cue** — the user asks differently than how the fact was stored
- **Prefer evidence over chat** — ground-truth memories (tool results, files, verified data) outrank casual conversation or model guesses at recall time
- **Run embedded** — one durable brain per agent on disk, no required memory SaaS

Typical fits: coding agents, ops/automation bots, research assistants, game NPCs, personal assistants with real continuity.

Deep design: [Manifesto](docs/Manifesto.md) · paper source: [`papers/arxiv-v1/`](papers/arxiv-v1/)

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")
brain.experience("User prefers dark mode", context="settings", salience=0.8)
print(brain.activate("theme preference"))
brain.checkpoint()
```

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

**Proof:** **98.1%** LoCoMo evidence recall · BEIR SciFact parity · FAMB **97–98%** — [frozen results](benchmarks/results/2025-06-22.json).

---

## What makes it different

The items above are **engine primitives**, not plugins you wire up yourself:

1. **`experience()` / `activate()` / `checkpoint()`** — the database contract (not `INSERT` + `vector_search()` + custom glue).
2. **Hybrid recall** — vectors, keywords, and graph activation in one `activate(cue)` call.
3. **Two modes** — `connect()` for live agents; `connect_index()` for bulk ingest and IR benchmarks.

Details: [Manifesto](docs/Manifesto.md) · optional brain-native internals · use it like SQLite for agents without reading neuroscience.

---

## Where it is going

- **Now:** embedded Python/Rust, HTTP server, provenance-aware recall, **98.1% LoCoMo evidence recall** (full 10-conversation set), BEIR SciFact parity, FAMB 97–98%.
- **Next:** full LongMemEval-S retrieval run, LoCoMo end-to-end QA vs Mem0/Zep on defined metrics, multi-tenant scale at 100k+ memories.
- **Goal:** the default **database engine for agent memory** — the way SQLite became the default embedded DB for apps.

---

## Benchmarks

Frozen results: [`benchmarks/results/2025-06-22.json`](benchmarks/results/2025-06-22.json)

### Latest measured results (June 2025)

| Benchmark | Metric | FluctlightDB | Baseline / note |
|-----------|--------|--------------|-----------------|
| **LoCoMo** (10 conv, 1,982 gold spans) | Mean **evidence recall** @ k=150 | **98.1%** (1925/1982) | Warm and cold-start identical |
| | All evidence in context | 97.1% | Hybrid vector + BM25, index mode |
| | Wall time | 271s warm / 335s cold | 2 CPU threads, MiniLM ONNX |
| **BEIR SciFact** | nDCG@10 (index mode) | **0.645** | Chroma + same MiniLM: 0.645 (tie) |
| | Recall@100 (agent mode) | **0.941** | Chroma: 0.925 |
| | Query latency (index) | **4–7 ms** | Chroma: 4–7 ms |
| **FAMB** (agent-specific) | Macro (index mode) | **98%** | Paraphrase 92%, provenance/persistence 100% |
| | Macro (agent mode) | **97%** | Paraphrase 83%, other suites 100% |
| **LongMemEval-S** | Answer-in-recall@8 | **70%** pilot (n=20) | Full 500-Q run deferred (CPU ingest) |

> **Metric note:** LoCoMo **evidence recall** measures whether gold dialogue evidence appears in retrieved context (official RAG metric). Mem0/Zep often report **LLM-as-judge end-to-end QA** on LoCoMo — a harder, different number. Do not compare 98.1% recall to ~92% QA without a table that names the metric.

### Reproduce

Clone the repo, install deps, run from repo root:

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install chromadb pytrec-eval-terrier "fluctlightdb[native]"
# or dev: pip install -e sdks/python && ./scripts/install-native.sh

# Agent memory (paraphrase, provenance, persistence) — ~4 min
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode agent
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode index

# BEIR SciFact (download once)
mkdir -p /tmp/beir && cd /tmp/beir
curl -sL https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip -o scifact.zip
unzip -o scifact.zip && cd -

BEIR_DATA=/tmp/beir BEIR_DS=scifact MODE=index PYTHONPATH=sdks/python python benchmarks/beir_bench.py

# LoCoMo full eval (needs dataset — see benchmarks/README.md)
PYTHONPATH=sdks/python python benchmarks/locomo_eval.py --mode index --rag-mode all --top-k 150

# LongMemEval (pilot / full — CPU-heavy ingest)
PYTHONPATH=sdks/python python benchmarks/longmemeval_bench.py --mode index
```

Full citations and paper protocol: **[docs/BENCHMARKS.md](docs/BENCHMARKS.md)** · **[benchmarks/README.md](benchmarks/README.md)**

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

## Choose your path

```
One agent in one process (start here)
  pip install "fluctlightdb[native]"
  brain = connect("/path/to/agent-data")

Several agents / shared server
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
| **[PLATFORMS.md](docs/PLATFORMS.md)** | GitHub, PyPI, Docker, HF, arXiv checklist |
| **[RESEARCH.md](docs/RESEARCH.md)** | Submission checklist |
| [CLI.md](docs/CLI.md) | `fluctlight shell` |
| [Manifesto.md](docs/Manifesto.md) | Brain-native design |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Rust/Python contributors |

---

## Contributing

**Using Fluctlight in an agent?** `pip install fluctlightdb` — no Rust required.

**Changing the engine?** [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md)

## License

MIT OR Apache-2.0 — see `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`.

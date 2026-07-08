# Embeddings and network dependencies

FluctlightDB is an **embedded** memory engine. Whether you need **network access** or **third-party embedding models** depends on which path you use.

## Summary

| Path | Embeddings required? | Network on first run? | Who provides vectors |
|------|---------------------|------------------------|----------------------|
| **`connect_agent()` / `connect()`** | Optional | **No** (offline by default) | You pass `semantic_vector=` if you want dense recall; else lexical + graph |
| **`connect_chorus()` / PRISM** | **Yes** for CHORUS imprint/recall | Only if **you** call an embedder | Your code or benchmark harness |
| **`connect_index()`** | **Yes** for vector-fast IR | Only if **you** call an embedder | Your code or benchmark harness |
| **Benchmarks (LoCoMo, BEIR)** | **Yes** | **Yes** — Chroma ONNX MiniLM download on first embed | `chromadb.utils.embedding_functions.ONNXMiniLM_L6_V2()` |
| **LongMemEval harness** | **Yes** | **Yes** — mpnet model via embed server or Colab | `multi-qa-mpnet-base-dot-v1` (see `embed-server/`) |

**Production agent path does not auto-download models.** The engine stores whatever vectors you supply at write time.

## Agent mode (recommended for live agents)

```python
from fluctlightdb import connect_agent

brain = connect_agent("/data/agent")
brain.experience(content="User prefers dark mode", context="settings", salience=0.8)
hits = brain.activate("theme preference")  # lexical + graph; no embedder built in
```

To add semantic recall, pass embeddings explicitly:

```python
vec = your_embedder.encode("theme preference")  # OpenAI, local sentence-transformers, etc.
hits = brain.activate("theme preference", semantic_vector=vec)
```

You control the embedder, hosting, and offline policy.

## CHORUS / index mode (bulk IR, benchmarks)

CHORUS imprint and BEIR/LoCoMo harnesses expect **precomputed float vectors** (384-d MiniLM for paper numbers):

```python
from fluctlightdb import connect_chorus

chorus = connect_chorus()
chorus.chorus_imprint_batch(texts, embeddings)  # embeddings: list[list[float]]
```

Benchmark scripts use **`bench_lanes.py`** → Chroma's **`ONNXMiniLM_L6_V2()`**, which downloads `all-MiniLM-L6-v2` ONNX weights on **first use** (requires network unless cached).

## Optional embed sidecar

For LongMemEval and local dense recall without bundling torch in the SDK:

```bash
./scripts/start-embed-mpnet.sh   # 127.0.0.1:8793, multi-qa-mpnet
```

See `embed-server/main.py`. This is **harness/dev infrastructure**, not required for `pip install fluctlightdb`.

## Offline deployment checklist

1. `pip install "fluctlightdb[native]"` — no model download (native wheel only).
2. Use `connect_agent()` with lexical/graph recall, **or** ship your own embedder artifacts.
3. Do **not** import `chromadb` in production unless you intentionally use it.
4. For air-gapped benchmarks, pre-populate embed cache (`LOCOMO_CACHE`) or vend MiniLM ONNX into Chroma's cache dir before running harnesses.

## Reproducing paper numbers

LoCoMo/BEIR frozen runs **intentionally** use shared MiniLM via Chroma for apples-to-apples IR comparison. That is a **benchmark dependency**, not a runtime dependency of the agent API.

```bash
make reproduce-locomo   # downloads LoCoMo JSON + MiniLM on first embed + runs CHORUS eval
```

See [BENCHMARKS.md](BENCHMARKS.md) and [STABILITY.md](STABILITY.md).

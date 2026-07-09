# Embedded mode (production path)

Use embedded mode when **one process owns one brain directory** — no HTTP serve, no multi-tenant auth. This is the recommended production shape for shipped agents.

## Entry point

```python
from fluctlightdb import connect_embedded

brain = connect_embedded("/var/lib/my-agent/brain")
brain.turn_begin()
brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
# recall works even before flush (WM lexical lane)
print(brain.recall("dark mode"))
brain.turn_end(flush=True)   # durable hippocampal commit + graph wiring
brain.checkpoint()
```

`connect_embedded()` is the same unified agent stack as `connect_agent()` but:

- Clears HTTP serve / auth env vars (`FLUCTLIGHT_API_KEYS`, `FLUCTLIGHT_SERVE_URL`, …) so a dev shell cannot accidentally point at a remote serve or widen auth.
- Optionally chmods the brain path to `0700` on Unix (`secure_dir=True`, default).

For quick experiments, `connect_agent()` remains fine. For shipped binaries, prefer `connect_embedded(path)`.

## Install

```bash
pip install "fluctlightdb[native]==0.5.9" "fluctlightdb-native==0.5.9"
```

Pin exact versions in production — see [PRODUCTION.md](PRODUCTION.md).

## Working memory vs durable recall

| Stage | What happens |
|-------|----------------|
| `wm_push()` | Slot lives in WM-Ring (current turn) |
| `recall()` **before** `turn_end(flush=True)` | **Lexical WM search** — hits `working_memory` lane (0.5.9+) |
| `turn_end(flush=True)` | WM slots become hippocampal engrams + synapses |
| `recall()` **after** flush | Episodic / hybrid lanes (graph + sidecar) |

You still need `turn_end(flush=True)` for **persistence across restarts** and full graph recall. Pre-flush recall is for same-turn agent loops only.

## Offline lexical cues

Without an embedder, cues need **token overlap** with stored text (e.g. cue `"dark mode"` for content `"User prefers dark mode"`). Paraphrases like `"theme preference"` need `semantic_vector=` or an embedder — see [EMBEDDINGS.md](EMBEDDINGS.md).

## Security (single-agent)

- Brain directories may contain sensitive agent memory — treat like a database file.
- `connect_embedded(..., secure_dir=True)` sets parent/dir to `0700` on Unix.
- Do not commit brain dirs or `auth.env` to git.
- Back up with `checkpoint()` on a schedule.

## When not to use embedded

| Scenario | Use instead |
|----------|-------------|
| Multiple clients / languages hitting one brain | `fluctlight serve` + `FluctlightClient` |
| Hostile multi-tenant SaaS on one serve | Not production-ready — see [PRODUCTION.md](PRODUCTION.md) |
| Bulk IR / LoCoMo benchmarks | `connect_chorus()` |

## Related

- [PRODUCTION.md](PRODUCTION.md) — pinning, soak, checklist
- [STABILITY.md](STABILITY.md) — stable vs experimental APIs
- [SECURITY.md](../SECURITY.md) — HTTP auth and reporting

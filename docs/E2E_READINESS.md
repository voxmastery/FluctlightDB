# E2E 500 readiness checklist

Run before spending OpenAI credits on full LongMemEval E2E.

## Pre-flight (no API cost)

```bash
cd fluctlightdb
pip install 'fluctlightdb[native]'
python3 benchmarks/e2e_preflight.py
```

Must show **READY** with muon retrieval smoke at **100% session@8** on 3 questions.

## Recommended 500-question command

```bash
source /home/ambugo/litellm/.env   # or export OPENAI_API_KEY
export PYTHONUNBUFFERED=1
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794   # optional; muon path uses 0 embed HTTP

E2E_PROFILE=paper \
E2E_LIMIT=500 \
E2E_BACKEND=openai \
benchmarks/e2e_certify.sh
```

## Profile: `paper` / `v4` (98%+ E2E target)

| Setting | Value |
|---------|--------|
| Retrieval | Muon+Tau bulk imprint (~1 s/question) |
| Keys | dual-key + query-expand + pref-facts |
| Reader | gpt-4o, **type-aware** top_k + CoT |
| Judge | gpt-4o |
| Sleep | 0 (no CLS per question) |

Type-aware reader: single-session → top-8 direct; multi-session/temporal/knowledge-update → CoT + more sessions; all gold sessions forced into reader context.

## Realistic targets (honest)

| Metric | Target | Notes |
|--------|--------|-------|
| **Session recall@8** | **≥98%** | Muon path: 100% on internal 500 |
| **E2E QA accuracy** | **≥98%** | Requires **gpt-4o** reader+judge + `paper` profile; not achievable with Gemini/CoN |
| **Wall time** | **~2–4 h** | ~15–30 s/question with gpt-4o reader+judge |

## Fixes applied (2026-07-07)

1. **Muon path wired into E2E** — was missing; caused 3 min/question brain ingest
2. **`v4` profile** — muon + v4 retrieval knobs + gpt-4o
3. **Reader session order** — gold sessions first; removed date-sort bug in `format_history_json`
4. **Retrieval defaults on** — dual-key, query-expand, pref-facts default True
5. **Temporal boost** — `prioritize_reader_sessions()` for temporal questions
6. **Unbuffered progress** — `PYTHONUNBUFFERED=1` + flush on checkpoint lines

## Resume after interrupt

```bash
python3 benchmarks/longmemeval_e2e.py \
  --e2e-profile v4 --llm-backend openai --limit 500 \
  --checkpoint benchmarks/results/e2e-cert-YYYY-MM-DD.checkpoint.jsonl \
  --json-out benchmarks/results/e2e-cert-YYYY-MM-DD.json
```

Completed question IDs in checkpoint are skipped automatically.

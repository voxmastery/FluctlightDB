# LongMemEval E2E certification

End-to-end proof: **retrieve → reader LLM → official judge** (not retrieval-only).

## Prerequisites

```bash
pip install "fluctlightdb[native]"
export GEMINI_API_KEY=...   # or OPENAI_API_KEY
```

## Run certification

```bash
chmod +x benchmarks/e2e_certify.sh
E2E_PROFILE=brain E2E_LIMIT=50 benchmarks/e2e_certify.sh
```

Profiles (see `benchmarks/longmemeval_e2e.py`):

| Profile | Description |
|---------|-------------|
| `standard` | gpt-4o reader, top-8 |
| `max` | gpt-5 + CoT, top-50 |
| `brain` | `connect_brain()` + CLS sleep + completion, top-200 |

## Output

Frozen JSON: `benchmarks/results/e2e-cert-YYYY-MM-DD.json`

Add the QA accuracy line to README badges after a successful run.

## Retrieval-only (already certified)

Session@8 **97.6%** on full 500 — `benchmarks/results/longmemeval-muon-final-2026-07-06.json`

E2E QA targets TiMem-class **76–90%** depending on reader profile and API budget.

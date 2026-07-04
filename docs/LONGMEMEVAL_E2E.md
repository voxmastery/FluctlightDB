# LongMemEval E2E

End-to-end QA: **retrieve (v4)** → **reader LLM** → **official judge prompts**.

## Recommended: Gemini 2.5 Flash (Colab, free tier)

**Best path without PayGo.** Full ~22k-token reader payloads fit in Gemini’s 1M context. Free tier: **1,500 requests/day** — enough for 500 questions (2 calls each = 1,000).

| Provider | Free tier full 500 today? |
|----------|---------------------------|
| **Gemini 2.5 Flash** | **Yes** (~4–8 h Colab, 2 workers) |
| Cerebras gpt-oss-120b | **No** (~1M tokens/day ≈ 22 questions) |
| Groq / OpenRouter free | **No** (TPM / prompt size caps) |

### Colab (`longmemeval_colab_v2.ipynb`)

1. **Runtime → GPU**
2. **Secrets** → `GEMINI_API_KEY` ([aistudio.google.com/apikey](https://aistudio.google.com/apikey))
3. Config: `BENCH_PROFILE = "v2"`, `E2E_LIMIT = 500`, `E2E_LLM_BACKEND = "gemini"`, `E2E_WORKERS = 2`
4. Run all cells (or `BENCH_PROFILE = "e2e"` if retrieval already done)
5. Download `longmemeval_colab_e2e_500.json`

### After Colab (finalize paper)

```bash
bash scripts/post-colab-e2e.sh /path/to/longmemeval_colab_e2e_500.json
```

### Server (optional)

```bash
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794
export LONGMEMEVAL_LLM_BACKEND=gemini
export LONGMEMEVAL_E2E_WORKERS=2
bash scripts/run-longmemeval-e2e-500.sh 500
```

## Smoke test

```bash
source /home/ambugo/litellm/.env
PYTHONPATH=benchmarks python3 -c "from cloud_llm import smoke_test; print(smoke_test('gemini'))"
```

## Protocol note

Our E2E uses **Gemini 2.5 Flash** reader + judge with Wu et al. judge *templates*. Baselines cite **gpt-4o** — report side-by-side, not strict SOTA.

## Cerebras (PayGo only)

If you add Cerebras PayGo later: `--llm-backend cerebras --reader-model gpt-oss-120b`

# LongMemEval E2E

End-to-end QA uses **Cursor Cloud Agents API only**:

- **Key:** `CURSOR_API_KEY` (`crsr_*`) — same as `/opt/ambugo/serverbrain/.env`
- **Model:** Auto (`default` on `GET /v1/models`)
- **Endpoint:** `POST https://api.cursor.com/v1/agents` → poll run → `run.result`
- **No** OpenAI key, **no** OpenRouter, **no** `cursor_sdk`

## Local

```bash
bash scripts/run-longmemeval-e2e-cursor.sh 50
```

Loads key from `CURSOR_ENV_FILE` (default `/opt/ambugo/serverbrain/.env`).

## Colab

[`longmemeval_colab_v2.ipynb`](../benchmarks/longmemeval_colab_v2.ipynb)

1. Colab **Secrets** → `CURSOR_API_KEY` = paste from serverbrain `.env`
2. `BENCH_PROFILE = "v2"` (500 retrieval + E2E) or `"e2e"`
3. GPU runtime for retrieval; E2E calls Cursor API over HTTPS

Open: https://colab.research.google.com/github/voxmastery/FluctlightDB/blob/main/benchmarks/longmemeval_colab_v2.ipynb

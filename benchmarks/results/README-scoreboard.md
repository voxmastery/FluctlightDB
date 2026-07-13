# LongMemEval scoreboard (BM25 vs Fluctlight)

World-record trophy measurement spine. Spec:
`docs/superpowers/specs/2026-07-13-longmemeval-world-record-trophy-design.md`

## 1) BM25 baseline (lexical / `--fast`)

Same key flags as v4 so lexical is not artificially weakened; dense is the only
disabled piece (`--fast`).

```bash
python3 benchmarks/longmemeval_bench.py \
  --data /tmp/longmemeval/data/longmemeval_s_cleaned.json \
  --mode index --granularity session --metric session \
  --fast --query-expand --dual-key --pref-facts-key \
  --top-k 8 --report-ks 1,3,5,8 \
  --checkpoint benchmarks/results/longmemeval-bm25.checkpoint.jsonl \
  --json-out benchmarks/results/longmemeval-bm25-baseline.json
```

## 2) Fluctlight v4 (mpnet)

```bash
export FLUCTLIGHT_EMBED_MODEL=sentence-transformers/multi-qa-mpnet-base-dot-v1
python3 benchmarks/longmemeval_bench.py \
  --data /tmp/longmemeval/data/longmemeval_s_cleaned.json \
  --mode index --granularity session --metric session \
  --query-expand --dual-key --pref-facts-key \
  --top-k 8 --report-ks 1,3,5,8 \
  --checkpoint benchmarks/results/longmemeval-v4-multik.checkpoint.jsonl \
  --json-out benchmarks/results/longmemeval-v4-multik.json
```

## 3) Merge + freeze H

```bash
python3 benchmarks/longmemeval_scoreboard.py \
  --bm25 benchmarks/results/longmemeval-bm25-baseline.json \
  --fluctlight benchmarks/results/longmemeval-v4-multik.json \
  --hard-slice-out benchmarks/results/longmemeval-hard-slice-H.json \
  --json-out benchmarks/results/longmemeval-scoreboard.json
```

Exit code `0` = all claim gates pass; `2` = gates not yet met (expected until
@5 > 97.6%, preference 30/30, and hard-slice win).

Do **not** claim #1 publicly until `"all_pass": true`.

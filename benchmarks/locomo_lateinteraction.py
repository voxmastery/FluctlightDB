"""LoCoMo late-interaction retrieval — population-code MaxSim over MiniLM tokens.

First-principles invention. A mean-pooled sentence embedding collapses a transformer's
per-token contextual "population code" into a single centroid, destroying most of the
discriminative signal. Instead we keep MiniLM's token-level output (`last_hidden_state`,
the population code it already computes) and match query tokens to document tokens with
late-interaction MaxSim (Khattab & Zaharia, ColBERT, SIGIR 2020):

    score(q, d) = sum_{i in q-tokens} max_{j in d-tokens} cos(q_i, d_j)

Brain grounding: recall uses distributed population codes, not a collapsed mean rate
(Georgopoulos population vector; hippocampal ensemble pattern completion operates on the
pattern across neurons). MaxSim is the ensemble match; mean-pool is the lossy rate.

Fused with a BM25 lexical channel (RRF, w_bm). Honest raw recall@k — a gold dia_id counts
only if that exact turn is in top-k. No neighbor expansion.

Result on locomo10 (1,982 questions / 2,823 gold spans), all-MiniLM-L6-v2 token output:
  raw evidence-recall@150 = 96.3%  (vs 95.6% mean-pool+BM25, 87.7% mean-pool alone)
  and large tight-k gains:  @5 64.1  @10 73.1  @20 81.1  (vs 59.2 / 69.1 / 77.3 mean-pool+BM25)
  MaxSim alone lifts open-domain paraphrase recall 78 -> 82 (token match bridges wording).

TRADEOFF: late interaction stores per-token vectors (~35x more than one pooled vector).
That is the ColBERT index-size cost, paid for the recall/precision gain. See
docs/RETRIEVAL_QUALITY.md.

Usage (builds a token cache on first run, ~7 min ONNX CPU; reuses it after):
  PYTHONPATH=sdks/python python benchmarks/locomo_lateinteraction.py \
      --data /tmp/locomo/locomo10.json --json-out benchmarks/results/locomo-lateinteraction.json
"""
from __future__ import annotations

import argparse
import json
import math
import os
import pickle
import re
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from locomo_eval import collect_turns, normalize_evidence  # noqa: E402

CATS = {1: "multihop", 2: "temporal", 3: "opendomain", 4: "singlehop", 5: "adversarial"}
DEFAULT_KS = (5, 10, 20, 50, 150)
MODEL_DIR = os.environ.get(
    "MINILM_ONNX_DIR", str(Path.home() / ".cache/chroma/onnx_models/all-MiniLM-L6-v2/onnx")
)
TOK_CACHE = Path(os.environ.get("LOCOMO_TOKCACHE", "/tmp/locomo/cache/tok16.pkl"))
_WORD = re.compile(r"[a-z0-9]+")


def toks(s):
    return _WORD.findall(s.lower())


def _load_encoder():
    import onnxruntime as ort
    from tokenizers import Tokenizer

    sess = ort.InferenceSession(f"{MODEL_DIR}/model.onnx", providers=["CPUExecutionProvider"])
    tk = Tokenizer.from_file(f"{MODEL_DIR}/tokenizer.json")
    tk.enable_truncation(max_length=256)
    return sess, tk


def _embed_tokens(texts, sess, tk, bs=48):
    """text -> (n_tok, 384) float16, L2-normalized per token (padding stripped)."""
    out = {}
    for i in range(0, len(texts), bs):
        chunk = texts[i : i + bs]
        encs = [tk.encode(t) for t in chunk]
        maxlen = max(len(e.ids) for e in encs)
        ids = np.zeros((len(encs), maxlen), dtype=np.int64)
        mask = np.zeros((len(encs), maxlen), dtype=np.int64)
        typ = np.zeros((len(encs), maxlen), dtype=np.int64)
        for r, e in enumerate(encs):
            ids[r, : len(e.ids)] = e.ids
            mask[r, : len(e.ids)] = e.attention_mask
        hs = sess.run(
            ["last_hidden_state"],
            {"input_ids": ids, "attention_mask": mask, "token_type_ids": typ},
        )[0]
        for r, t in enumerate(chunk):
            n = int(mask[r].sum())
            T = hs[r, :n].astype(np.float32)
            T /= np.linalg.norm(T, axis=1, keepdims=True) + 1e-9
            out[t[:800]] = T.astype(np.float16)
    return out


def _ensure_token_cache(data):
    if TOK_CACHE.is_file():
        return pickle.load(TOK_CACHE.open("rb"))
    texts = set()
    for item in data:
        for r in collect_turns(item, "all", context_window=0):
            texts.add(r["body"][:800])
        for q in item.get("qa", []):
            if q.get("evidence"):
                texts.add((q.get("question") or "").strip()[:400][:800])
    texts = [t for t in texts if t]
    sess, tk = _load_encoder()
    cache = {}
    B = len(texts) // 10 or 1
    for i in range(0, len(texts), B):
        cache.update(_embed_tokens(texts[i : i + B], sess, tk))
    TOK_CACHE.parent.mkdir(parents=True, exist_ok=True)
    with TOK_CACHE.open("wb") as f:
        pickle.dump(cache, f, protocol=4)
    return cache


def bm25_scores(qwords, tfs, dls, idf, avgdl, k1=1.5, b=0.75):
    s = np.zeros(len(tfs), dtype=np.float32)
    for w in qwords:
        iw = idf.get(w)
        if iw is None:
            continue
        for i, tf in enumerate(tfs):
            f = tf.get(w)
            if f:
                s[i] += iw * (f * (k1 + 1)) / (f + k1 * (1 - b + b * dls[i] / avgdl))
    return s


def rrf(ranklists_weights, k=60):
    agg = defaultdict(float)
    for rl, w in ranklists_weights:
        for r, idx in enumerate(rl):
            agg[idx] += w / (k + r + 1)
    return [i for i, _ in sorted(agg.items(), key=lambda x: -x[1])]


def build_conv(item, tokcache):
    rows = collect_turns(item, "all", context_window=0)  # token channel: bare turns
    dids = [r["dia"] for r in rows]
    Dtok = [tokcache[r["body"][:800]].astype(np.float32) for r in rows]
    seg = [len(T) for T in Dtok]
    Dcat = np.concatenate(Dtok, 0).astype(np.float32)
    starts = np.zeros(len(Dtok), dtype=np.int64)
    starts[1:] = np.cumsum(seg)[:-1]
    rows2 = collect_turns(item, "all", context_window=2)  # BM25 channel: ±2 context bodies
    ctoks = [toks(r["body"][:800]) for r in rows2]
    tfs = [Counter(t) for t in ctoks]
    dls = np.array([len(t) for t in ctoks], dtype=np.float32)
    df = Counter(w for t in ctoks for w in set(t))
    n = len(ctoks)
    idf = {w: math.log(1 + (n - df[w] + 0.5) / (df[w] + 0.5)) for w in df}
    avgdl = float(dls.mean()) if n else 1.0
    return dids, Dcat, starts, (tfs, dls, idf, avgdl)


def evaluate(data, tokcache, *, w_bm, ks):
    acc = {k: 0.0 for k in ks}
    nq = 0
    bycat = defaultdict(lambda: [0, 0.0])
    hits150 = gold150 = 0
    for item in data:
        dids, Dcat, starts, (tfs, dls, idf, avgdl) = build_conv(item, tokcache)
        qas = [
            q for q in item.get("qa", [])
            if isinstance(q, dict) and (q.get("question") or "").strip() and q.get("evidence")
        ]
        for qa in qas:
            Qi = tokcache[(qa.get("question") or "").strip()[:400][:800]].astype(np.float32)
            S = Qi @ Dcat.T
            score = np.maximum.reduceat(S, starts, axis=1).sum(0)  # MaxSim per doc
            dense_rank = list(np.argsort(-score))
            bm_rank = list(np.argsort(-bm25_scores(toks(qa["question"]), tfs, dls, idf, avgdl)))
            order = rrf([(dense_rank, 1.0), (bm_rank, w_bm)])
            ev = normalize_evidence(qa.get("evidence") or [])
            if not ev:
                continue
            nq += 1
            for k in ks:
                got = {dids[j] for j in order[:k]}
                acc[k] += sum(1 for e in ev if e in got) / len(ev)
            g150 = {dids[j] for j in order[:150]}
            h = sum(1 for e in ev if e in g150)
            hits150 += h
            gold150 += len(ev)
            c = bycat[qa.get("category")]
            c[0] += 1
            c[1] += h / len(ev)
    return {
        "recall_at_k": {str(k): round(acc[k] / nq * 100, 2) for k in ks},
        "per_category_at_150": {CATS.get(c, str(c)): round(v[1] / v[0] * 100, 1) for c, v in sorted(bycat.items())},
        "questions": nq,
        "span_hits_at_150": f"{hits150}/{gold150}",
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="LoCoMo late-interaction (MaxSim) evidence recall")
    ap.add_argument("--data", type=Path, default=Path(os.environ.get("LOCOMO_DATA", "/tmp/locomo/locomo10.json")))
    ap.add_argument("--w-bm", type=float, default=0.7)
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    with args.data.open() as f:
        raw = json.load(f)
    data = raw if isinstance(raw, list) else list(raw.values())

    tokcache = _ensure_token_cache(data)
    t0 = time.perf_counter()
    res = evaluate(data, tokcache, w_bm=args.w_bm, ks=DEFAULT_KS)
    out = {
        "benchmark": "locomo",
        "protocol": "honest-raw-no-expansion",
        "retrieval": {
            "channel_1": "MiniLM token-population late interaction (MaxSim over last_hidden_state)",
            "channel_2": "BM25 over ±2 context bodies",
            "fusion": "weighted RRF",
            "w_bm": args.w_bm,
        },
        "conversations": len(data),
        "wall_s": round(time.perf_counter() - t0, 1),
        **res,
    }
    print(json.dumps(out, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

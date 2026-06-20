#!/usr/bin/env python3
"""Idempotent re-ingest brain-feed.jsonl into Fluctlight via HTTP API."""
import argparse
import hashlib
import json
import math
import os
import re
import struct
import urllib.error
import urllib.request

SERVE = os.environ.get("FLUCTLIGHT_SERVE_URL", "http://127.0.0.1:8792").rstrip("/")
EMBED = os.environ.get("FLUCTLIGHT_EMBED_URL", "http://127.0.0.1:8793/embed").rstrip("/")
EMBED_LAZY = os.environ.get("FLUCTLIGHT_EMBED_LAZY", "1").lower() in ("1", "true", "yes")
EMBED_DIM = int(os.environ.get("FLUCTLIGHT_EMBED_DIM", "384"))
API_KEY = os.environ.get("FLUCTLIGHT_API_KEY", "")
FEED = os.environ.get("FLUCTLIGHT_FEED", os.path.expanduser("~/.fluctlight/brain-feed.jsonl"))
DOC_ID = "brain-feed"


def _headers(extra=None):
    h = {"Content-Type": "application/json"}
    if API_KEY:
        h["Authorization"] = f"Bearer {API_KEY}"
    if extra:
        h.update(extra)
    return h


def _hash_projection(text, dim):
    vec = [0.0] * dim
    tokens = re.findall(r"[a-z0-9]+", (text or "").lower())
    if not tokens:
        tokens = [(text or "")[:32].lower()]
    for tok in tokens:
        h = hashlib.sha256(tok.encode("utf-8")).digest()
        for i in range(0, min(len(h) - 3, 32), 4):
            idx = struct.unpack("<I", h[i : i + 4])[0] % dim
            sign = 1.0 if h[i] & 1 else -1.0
            vec[idx] += sign
    norm = math.sqrt(sum(v * v for v in vec)) or 1.0
    return [v / norm for v in vec]


def embed(text, salience=0.55):
    text = (text or "").strip()
    if not text:
        return None
    if EMBED_LAZY and salience < 0.7:
        return _hash_projection(text[:500], EMBED_DIM)
    try:
        req = urllib.request.Request(
            EMBED,
            data=json.dumps({"text": text[:500]}).encode(),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=8) as resp:
            data = json.loads(resp.read())
        vec = data.get("embedding") or data.get("vector")
        if isinstance(vec, list) and vec:
            return vec
    except Exception:
        pass
    return _hash_projection(text[:500], EMBED_DIM)


def line_key(row, content):
    raw = json.dumps(row, sort_keys=True, default=str)
    return hashlib.sha256(raw.encode()).hexdigest()[:16]


def experience(content, context, chunk_id, uid=None, salience=0.55, dry_run=False):
    if dry_run:
        return {"dry_run": True, "chunk_id": chunk_id}
    payload = {
        "content": content[:500],
        "context": context[:120],
        "salience": salience,
        "semantic_vector": embed(content[:500], salience),
        "doc_id": DOC_ID,
        "chunk_id": chunk_id,
        "source_uri": f"feed://{FEED}#{chunk_id}",
        "provenance_kind": "chat_assertion",
    }
    if uid:
        payload["agent_id"] = str(uid)
    req = urllib.request.Request(
        f"{SERVE}/api/v1/experience",
        data=json.dumps(payload).encode(),
        headers=_headers({"Idempotency-Key": f"feed-{chunk_id}"}),
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read())


def main():
    parser = argparse.ArgumentParser(description="Re-ingest brain-feed.jsonl idempotently")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--feed", default=FEED)
    args = parser.parse_args()

    if not os.path.isfile(args.feed):
        print(json.dumps({"ok": False, "error": "feed missing", "path": args.feed}))
        return
    n = skipped = deduped = 0
    with open(args.feed) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            kind = row.get("kind", "event")
            content = row.get("content", "")
            if not content:
                continue
            ctx = row.get("context", "feed")
            uid = row.get("uid")
            prefix = {"user": "user:", "assistant": "assistant:", "think": "think:"}.get(kind, "")
            chunk_id = line_key(row, content)
            salience = 0.62 if kind == "user" else 0.58
            try:
                rep = experience(
                    f"{prefix}{content}",
                    ctx,
                    chunk_id,
                    uid=uid,
                    salience=salience,
                    dry_run=args.dry_run,
                )
                if rep.get("deduplicated"):
                    deduped += 1
                n += 1
            except urllib.error.HTTPError as e:
                body = e.read().decode("utf-8", errors="replace")
                if "duplicate idempotency" in body.lower():
                    skipped += 1
                else:
                    raise
    print(
        json.dumps(
            {
                "ok": True,
                "reingested": n,
                "deduplicated": deduped,
                "skipped_idempotent": skipped,
                "dry_run": args.dry_run,
            }
        )
    )


if __name__ == "__main__":
    main()

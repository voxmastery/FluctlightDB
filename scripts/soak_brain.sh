#!/usr/bin/env bash
# Sustained experience/activate soak — tracks memory growth and recall hit rate.
# CI: short run (default 2000 cycles). Production prep: FLUCTLIGHT_SOAK_CYCLES=50000+
set -euo pipefail

BRAIN_PATH="${1:-/tmp/fluctlight-soak.brain}"
CYCLES="${FLUCTLIGHT_SOAK_CYCLES:-2000}"
CHECKPOINT_EVERY="${FLUCTLIGHT_SOAK_CHECKPOINT_EVERY:-500}"

rm -rf "$BRAIN_PATH"
export FLUCTLIGHT_FAST_INGEST=1

python3 - "$BRAIN_PATH" <<'PY'
import os, sys, time
from fluctlightdb import connect

path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/fluctlight-soak.brain"
cycles = int(os.environ.get("FLUCTLIGHT_SOAK_CYCLES", "2000"))
ckpt_every = int(os.environ.get("FLUCTLIGHT_SOAK_CHECKPOINT_EVERY", "500"))

brain = connect(path)
t0 = time.perf_counter()
hits = 0
misses = 0

for i in range(cycles):
    token = f"fact-{i}"
    brain.experience(f"soak memory item {token}", context="soak", salience=0.55)
    if i % 10 == 0:
        r = brain.activate(token)
        recalls = r.get("recalls") or []
        if recalls:
            hits += 1
        else:
            misses += 1
    if ckpt_every and i > 0 and i % ckpt_every == 0:
        brain.checkpoint()

brain.checkpoint()
elapsed = time.perf_counter() - t0
status = brain.status()
disk = sum(
    os.path.getsize(os.path.join(dp, f))
    for dp, _, files in os.walk(path)
    for f in files
)
print(f"cycles={cycles} elapsed_s={elapsed:.2f} rate={cycles/elapsed:.1f}/s")
print(f"recall_probes hits={hits} misses={misses}")
print(f"status engrams={status.get('engrams')} synapses={status.get('synapses')}")
print(f"disk_bytes={disk}")
PY

echo "soak OK: $BRAIN_PATH ($CYCLES cycles)"

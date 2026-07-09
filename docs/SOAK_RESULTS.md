# Soak test results (maintainer sample)

Sample run on maintainer hardware — not a SLA or capacity guarantee. Re-run locally before your launch.

## Command

```bash
FLUCTLIGHT_SOAK_CYCLES=3000 bash scripts/soak_brain.sh /tmp/soak-sample
```

## 2026-07-09 — Linux x86_64, `fluctlightdb-native` 0.5.8 (local build)

| Metric | Value |
|--------|------:|
| Cycles | 3000 `experience()` |
| Wall time | 185.1 s |
| Throughput | ~16.2 writes/s |
| Recall probes (every 10th) | 300 hits / 0 misses |
| Final engrams | 3000 |
| Final synapses | 40464 |
| On-disk size | ~6.2 MB |

**Interpretation:** No recall misses on lexical probes; memory grew linearly with cycle count. For production sizing, run `FLUCTLIGHT_SOAK_CYCLES=50000+` on your target hardware and monitor disk + RSS.

CI runs a shorter soak (1500 cycles) in the `python-sdk-native` job.

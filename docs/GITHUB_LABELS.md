# GitHub labels

Labels are defined in [`.github/labels.json`](../.github/labels.json) and synced by the [**Sync GitHub labels** workflow](https://github.com/voxmastery/FluctlightDB/actions/workflows/sync-labels.yml) on push to `main`.

Manual one-off:

```bash
gh label create reproduction --color "1D76DB" --description "Independent benchmark reproduction report"
gh label create benchmark --color "FBCA04" --description "Benchmark harness or eval work"
gh label create supply-chain --color "5319E7" --description "cargo-audit, cargo-deny, dependency upgrades"
```

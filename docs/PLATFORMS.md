# FluctlightDB — distribution platforms

While arXiv cs.DB endorsement is pending, keep messaging aligned everywhere.

## Mission (use everywhere)

> **Goal:** SQLite for agent memory.  
> **Claim:** Long-term agent memory is a **third data model** — native `experience()` / `activate()` in an embedded Rust engine, not a vector DB wrapper or Mem0-style chat layer.  
> **Proof:** LoCoMo **98.1%** evidence recall, BEIR SciFact parity, FAMB 97–98%.  
> **Paper:** `papers/arxiv-v1/` in the repo (private draft workflow).

---

## Platform checklist

| Platform | URL | Status | Action |
|----------|-----|--------|--------|
| **GitHub** | https://github.com/voxmastery/FluctlightDB | Live | `GITHUB_TOKEN=… ./scripts/update-github-about.sh` after each positioning change |
| **PyPI** `fluctlightdb` | https://pypi.org/project/fluctlightdb/ | Live (0.5.0) | Bump `sdks/python/pyproject.toml` → tag or `workflow_dispatch` **Publish to PyPI** |
| **PyPI** `fluctlightdb-native` | https://pypi.org/project/fluctlightdb-native/ | Live | Same release workflow (native wheels) |
| **GHCR Docker** | `ghcr.io/voxmastery/fluctlightdb` | Live | Image labels in `Dockerfile`; new image on **Release** workflow |
| **Paper (LaTeX)** | `papers/arxiv-v1/` in repo | Draft | arXiv when ready — no public viewer URL in README |
| **crates.io** | (optional) | Not published | `cargo publish -p fluctlightdb` when ready |
| **Hugging Face** | https://huggingface.co/voxmastery (create org) | Todo | Use `hub/README.md` as org profile; optional Space linking to paper |
| **arXiv** | cs.DB | Pending endorsement | `papers/arxiv-v1/` + `papers/site/files/guide.md` |

---

## One-command updates

```bash
# GitHub About (description, homepage, topics)
export GITHUB_TOKEN=ghp_…
./scripts/update-github-about.sh

# Paper site on VPS
./papers/site/install-search-site.sh

# PyPI (after version bump on main)
# GitHub → Actions → "Publish to PyPI" → Run workflow
```

---

## PyPI alignment

- **Short description:** `pyproject.toml` → `description` field (one line on pypi.org).
- **Long description:** `readme = "../../README.md"` — full repo README including **Mission** and benchmarks.
- **Homepage:** paper viewer URL (not only GitHub).

---

## Suggested next platforms (non-arXiv)

1. **Hugging Face org** — AI-agent audience; link paper + `pip install fluctlightdb[native]`.
2. **Awesome lists** — PR to `awesome-llm-agents`, `awesome-rag`, etc. with one paragraph + benchmark link.
3. **Reddit / HN** — post after PyPI 0.4.4 + GitHub About synced (lead with third data model, not hype).
4. **Dev.to / Medium** — shortened Mission + LoCoMo table (you control ServerBrain publishing).
5. **LinkedIn** — founder post linking paper viewer + GitHub.

Do **not** compare 98.1% evidence recall to Mem0 ~92% QA without naming the metric.

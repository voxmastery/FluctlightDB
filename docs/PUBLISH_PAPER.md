# Publishing the FluctlightDB paper

arXiv is the primary preprint. **Full venue ladder:** [RESEARCH_VENUES.md](RESEARCH_VENUES.md)

## Quick commands

```bash
# Sync HF artifacts (default namespace: Voxiesz — set HF_ORG=voxmastery when org access works)
hf auth login
bash scripts/publish-paper-huggingface.sh

# Private VPS viewer (password-protected)
./papers/site/install-search-site.sh
```

## Live Hugging Face (under account Voxiesz)

| Artifact | URL |
|----------|-----|
| Paper card | https://huggingface.co/Voxiesz/fluctlightdb-paper |
| Benchmarks | https://huggingface.co/datasets/Voxiesz/fluctlightdb-benchmarks |
| Space viewer | https://huggingface.co/spaces/Voxiesz/fluctlightdb-paper-viewer |

**Why not `voxmastery/`?** The HF org exists but the logged-in user lacks create rights. Fix: HF org settings → add **Voxiesz** as member with write → then `HF_ORG=voxmastery bash scripts/publish-paper-huggingface.sh`.

## Platform checklist

| Platform | When | Notes |
|----------|------|-------|
| **arXiv cs.DB** | First | `papers/arxiv-v1/` · endorsement required |
| **Zenodo** | With arXiv | GitHub release → DOI |
| **GitHub + CITATION.cff** | Now | Cite this repository |
| **Hugging Face** | Now | See URLs above |
| **Private VPS** | Draft review | search.ambugo.help/paper |
| **Semantic Scholar / Scholar** | After arXiv ID | Auto-ingest |
| **CIDR 2027** | ~Oct 2026 | Vision paper |
| **VLDB / SIGMOD / ICDE** | 2027+ | Full eval paper |

GitHub Pages preprint was removed from public README — not used as the public face of the paper.

## Outreach (after arXiv)

Copy from `papers/outreach/` — Dev.to, LinkedIn, HN, awesome lists.

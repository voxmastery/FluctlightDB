# Publishing the FluctlightDB paper (non-arXiv)

arXiv cs.DB is planned separately. This guide covers **every other platform** now.

## Quick start

```bash
cd /home/ambugo/fluctlightdb

# 1. Sync public site + show status
bash scripts/publish-paper-platforms.sh

# 2. Push to GitHub (deploys Pages automatically)
git add papers/public CITATION.cff .github/workflows/paper-pages.yml scripts/
git commit -m "Publish paper preprint to GitHub Pages and HF"
git push origin main

# 3. Hugging Face (after login)
hf auth login
bash scripts/publish-paper-huggingface.sh
```

---

## Platform checklist

| Platform | URL (after publish) | How |
|----------|---------------------|-----|
| **GitHub Pages** | https://voxmastery.github.io/FluctlightDB/ | Push `main` → workflow `paper-pages.yml` |
| **GitHub Citation** | Repo sidebar “Cite this repository” | `CITATION.cff` at repo root |
| **Hugging Face paper card** | https://huggingface.co/voxmastery/fluctlightdb-paper | `scripts/publish-paper-huggingface.sh` |
| **HF benchmark dataset** | https://huggingface.co/datasets/voxmastery/fluctlightdb-benchmarks | Same script |
| **HF Space viewer** | https://huggingface.co/spaces/voxmastery/fluctlightdb-paper-viewer | Same script (static HTML) |
| **PyPI long description** | https://pypi.org/project/fluctlightdb/ | Already uses README; bump version to refresh |
| **Private VPS viewer** | https://search.ambugo.help/paper/ | `papers/site/install-search-site.sh` (htpasswd) |
| **Dev.to / Medium** | Your account | Copy from `papers/outreach/devto.md` |
| **LinkedIn** | Your profile | Copy from `papers/outreach/linkedin.md` |
| **Hacker News / Reddit** | — | Copy from `papers/outreach/hn.md` (post after Pages live) |
| **Awesome lists** | GitHub PRs | Template in `papers/outreach/awesome-list-pr.md` |
| **Zenodo** | zenodo.org | Connect GitHub repo → create release → auto-DOI (optional) |
| **Semantic Scholar** | — | Auto-indexes after arXiv; manual claim later |
| **arXiv** | cs.DB | **Later** — see `docs/PAPER_DRAFT.md` |

---

## GitHub Pages (one-time setup)

1. Repo **Settings → Pages → Build and deployment → Source: GitHub Actions**
2. Push to `main` with changes under `papers/` or run workflow manually (**Actions → Publish paper**)
3. Set repo **About → Website** to `https://voxmastery.github.io/FluctlightDB/`

Or with token:

```bash
export GITHUB_TOKEN=ghp_…
HOMEPAGE="https://voxmastery.github.io/FluctlightDB/" \
  ./scripts/update-github-about.sh
```

---

## Hugging Face org

1. Create org: https://huggingface.co/organizations/new → `voxmastery`
2. `hf auth login` (write token)
3. `bash scripts/publish-paper-huggingface.sh`

Creates three public artifacts:
- **Model card** `voxmastery/fluctlightdb-paper` — paper README + links
- **Dataset** `voxmastery/fluctlightdb-benchmarks` — frozen `results.json`
- **Space** `voxmastery/fluctlightdb-paper-viewer` — same HTML as GitHub Pages

Update org profile bio from `hub/README.md`.

---

## Outreach timing

**Do now:** GitHub Pages + HF + CITATION.cff  
**After Pages is live:** Dev.to, LinkedIn, awesome-list PRs  
**After PyPI bump:** HN / r/MachineLearning (lead with metric table + link, not hype)  
**Later:** arXiv → Semantic Scholar auto-ingest → ResearchGate claim

---

## Sync after paper edits

```bash
# Edit papers/site/files/draft.md or papers/arxiv-v1/main.tex
bash scripts/sync-paper-public.sh   # updates papers/public/
bash scripts/publish-paper-huggingface.sh  # if HF logged in
git commit && git push                 # redeploys Pages
```

Private VPS copy:

```bash
./papers/site/install-search-site.sh
```

---

## What not to do

- Do not compare 98.1% **evidence recall** to Mem0 ~92% **LLM QA** without naming metrics.
- Do not link private VPS paper URL in public README until you want htpasswd-only access removed.

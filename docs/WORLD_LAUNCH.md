# World launch playbook — FluctlightDB paper

Goal: **maximum credible visibility** before arXiv endorsement lands. One peer-reviewed track + one preprint DOI + three community waves.

---

## Phase 0 — This week (you + agent)

| # | Action | Impact | Time |
|---|--------|--------|------|
| 1 | **Zenodo DOI** | Permanent citable preprint | 30 min |
| 2 | **IEEE Access submit** | Scopus Q1, fastest legitimate peer review | 2 hr |
| 3 | **HF org fix** | Brand-aligned URLs | 10 min |
| 4 | **GitHub Release** | Version pin for citations | 15 min |

### 1. Zenodo DOI (do first)

1. https://zenodo.org → Login with GitHub  
2. Account → **GitHub** → Enable **voxmastery/FluctlightDB**  
3. `.zenodo.json` is at repo root (already committed)  
4. GitHub → **Releases → New release** → tag `paper-v1.0` → attach `papers/arxiv-v1/main.pdf` if built  
5. Zenodo auto-creates DOI → paste into `CITATION.cff` and HF README  

### 2. IEEE Access

Follow `papers/submit/ieee-access/checklist.md`  
Build PDF: `cd papers/arxiv-v1 && ./build.sh`

### 3. Hugging Face org

HF → **voxmastery** org → Members → invite **Voxiesz** (write)  
Then: `HF_ORG=voxmastery bash scripts/publish-paper-huggingface.sh`

### 4. GitHub Release

```bash
# After main.pdf exists
gh release create paper-v1.0 papers/arxiv-v1/main.pdf \
  --title "FluctlightDB paper preprint v1.0" \
  --notes-file papers/submit/release-notes.md
```

---

## Phase 1 — Discovery wave (day Zenodo DOI live)

Post **same day**, staggered 2–4 hours apart. Lead with **DOI + one number (98.1% LoCoMo)**, not hype.

| Channel | Audience | Copy |
|---------|----------|------|
| **Hacker News** | Builders, VCs, engineers | `papers/outreach/hn.md` — add Zenodo DOI |
| **r/MachineLearning** | ML researchers | Same + metric disclaimer |
| **r/LocalLLaMA** | Agent builders | Emphasize embedded / pip install |
| **LinkedIn** | Industry, DB folks | `papers/outreach/linkedin.md` |
| **Dev.to** | Developers | `papers/outreach/devto.md` |
| **X/Twitter** | Short thread: 3 tweets — problem, number, links |

**HN title that works:**  
`FluctlightDB – embedded memory engine for agents (98.1% LoCoMo evidence recall, MIT)`

---

## Phase 2 — Credibility wave (week 1)

| Action | Why |
|--------|-----|
| **Awesome list PRs** | `papers/outreach/awesome-list-pr.md` — 3–5 lists |
| **HF Papers** (if indexed) | Link DOI on paper card |
| **Semantic Scholar** | Claim author after Zenodo/arXiv |
| **Mem0 / Zep Discord** | One factual post: same-metric comparison offer |

Do **not** spam. One post per community; respond to metric questions honestly.

---

## Phase 3 — Conference pipeline (month 1–3)

| Target | Deadline (typical) | Paper version |
|--------|-------------------|---------------|
| **CIDR 2027** | ~Oct 2026 | Vision / 6-page — brain-native framing |
| **MLSys 2027** | ~Jan 2027 | Benchmark + reproducibility lead |
| **ICDE 2027** | ~Oct 2026 | Full eval if LongMemEval complete |

See `docs/RESEARCH_VENUES.md`.

---

## Phase 4 — When arXiv endorses

1. Upload same PDF + Zenodo DOI in comments  
2. Update everywhere: `arXiv:XXXX.XXXXX`  
3. Second HN thread is OK **only if** new angle (arXiv + IEEE under review)

---

## Where impact is solid (ranked)

1. **IEEE Access** — global Scopus index, Google Scholar, industry trust  
2. **Zenodo DOI** — citable everywhere immediately  
3. **Hacker News** — highest signal for systems/engineering crowd  
4. **Hugging Face** — agent/ML builder funnel → `pip install`  
5. **CIDR → VLDB** — long-term academic legitimacy (SQL path)  
6. **Awesome lists** — sustained discovery in agent tooling repos  

---

## One-line pitch (use everywhere)

> **SQLite for agent memory:** `experience()` / `activate()` in an embedded engine—not a vector DB wrapper. **98.1%** LoCoMo evidence recall. MIT.

---

## Legal reminder

- ✅ Zenodo + HF + GitHub + IEEE Access (one journal at a time)  
- ❌ Do not submit same manuscript to IEEE Access and FGCS simultaneously  
- ✅ arXiv later with same PDF is standard  

Run: `bash scripts/launch-paper.sh`

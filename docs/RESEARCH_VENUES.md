# Where to publish the FluctlightDB research paper

arXiv is the **first** step, not the only one. This doc is the planned publication ladder from your repo (`papers/README.md`, `docs/PLATFORMS.md`, `papers/site/files/guide.md`).

---

## Tier 1 — Preprint & discovery (do now, before or with arXiv)

| Platform | What it is | Fit for FluctlightDB | Action |
|----------|------------|----------------------|--------|
| **arXiv** | Free CS preprint server | **Primary.** cs.DB + cross-list cs.AI, cs.IR | Submit `papers/arxiv-v1/main.pdf` when endorsed |
| **Zenodo** | CERN-backed archive, DOI | Permanent DOI for PDF + code snapshot | Connect GitHub → GitHub release → Zenodo DOI |
| **OSF Preprints** | Open Science Framework | Alternative preprint if arXiv delayed | Upload PDF + link repo |
| **SSRN** | Social science / tech preprints | Secondary; less common for systems DB papers | Optional |
| **GitHub repo** | Code + LaTeX source | **Required artifact host** | `papers/arxiv-v1/`, `CITATION.cff`, frozen `benchmarks/results/` |
| **Hugging Face** | ML/agent community | Paper card + benchmark dataset + Space viewer | `Voxiesz/fluctlightdb-*` (move to `voxmastery/` org when you have write access) |
| **Private VPS viewer** | Password-protected draft | Internal review only | `papers/site/install-search-site.sh` → search.ambugo.help/paper |
| **Semantic Scholar** | Citation index | Auto-indexes after arXiv; claim author profile | After arXiv ID exists |
| **Google Scholar** | Citation index | Add after arXiv / DOI | Author profile → add article |
| **ORCID** | Author ID | **Done** — link to arXiv + Zenodo | 0009-0006-7758-4114 |

**Not a publisher:** Harvard, MIT, etc. do not “publish” your paper unless you submit to **their journals** (e.g. Harvard Data Science Review) or use **institutional repositories** (DASH) for green open access copies *after* acceptance elsewhere.

---

## Tier 2 — Peer-reviewed systems & DB venues (planned in repo)

| Venue | Type | Deadline rhythm | Fit | Our plan |
|-------|------|-----------------|-----|----------|
| **CIDR** | Vision / early systems (DB) | ~Oct for next year | Strong for “third data model” thesis | **Target CIDR 2027** — short visionary paper after arXiv feedback |
| **VLDB** | Top-tier DB conference | ~Mar | Full eval + scale + reproducibility | **Target** after benchmark freeze + LongMemEval full |
| **SIGMOD** | Top-tier DB conference | ~Dec | Same as VLDB | Alternative to VLDB same paper track |
| **ICDE** | IEEE DB conference | ~Oct | Systems + benchmarks | Good if IEEE affiliation desired |
| **EDBT** | European DB | ~Oct | Similar to ICDE | EU visibility |
| **OSDI / SOSP** | Top systems | biennial | Hard bar; need production scale story | Stretch — only with multi-tenant + SLO data |
| **NSDI** | Networked systems | ~Mar/Sep | If you frame serve + replication | Phase 6 roadmap |
| **EuroSys** | European systems | ~Oct | Embedded engine + eval | Possible mid-tier target |

**Agent / AI memory angle (cross-list or primary):**

| Venue | Fit |
|-------|-----|
| **NeurIPS Datasets & Benchmarks** | If you lead with LoCoMo/FAMB benchmark contribution |
| **ACL/EMNLP Industry / System Demonstrations** | Agent memory demo + retrieval numbers |
| **AAAI** | AI systems track — weaker DB credibility than cs.DB |
| **ICML** | Usually needs ML novelty — not ideal for engine paper |

---

## Tier 3 — IEEE & ACM (what people mean by “IEEE paper”)

| Publisher | Typical venue | Notes |
|-----------|---------------|-------|
| **IEEE** | ICDE, TKDE (journal), Internet Computing | ICDE = conference; TKDE = journal (longer, slower) |
| **ACM** | SIGMOD, VLDB (joint), PACMMOD | SIGMOD/VLDB are the gold standard for data systems |
| **USENIX** | ATC, FAST (if storage angle) | USENIX loves open artifacts + reproducibility |

Conference papers are **peer-reviewed** (3–5 months). arXiv preprint does **not** block submission if venue allows preprints (SIGMOD/VLDB/ICDE all allow arXiv).

---

## Tier 4 — Journals (slow, high permanence)

| Journal | Publisher | When to consider |
|---------|-----------|------------------|
| **VLDB Journal** | Springer/ACM | Extended version after conference |
| **ACM TODS** | ACM | Theoretical + systems DB |
| **IEEE TKDE** | IEEE | Broad data engineering |
| **PVLDB** | VLDB Endowment | Rolling submission, very competitive |

Rule: **conference first** for systems work; journal = extended (+30% new material).

---

## Tier 5 — Industry & outreach (not “papers” but citation drivers)

| Channel | Purpose |
|---------|---------|
| **Dev.to / Medium / LinkedIn** | Narrative + link to arXiv/Zenodo |
| **Hacker News / r/MachineLearning** | After arXiv ID exists |
| **Awesome lists** | GitHub PRs — `papers/outreach/awesome-list-pr.md` |
| **Hugging Face org profile** | Agent audience |
| **PyPI README** | Install funnel → paper DOI |

---

## Recommended timeline (from your plan)

```
Now          arXiv cs.DB preprint + Zenodo DOI + HF artifacts
             Fix HF org: invite Voxiesz → voxmastery namespace
3–6 mo       CIDR 2027 submission (vision + LoCoMo headline)
6–12 mo      VLDB/SIGMOD/ICDE full paper (LongMemEval + baselines + scale)
After accept Conference camera-ready → update arXiv v2 → Semantic Scholar
Optional     VLDB Journal extension
```

---

## What we deliberately removed from public display

- **GitHub Pages** preprint site — not promoted in README; use arXiv (later), Zenodo DOI, or private VPS viewer for drafts.

---

## Checklist before any submission

1. Frozen metrics in `benchmarks/results/YYYY-MM-DD.json`
2. `main.pdf` builds from `papers/arxiv-v1/`
3. Artifact link (this repo + commit hash)
4. Limitations section honest (LongMemEval full, LLM QA, scale)
5. Metric table separates **evidence recall** vs **LLM QA**
6. ORCID on title page

See also: `docs/PAPER_DRAFT.md`, `docs/BENCHMARKS.md`, `papers/site/files/guide.md`.

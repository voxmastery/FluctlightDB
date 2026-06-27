# Paper format & model guide

Research checklist for arXiv submission and AI-assisted drafting.

## Target venue: arXiv — primary cs.DB

| Requirement | Our setup |
|-------------|-----------|
| Format | `article` class, 11pt, 1in margins (`geometry`) |
| Compiler | **pdfLaTeX** (not XeLaTeX unless fonts needed) |
| Bibliography | `plain` + `references.bib` → run `bibtex main` |
| Length | 8–14 pages typical for systems papers |
| Abstract | Single paragraph, ~150–250 words; state problem, method, headline number |
| Figures | PDF, PNG, or EPS; vector preferred for diagrams |
| Group | **cs** (Computer Science) |
| Primary category | **cs.DB** (Databases) — matches the "third data model" thesis, reaches the database community |
| Cross-list | **cs.AI**, **cs.IR** — reaches agents + retrieval crowd |
| Author block | Real name (Ganesh S), Independent Researcher, work email, optional ORCID |
| Endorsement | First-time cs.DB submitters usually need an endorser (anti-spam gate) |
| License | arXiv default (non-exclusive); repo MIT is fine |
| Source upload | Recommended: zip `main.tex`, `references.bib`, `.bbl`, figures |

### arXiv account registration

| Field | Value |
|-------|-------|
| First / Last | Ganesh / S |
| Affiliation | Independent Researcher |
| Country | India |
| Email | voxmastery@ambugo.tech (matches paper) |
| Group | cs |
| Default category | cs.DB |
| Homepage | https://github.com/voxmastery/FluctlightDB |

### What arXiv rejects or flags

- Missing `.bbl` when uploading source only (run full bibtex chain)
- Hyperref link colors breaking print (use `hidelinks` if needed)
- Copyrighted figures without permission
- Abstract that reads like an advertisement (keep factual)
- Wrong compiler (uploading XeLaTeX-only fonts to pdfLaTeX)

### Standard section order (we follow this)

1. Abstract  
2. Introduction + contributions bullet list  
3. Background / related work (can merge or split)  
4. System design (write path, read path, persistence)  
5. Evaluation (one subsection per benchmark)  
6. Discussion + limitations  
7. Conclusion  
8. References  
9. Optional: Appendix, Artifacts

---

## Which model to use for drafting

**Do not use one model for everything.** Split by task:

| Task | Recommended model | Why |
|------|-------------------|-----|
| **Outline & section structure** | Claude Sonnet / Opus, or GPT-4 class | Strong at IMRaD structure and contribution framing |
| **Prose polish (intro, discussion)** | Claude Opus or Sonnet | Best academic tone; fewer hallucinated citations |
| **LaTeX tables & formatting** | **Human + template** | LLMs break `\cite`, labels, and table floats |
| **Literature search** | Exa / Semantic Scholar + human verify | Models invent papers; always check DOI |
| **Benchmark numbers** | **Never LLM** | Copy from `benchmarks/results/*.json` only |
| **Cheap VPS draft pass** | Cloudflare **Llama 3.3 70B** | Available on your VPS; good for first draft paragraphs |
| **Avoid for final draft** | Cerebras `zai-glm-4.7` | Puts output in `reasoning` field; weak on QA-style tasks |

### Recommended workflow

1. **You** lock numbers in JSON and tables (already done for LoCoMo).  
2. **Cloudflare Llama 3.3 70B** — expand bullet outline → paragraph draft per section.  
3. **Claude (ServerBrain / Cursor)** — rewrite for tone, add related-work sentences with verified cites.  
4. **You** — paste into `main.tex`, compile locally, fix LaTeX errors manually.  
5. **Second human pass** — abstract, limitations, and claims vs evidence.

### Prompt template for section drafting

```
You are writing the {section} of a systems research paper on FluctlightDB,
an agent memory engine. Use ONLY these frozen metrics: {paste JSON}.
Do not invent citations. Tone: ACL/arXiv systems paper. 2–4 paragraphs.
Separate retrieval metrics from LLM QA metrics.
```

---

## What still needs writing (before submit)

- [ ] ORCID created + linked to arXiv account (permanent author ID for "Ganesh S")
- [ ] Related work paragraph with 4–6 real citations (Mem0, Zep, MemGPT, HippoRAG)
- [ ] System figure (write path / read path diagram)
- [ ] LongMemEval full number OR remove table row and say "deferred"
- [ ] Limitations paragraph (CPU ingest, no multi-tenant eval at scale)
- [ ] Optional: comparison table vs Mem0/Zep on LoCoMo *same metric*

---

## Build PDF locally

```bash
cd papers/arxiv-v1
pdflatex main.tex
bibtex main
pdflatex main.tex
pdflatex main.tex
# → main.pdf
```

Upload `main.pdf` to arXiv; optionally upload source zip.

---

## Live viewer

Private VPS viewer (not public). Deploy with `papers/site/install-search-site.sh` on your host.

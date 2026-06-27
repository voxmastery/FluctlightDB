# IEEE Access submission checklist

Portal: https://ieeeaccess.ieee.org/

## Before you click Submit

- [ ] Build PDF: `cd papers/arxiv-v1 && ./build.sh` → `main.pdf`
- [ ] ORCID linked to IEEE account (0009-0006-7758-4114)
- [ ] Cover letter pasted from `cover-letter.md`
- [ ] Highlights pasted from `highlights.txt`
- [ ] **Declare preprint** if Zenodo DOI exists (not a blocker)
- [ ] Manuscript is **not** simultaneously at FGCS / IJIDS / another journal
- [ ] Figures compile in PDF (add system diagram if missing—optional but helps)
- [ ] References compile (`bibtex main` run in build.sh)
- [ ] APC budget: IEEE Access is **open access (~USD 1,850)**—confirm before submit

## Files to upload

| File | Path |
|------|------|
| Manuscript PDF | `papers/arxiv-v1/main.pdf` |
| Cover letter | `papers/submit/ieee-access/cover-letter.md` → PDF or paste |
| Supplementary (optional) | `benchmarks/results/2025-06-22.json` |

## Suggested keywords

agent memory, database systems, information retrieval, episodic memory, LoCoMo, embedded systems, provenance, AI agents

## After accept

1. Update `CITATION.cff` with IEEE DOI  
2. arXiv v2 with journal reference (if allowed)  
3. Post to LinkedIn / HN with **journal link**, not just GitHub  

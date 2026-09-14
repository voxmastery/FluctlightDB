# arXiv replacement (v1 → v2) — FluctlightDB

**Live now:** [arXiv:2608.12365v1](https://arxiv.org/abs/2608.12365) — submitted 2026-07-10, never revised.

## Why a replacement is owed

v1's abstract reports **99.0%** LoCoMo evidence recall. That figure was retracted on
2026-07-16: it was an artifact of applying `expand_session_neighbors(±3)` *after* retrieval,
crediting neighbours that were never retrieved. A trivial BM25 baseline also reaches ~99%
under that protocol, so it distinguishes nothing.

`main.tex` has carried the corrected numbers since 2026-07-16 and was rebuilt on 2026-09-04.
The published paper has not been updated, so the citable version overstates LoCoMo and omits
a disclosure the local version adds.

| | arXiv v1 (live) | local `main.tex` (correct) |
|---|---|---|
| LoCoMo headline | 99.0% | **96.8%** @k=150 raw, no expansion |
| tight-k | not reported | **72.6%** @k=5 (MiniLM) / 75.1% (mpnet) |
| E2E QA | not reported | ≈85% @k=15 (retrieval-bound) |
| provenance conflict (n=50) | **absent** | 18% top-1 shared-brain vs 100% isolated |

## Procedure

arXiv calls this a **replacement**. The identifier stays `2608.12365`; it becomes **v2**.
v1 remains permanently accessible — correct practice for a correction, not a problem.

1. Log in at <https://arxiv.org> with the submitting account → user dashboard → find the
   article → **Replace**.
2. Upload `papers/arxiv-v1/fluctlightdb-arxiv-source.zip`
   (`main.tex`, `references.bib`, `main.bbl`, `figures/*.pdf`, flat layout).
   Rebuild it first with `bash papers/arxiv-v1/build.sh` if `main.tex` changed.
3. **Update the abstract in the metadata form.** arXiv's abstract is a metadata field, *not*
   extracted from the PDF — uploading alone leaves "99.0%" on the listing page forever.
   Paste the plain-text abstract (strip LaTeX; `build.sh` does not generate this, see below).
4. Fill **Comments**, e.g.:

   > v2: corrects the LoCoMo headline. v1 reported 99.0% evidence recall, an artifact of ±3
   > neighbor expansion applied after retrieval; the honest raw figure with no expansion is
   > 96.8% @k=150 and 72.6% @k=5. Adds tight-k results, an end-to-end QA figure, and a graded
   > provenance-conflict suite (18% top-1 shared-brain vs 100% isolated).

5. Submit. Moderation is typically same-day; announcements go out Sun–Fri 20:00 ET, so expect
   v2 live within about one business day.

### Generating the plain-text abstract

```bash
sed -n '/\\begin{abstract}/,/\\end{abstract}/p' papers/arxiv-v1/main.tex \
  | sed -e '1d;$d' | tr '\n' ' ' \
  | sed -e 's/\\textbf{\([^}]*\)}/\1/g' -e 's/\\emph{\([^}]*\)}/\1/g' \
        -e 's/\\texttt{\([^}]*\)}/\1/g' -e 's/\$k{=}\([0-9]*\)\$/k=\1/g' \
        -e 's/\$\\approx\$//g' -e 's/(\$n{=}\([0-9]*\)\$)/(n=\1)/g' \
        -e 's/\\%/%/g' -e 's/1{,}982/1,982/g' -e 's/---/ -- /g' \
        -e 's/\\ / /g' -e 's/\\_/_/g' -e 's/  */ /g'
```

## After v2 announces

- Update `CITATION.cff` — drop the "v1 carries the deprecated figure" note.
- Update `benchmarks/results/paper-2026-07-09.json` `"arxiv"` field likewise.
- `README.md` already links `arXiv:2608.12365` (version-agnostic, no change needed).

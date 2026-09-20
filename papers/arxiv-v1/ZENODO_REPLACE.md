# Zenodo record correction — FluctlightDB

**Live now:** [10.5281/zenodo.20949890](https://doi.org/10.5281/zenodo.20949890) — deposited 2026-06-27, never revised.

| | Zenodo record (live) | correct (arXiv v2 / `main.tex`) |
|---|---|---|
| LoCoMo headline | **98.1%** @k=150 | **96.8%** @k=150 raw, no expansion |
| tight-k | not reported | **72.6%** @k=5 |
| BEIR SciFact nDCG@10 | **0.645** ("matching Chroma") | **0.646** — 0.645 was *Chroma's* score |
| BEIR SciFact R@10 | absent | **0.792** vs Chroma 0.783 |
| FAMB | **97–98%** macro | **100%** macro (internal regression, not peer benchmark) |
| provenance conflict (n=50) | **absent** | 18% top-1 shared-brain vs 100% isolated |
| arXiv status | "submission pending" | **arXiv:2608.12365v2**, revised 2026-09-14 |

The 98.1% and the later 99.0% both came from applying `expand_session_neighbors(±3)` *after*
retrieval, crediting neighbours that were never retrieved. Retracted 2026-07-16. See
[`ARXIV_REPLACE.md`](ARXIV_REPLACE.md) for the same correction on the arXiv side (already applied).

## Why this record matters more than arXiv v1 did

The version DOI `10.5281/zenodo.20949890` is the identifier third parties actually cite — including
the [TeleAI Awesome-Agent-Memory](https://github.com/TeleAI-UAGI/Awesome-Agent-Memory) listing
(entry #71). arXiv v2 is corrected; this record is not, so the most-cited pointer still serves
withdrawn numbers.

## DOI structure (read before choosing an action)

| DOI | Points at |
|---|---|
| `10.5281/zenodo.20949889` | **concept** DOI — always resolves to the newest version |
| `10.5281/zenodo.20949890` | **version** DOI — pinned to the v1.0 deposit; this is what gets cited |

A *New Version* mints a **new** version DOI. It does **not** change what `…890` serves, so anyone
following an existing citation still lands on 98.1%. A *metadata edit* changes `…890` in place but
cannot replace the attached PDF (Zenodo locks files on published records).

**Do both, in this order.** Step 1 stops the cited DOI serving withdrawn numbers today; step 2 gets
the corrected PDF onto the concept DOI.

## Step 1 — correct the metadata on the cited record (`…890`)

Payload is prepared: [`zenodo-metadata-v2.json`](zenodo-metadata-v2.json). It carries the arXiv v2
abstract plus an explicit correction notice naming the withdrawn figures.

```bash
export ZENODO_TOKEN=...        # zenodo.org → Applications → Personal access tokens
                               # scopes: deposit:write deposit:actions
ID=20949890

# 1. unlock the published record for metadata editing
curl -sS -X POST -H "Authorization: Bearer $ZENODO_TOKEN" \
  "https://zenodo.org/api/deposit/depositions/$ID/actions/edit"

# 2. push the corrected metadata
curl -sS -X PUT -H "Authorization: Bearer $ZENODO_TOKEN" \
  -H "Content-Type: application/json" \
  --data @papers/arxiv-v1/zenodo-metadata-v2.json \
  "https://zenodo.org/api/deposit/depositions/$ID"

# 3. REVIEW in the web UI before this line — publishing is irreversible
curl -sS -X POST -H "Authorization: Bearer $ZENODO_TOKEN" \
  "https://zenodo.org/api/deposit/depositions/$ID/actions/publish"
```

Equivalent in the web UI: open the record → **Edit** → replace Description → **Publish**.

## Step 2 — new version carrying the corrected PDF

```bash
NEW=$(curl -sS -X POST -H "Authorization: Bearer $ZENODO_TOKEN" \
  "https://zenodo.org/api/deposit/depositions/20949890/actions/newversion" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["links"]["latest_draft"].rsplit("/",1)[-1])')

BUCKET=$(curl -sS -H "Authorization: Bearer $ZENODO_TOKEN" \
  "https://zenodo.org/api/deposit/depositions/$NEW" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["links"]["bucket"])')

# drop the v1 PDF from the draft, then upload the rebuilt one
curl -sS -X PUT -H "Authorization: Bearer $ZENODO_TOKEN" \
  --upload-file papers/arxiv-v1/main.pdf \
  "$BUCKET/fluctlightdb-preprint-v2.pdf"

curl -sS -X PUT -H "Authorization: Bearer $ZENODO_TOKEN" \
  -H "Content-Type: application/json" \
  --data @papers/arxiv-v1/zenodo-metadata-v2.json \
  "https://zenodo.org/api/deposit/depositions/$NEW"
# review, then publish as in step 1
```

## Caveats

- Zenodo is migrating to the InvenioRDM API. The legacy `/api/deposit/depositions` endpoints above
  still work but are not guaranteed; if a call 404s, use the web UI, which does the same thing.
- **These commands have not been executed or tested against the live API** — they need a token this
  repo does not carry. Verify each response before publishing.
- `publication_date` is deliberately left at the original `2026-06-27`. Zenodo treats it as the date
  of the work, not of the edit.
- `.zenodo.json` at the repo root keeps `license: mit` (it archives the *software* on GitHub
  release); the paper payload uses `cc-by-4.0`, matching the live record. Reconcile if you want one
  license across both.

## After publishing

- [ ] Confirm `https://doi.org/10.5281/zenodo.20949890` shows 96.8%, not 98.1%
- [ ] Update `hub/README.md` (still pins 0.5.9 and the July results table)
- [ ] Refresh the Hugging Face paper card `Voxiesz/fluctlightdb-paper` — it still says 98.1% and
      "97–98% FAMB"
- [ ] Open a PR against TeleAI Awesome-Agent-Memory only if the entry text needs changing; the DOI
      itself stays valid

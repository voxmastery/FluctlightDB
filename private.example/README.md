# Private files (do not commit to the public repo)

Copy this folder outside the repository for local-only work:

```bash
mkdir -p ~/fluctlightdb-private
cp -r private.example/* ~/fluctlightdb-private/
```

Suggested layout:

```
~/fluctlightdb-private/
├── grants/              # Grant proposals, APPLY_NOW, supporting docs
├── outreach/            # HN, LinkedIn, Dev.to, awesome-list PR drafts
├── launch/              # WORLD_LAUNCH.md, launch-paper.sh, release-notes
├── venues/              # RESEARCH_VENUES.md, PLATFORMS.md, PUBLISH_PAPER.md
├── ieee-access/         # Journal submission kit
├── funding.json         # FOSS funding manifest (if you use grants)
└── papers-site/         # Optional private VPS deploy scripts
```

The public repo `.gitignore` blocks these paths if you create them inside the tree.
Keep strategy, grants, and outreach **outside** `FluctlightDB/` or in a **private GitHub repo**.

Public repo keeps: LaTeX (`papers/arxiv-v1/`), benchmarks, `docs/BENCHMARKS.md`, technical contributor docs.

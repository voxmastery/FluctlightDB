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
├── funding.json         # FOSS funding manifest (if you use grants)
├── ieee-access/         # Journal submission kit (cover letter, checklist)
└── papers-site/         # Optional private VPS deploy scripts
```

The public repo `.gitignore` blocks these paths if you create them inside the tree.
Keep grants and paid-journal workflows **outside** `FluctlightDB/` or in a **private GitHub repo**.

Title: FluctlightDB – SQLite for agent memory (98.1% LoCoMo evidence recall, MIT, open source)

---

We built an embedded **database engine** for AI agent memory—not a vector DB wrapper, not a chat extraction layer.

Native API: `experience()` to encode episodes, `activate()` to recall from a cue, provenance so trusted sources beat chat.

**Numbers (frozen, reproducible harnesses in repo):**
- LoCoMo evidence recall: **98.1%** on full 10-conversation set (1,982 gold spans, k=150)
- BEIR SciFact nDCG@10: **0.645** (ties Chroma + MiniLM)
- FAMB agent-memory suite: **97–98%** macro

The claim: agent memory is a **third data model** (after relational + vector). Rows answer "which records match?" Vectors answer "what's nearest?" Agents need "what did I learn, and what can I trust?"

```bash
pip install "fluctlightdb[native]"
```

Links:
- Code: https://github.com/voxmastery/FluctlightDB
- Paper (LaTeX): https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1
- HF: https://huggingface.co/Voxiesz/fluctlightdb-paper
- Launch playbook: https://github.com/voxmastery/FluctlightDB/blob/main/docs/WORLD_LAUNCH.md

Zenodo DOI: _(add after release — see WORLD_LAUNCH.md)_

I'm the author (Ganesh S, independent). Happy to discuss metrics—LoCoMo **evidence recall** is not the same as end-to-end LLM QA scores some memory layers report.

---

**Posting:** Tuesday–Thursday 9–11am US Eastern. Reply to every comment for 4 hours.

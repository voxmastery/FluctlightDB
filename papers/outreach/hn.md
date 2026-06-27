Title suggestion: **FluctlightDB – embedded memory engine for AI agents (98.1% LoCoMo evidence recall)**

---

We built FluctlightDB because vector search alone isn't agent memory.

It's an embedded Rust engine (Python: `pip install fluctlightdb[native]`) with native `experience()` / `activate()` — encode episodes, recall from cues, boost verified sources over chat.

**Benchmarks (frozen, reproducible):**
- LoCoMo evidence recall: **98.1%** on full 10-conversation set
- BEIR SciFact nDCG@10: **0.645** (ties Chroma + MiniLM)
- FAMB: 97–98% macro

Preprint draft: https://voxmastery.github.io/FluctlightDB/
Code: https://github.com/voxmastery/FluctlightDB

MIT. Happy to discuss the "third data model" framing vs Mem0/Zep-style layers.

---

**Posting tips:** Post Tue–Thu morning US time. Link preprint first, GitHub second. Be ready to clarify evidence recall vs LLM QA metrics in comments.

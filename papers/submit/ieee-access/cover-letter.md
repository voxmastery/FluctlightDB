# Cover letter — IEEE Access

**Journal:** IEEE Access  
**Article type:** Research Article  
**Title:** FluctlightDB: A Memory Model of Data for AI Agents  

---

Dear Editor,

Please consider our manuscript for publication in **IEEE Access**.

**What we claim:** Long-term agent memory is a **third data model**—distinct from relational facts and vector similarity—with its own write semantics (`experience`: encode, separate, consolidate, provenance) and read semantics (`activate`: cue-driven hybrid retrieval over a memory graph).

**What we ship:** FluctlightDB, an embedded open-source engine (MIT) exposing that model as native API primitives, one durable store per agent—analogous to SQLite for application data, but for agent memory.

**Headline evidence (reproducible, frozen JSON in repository):**
- **LoCoMo** (10 conversations, 1,982 gold evidence spans): **98.1%** mean evidence recall @ k=150 (warm = cold-start).
- **BEIR SciFact:** nDCG@10 **0.645**, matching Chroma + MiniLM at equal latency; agent mode improves Recall@100.
- **FAMB** (agent-specific suite): **97–98%** macro on paraphrase recall, provenance ranking, persistence, separation.

**Why IEEE Access:** The work sits at the intersection of **data systems**, **information retrieval**, and **autonomous agents**—a broad systems contribution with immediate relevance to the AI engineering community. We provide open harnesses and frozen benchmarks so results are verifiable.

**Preprint status:** A preprint may be deposited on Zenodo and (pending endorsement) arXiv cs.DB before publication. This manuscript is not under consideration elsewhere. We will declare any DOI updates at proof stage per IEEE Access policy.

**Suggested reviewers (optional):** Researchers in agent memory systems (Mem0, Zep, MemGPT), vector/ hybrid retrieval, and embedded database engines.

Thank you for your consideration.

Sincerely,  
**Ganesh S**  
Independent Researcher  
voxmastery@ambugo.tech  
ORCID: 0009-0006-7758-4114  
https://github.com/voxmastery/FluctlightDB

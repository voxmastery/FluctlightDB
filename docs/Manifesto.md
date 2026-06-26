# FluctlightDB Manifesto

## A brain for agents — not a database for documents

Agents today remember by scraping context windows and vector stores. That is lookup, not life.

FluctlightDB exists so agents can:

- **Experience** moments with context, outcome, and salience
- **Remember** through engrams and activation — the way minds recall, not the way search engines rank
- **Grow** from newborn to expert through use, sleep, and pruning
- **Feel alive** — reward, surprise, and consolidation shape what matters
- **Persist** identity across sessions without pretending JSON files are a soul

## Principles

1. **Memory is physical** — engrams are neuron ensembles + synapses, not rows or vectors.
2. **Recall is activation** — thoughts spread through a graph; they are not similarity searches.
3. **Learning is plasticity** — Hebbian strengthening, neuromodulator gating, sleep consolidation.

   In product terms (see [README](../README.md#what-we-mean-by-learning)): **learning is not model training**. It is **operational memory** — `experience()` to encode, `activate()` to recall under new cues, `sleep()` / `checkpoint()` to consolidate. The brain file gets richer and more linked the longer the agent lives.

4. **Growth is developmental** — baby → adult; capability emerges from living, not from config.
5. **Life has chapters** — episodic memory can reset; core identity can endure.
6. **No vector DB as primary store** — vectors may assist later; they are not the mind.

## Who this is for

Any autonomous or semi-autonomous agent that should get **smarter over time**:

- Coding and dev agents
- Research and analysis agents
- Game and simulation characters
- Personal assistants with long-term continuity
- Multi-agent systems where each agent carries its own brain

FluctlightDB is **agent infrastructure**, not tied to any single product or company.

## Long-term vision

FluctlightDB aims to be **foundational memory infrastructure** for durable, trustworthy autonomy: the layer between a stateless LLM call and systems that must run for weeks, integrate tools and files as evidence, and carry continuity across sessions and agents.

We are building the **database engine for that layer** — SQLite for *what agents learn* — not claiming to be AGI. Any serious path toward general, long-horizon autonomous intelligence still needs a third data model for *what was learned and what can be trusted*; relational and vector stores were not designed to answer that question.

## What we reject

- SQLite with hippocampus table names
- pgvector / Pinecone / Weaviate as the *memory model*
- "Memory" as markdown files the LLM re-reads every turn
- Static agents that never mature

## What we build

A `.flct` brain file that ** grows, sleeps, and learns** with the agent — helping it become more capable, more coherent, and more *present* the longer it lives.

## Verify the build matches this manifesto

From a clone:

```bash
./scripts/manifesto-audit.sh
```

This runs automated pass/fail checks for activation recall, provenance (ledger beats chat), separation gate, sleep/growth, and life chapters.

# FluctlightDB

**A brain-native database for AI agents.** Not a vector database. Not SQL.

FluctlightDB gives agents a mind they can **grow** — where they **live, experience, remember, sleep, and learn** from a brain shaped like biological memory, not a search index.

Built for any agent runtime: coding assistants, autonomous workers, game NPCs, research agents, personal companions. One `.flct` brain file per agent. The agent gets smarter and more *alive* over time because it **experiences** life, not because it reads more documents.

## Why agents need this

| Today | Problem | FluctlightDB |
|-------|---------|--------------|
| Vector DB | Similarity ≠ memory | **Engrams** + spreading **activation** |
| SQL / KV | Facts without life | **Regions**, **plasticity**, **development** |
| RAG | Memory outside the agent | **Lived experience** inside the agent |
| Prompt history | Forgets, no growth | **Sleep**, **consolidation**, **maturation** |

## What it does for agents

- **Experience** — every action, outcome, and context becomes an engram
- **Recall** — `ACTIVATE(cue)` spreads activation through what the agent *lived*, not vector search
- **Grow** — developmental stages unlock automatically (newborn → expert)
- **Sleep** — replay and consolidate memories offline; prune noise
- **Persist identity** — core memories survive resets; episodic memory can be scoped per life
- **Get livelier** — neuromodulators (reward, surprise, arousal) gate what sticks

## Core primitives

- **Engram** — sparse neuron ensemble = one memory trace
- **Synapse** — weighted connection with plasticity state
- **SEPARATE()** — dentate gyrus pattern separation (similar events stay distinct)
- **ACTIVATE(cue)** — spreading activation recall
- **COMPLETE(cue)** — pattern completion from partial cue
- **SLEEP()** — replay, consolidation, pruning
- **tick()** — autonomic heartbeat + auto-sleep in the background

## Developmental growth (automatic)

The brain **matures by itself** from experience + sleep — no manual stage flags:

```
embryonic → newborn → infant → child → adolescent → adult → expert
```

Each stage unlocks capabilities: faster recall (myelination), executive control (PFC), smarter pruning, higher synapse capacity.

## Quick start

```bash
cargo build --release
./target/release/fluctlight status
./target/release/fluctlight experience "user asked for refactor" coding-session
./target/release/fluctlight activate "refactor"
./target/release/fluctlight tick 5          # autonomic heartbeat + auto-sleep
./target/release/fluctlight demo-separate   # pattern separation demo
./target/release/fluctlight export-viz      # → ~/.fluctlight/brain-viz.json
# Open docs/visual.html in browser and load the JSON file
```

## Rust API

```rust
use fluctlightdb::{Episode, FluctlightBrain};

let mut brain = FluctlightBrain::open("/path/to/agent.brain.flct").unwrap();

// Agent lives a moment
brain.experience(Episode {
    content: "fixed race condition in cache".into(),
    context: "debugging session".into(),
    outcome: Some("tests pass".into()),
    salience_hint: 0.8,
}).unwrap();

// Agent remembers by activation, not embedding search
let recalls = brain.activate("race condition cache");

// Background maturation
brain.tick().unwrap();
```

## Integrate with your agent

1. Call `experience()` after every meaningful turn (tool result, user message, outcome).
2. Call `activate(cue)` before planning — inject what the agent has lived.
3. Run `tick()` on a timer or after idle — sleep and growth happen automatically.
4. Mark `core` memories for values and identity that must survive resets.

## Quick start

```bash
cargo build --release
fluctlight shell --local          # interactive REPL
fluctlight serve --addr 127.0.0.1:8792   # HTTP API (multi-tenant / remote)
```

**Python agents — library call (recommended, like `sqlite3`):**

```bash
./scripts/install-native.sh       # builds PyO3 extension
export FLUCTLIGHT_NATIVE=1
python3 -c "from fluctlightdb import get_recall_client; print(get_recall_client().activate('hello'))"
```

Open source: [github.com/voxmastery/FluctlightDB](https://github.com/voxmastery/FluctlightDB)

## Docs

- **[Getting started](docs/GETTING_STARTED.md)** — UX vs SQL/vector, 5-minute tutorial  
- [CLI.md](docs/CLI.md) — command mapping + REPL  
- [DEPLOYMENT.md](docs/DEPLOYMENT.md) — replicas, backup, industrial HA  
- [Manifesto.md](docs/Manifesto.md) — philosophy  
- `docs/DevStages.md` — automatic growth stages
- `docs/visual.html` — brain visualizer

## License

Dual-licensed under **MIT OR Apache-2.0** (same as the Rust ecosystem standard).

- `LICENSE-MIT`
- `LICENSE-APACHE`

For GitHub: choose **MIT** as the displayed license if you only pick one; the repo legally offers either license.

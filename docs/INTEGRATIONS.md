# Framework integrations

## Canonical entry: `connect_agent()`

```python
from fluctlightdb import connect_agent

brain = connect_agent("/data/my-agent.brain", retain_days=30)
brain.turn_begin()
brain.wm_push("Use pytest for tests", context="project", salience=0.7)
hits = brain.recall("pytest", mode="auto")  # lexical overlap — not "testing framework"
fact = brain.resolve("pytest")
brain.turn_end(flush=True)
```

## LangChain

```bash
pip install "fluctlightdb[langchain]"
```

```python
from fluctlightdb import connect_agent
from fluctlightdb.integrations.langchain import FluctlightMemory, FluctlightChatMessageHistory

brain = connect_agent()
memory = FluctlightMemory(brain=brain)
history = FluctlightChatMessageHistory(brain=brain, session_id="chat-1")
```

## OpenAI Agents SDK

```python
from fluctlightdb import connect_agent
from fluctlightdb.integrations.openai_agents import FluctlightAgentsMemory

mem = FluctlightAgentsMemory(connect_agent())
handlers = mem.handlers()  # remember_memory, search_memory
```

## LlamaIndex

```bash
pip install "fluctlightdb[llamaindex]"
```

```python
from fluctlightdb.integrations.llamaindex import FluctlightLlamaMemory
memory = FluctlightLlamaMemory(connect_agent())
```

## TypeScript / Node

```bash
# HTTP path (fluctlight-serve)
import { connectAgent } from "@fluctlightdb/agent/agent";
const brain = connectAgent({ baseUrl: "http://127.0.0.1:8792" });
await brain.remember("dark mode preference");
```

## MCP (Cursor / Claude / Codex)

```bash
pip install "fluctlightdb[native,mcp]"
```

Standard tools: `memory_remember`, `memory_recall`, `memory_resolve`, `memory_consolidate`, `memory_observe_tool`.

## Reflex auto-ingest

```python
from fluctlightdb import connect_agent
from fluctlightdb.reflex import reflex_ingest_turn

brain = connect_agent()
reflex_ingest_turn(
    brain,
    user_text="We decided to use PostgreSQL 16",
    assistant_text="I'll remember that for migrations.",
    tools=[{"name": "grep", "result": "found postgres in docker-compose.yml"}],
)
```

Cursor hook template: `sdks/python/fluctlightdb/templates/cursor/reflex_hook.py`

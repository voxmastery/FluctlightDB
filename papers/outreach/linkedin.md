We shipped a preprint on something we've been building for a while: **FluctlightDB** — an embedded database engine for AI agent memory.

Not another vector DB wrapper. The native API is `experience()` and `activate()` — encode episodes, recall from cues, rank trusted sources above chat.

**Headline result:** 98.1% evidence recall on the full LoCoMo benchmark (1,982 gold spans, hybrid retrieval).

The argument: agent memory is a third data model alongside relational facts and vector similarity. Rows answer "which records match?" Vectors answer "what's nearest?" Agents need "what did I learn, and what can I trust?"

📄 Preprint: https://voxmastery.github.io/FluctlightDB/
💻 GitHub: https://github.com/voxmastery/FluctlightDB
📦 pip install fluctlightdb[native]

Open source (MIT). arXiv submission coming — this is the public draft.

#AI #Agents #OpenSource #Database #LLM

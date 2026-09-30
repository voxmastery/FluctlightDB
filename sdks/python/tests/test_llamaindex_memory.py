"""LlamaIndex adapter: constructible against current BaseMemory."""

from __future__ import annotations

import re
import unittest

try:
    from fluctlightdb.integrations.llamaindex import FluctlightLlamaMemory
    from llama_index.core.llms import ChatMessage, MessageRole
    from llama_index.core.memory import BaseMemory
except ImportError:
    FluctlightLlamaMemory = None  # type: ignore[misc, assignment]
    ChatMessage = None  # type: ignore[misc, assignment]
    MessageRole = None  # type: ignore[misc, assignment]
    BaseMemory = None  # type: ignore[misc, assignment]


def _tokens(text: str) -> set[str]:
    return {tok for tok in re.split(r"[^0-9A-Za-z]+", text.lower()) if len(tok) >= 2}


class _LexicalBrain:
    """Recall matches stored text only, same gap as episodic content tokens."""

    def __init__(self) -> None:
        self.slots: list[dict[str, str]] = []

    def wm_push(self, content: str, context: str = "turn", salience: float = 0.6, **_kwargs: object) -> None:
        self.slots.append({"content": content, "context": context})

    def recall(self, cue: str, mode: str = "auto", limit: int = 8, **_kwargs: object) -> dict:
        cue_toks = _tokens(cue)
        hits = [
            slot
            for slot in self.slots
            if cue_toks & _tokens(slot["content"])
        ]
        return {"hits": hits[:limit]}

    def turn_begin(self) -> None:
        return None

    def turn_end(self, flush: bool = True) -> dict:
        return {"committed": len(self.slots) if flush else 0}


@unittest.skipUnless(FluctlightLlamaMemory is not None, "llama-index is not installed")
class TestFluctlightLlamaMemory(unittest.TestCase):
    def test_constructs_positionally_and_from_defaults(self) -> None:
        brain = _LexicalBrain()
        memory = FluctlightLlamaMemory(brain)
        self.assertIsInstance(memory, BaseMemory)
        self.assertEqual(memory.session_id, "llamaindex")

        via_defaults = FluctlightLlamaMemory.from_defaults(brain=brain, session_id="chat-9")
        self.assertIsInstance(via_defaults, BaseMemory)
        self.assertIs(via_defaults.brain, brain)
        self.assertEqual(via_defaults.session_id, "chat-9")

    def test_put_get_roundtrip(self) -> None:
        brain = _LexicalBrain()
        memory = FluctlightLlamaMemory.from_defaults(brain=brain, session_id="llama-session")
        memory.put(ChatMessage(role=MessageRole.USER, content="store the indigo finch"))
        memory.put(ChatMessage(role=MessageRole.ASSISTANT, content="finch stored"))

        other = FluctlightLlamaMemory(brain, session_id="other-session")
        other.put(ChatMessage(role=MessageRole.USER, content="do not leak this turn"))

        messages = memory.get()
        texts = [str(message.content) for message in messages]
        self.assertEqual(texts, ["store the indigo finch", "finch stored"])
        roles = [message.role for message in messages]
        self.assertEqual(roles[0], MessageRole.USER)
        self.assertEqual(roles[1], MessageRole.ASSISTANT)


if __name__ == "__main__":
    unittest.main()

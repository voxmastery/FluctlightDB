"""LangChain adapter: importable BaseMemory and session history retrieval."""

from __future__ import annotations

import importlib.util
import os
import re
import shutil
import tempfile
import unittest
from pathlib import Path

_HAS_NATIVE = importlib.util.find_spec("fluctlightdb_native") is not None

try:
    from fluctlightdb.integrations.langchain import (
        FluctlightChatMessageHistory,
        FluctlightMemory,
    )
    from langchain_core.memory import BaseMemory
    from langchain_core.messages import AIMessage, HumanMessage
except ImportError:
    FluctlightChatMessageHistory = None  # type: ignore[misc, assignment]
    FluctlightMemory = None  # type: ignore[misc, assignment]
    BaseMemory = None  # type: ignore[misc, assignment]
    HumanMessage = None  # type: ignore[misc, assignment]
    AIMessage = None  # type: ignore[misc, assignment]


def _tokens(text: str) -> set[str]:
    return {tok for tok in re.split(r"[^0-9A-Za-z]+", text.lower()) if len(tok) >= 2}


class _LexicalBrain:
    """Content-token recall. Context metadata is stored but not searched."""

    def __init__(self) -> None:
        self.slots: list[dict[str, str]] = []

    def wm_push(self, content: str, context: str = "turn", salience: float = 0.6, **_kwargs: object) -> None:
        self.slots.append({"content": content, "context": context})

    def observe_tool(self, tool_name: str, result: str, context: str | None = None, **_kwargs: object) -> dict:
        self.slots.append({"content": f"[{tool_name}] {result}", "context": context or ""})
        return {"stored": "hippocampus"}

    def recall(self, cue: str, mode: str = "auto", limit: int = 8, **_kwargs: object) -> dict:
        cue_toks = _tokens(cue)
        hits = [slot for slot in self.slots if cue_toks & _tokens(slot["content"])]
        return {"hits": hits[:limit]}

    def turn_begin(self) -> None:
        return None

    def turn_end(self, flush: bool = True) -> dict:
        return {"committed": len(self.slots) if flush else 0}


class TestLangchainExtra(unittest.TestCase):
    def test_extra_pins_langchain_core_where_memory_module_exists(self) -> None:
        pyproject = Path(__file__).resolve().parents[1] / "pyproject.toml"
        text = pyproject.read_text(encoding="utf-8")
        match = re.search(r"^langchain\s*=\s*\[(.*?)\]", text, re.M | re.S)
        self.assertIsNotNone(match, "fluctlightdb[langchain] extra missing")
        body = match.group(1)
        spec = re.search(r"langchain-core([^\"']+)", body)
        self.assertIsNotNone(spec, body)
        requirement = "langchain-core" + spec.group(1).strip().rstrip(",")
        try:
            from packaging.requirements import Requirement
        except ImportError:
            self.assertIn(">=0.2.0", requirement)
            self.assertIn("<1.0.0", requirement)
            return
        parsed = Requirement(requirement)
        self.assertTrue(parsed.specifier.contains("0.3.86"))
        self.assertFalse(parsed.specifier.contains("1.0.0"))
        self.assertFalse(parsed.specifier.contains("1.2.0"))


@unittest.skipUnless(FluctlightChatMessageHistory is not None, "langchain-core is not installed")
class TestFluctlightChatMessageHistory(unittest.TestCase):
    def test_adapter_imports_and_memory_subclasses_base(self) -> None:
        brain = _LexicalBrain()
        memory = FluctlightMemory(brain=brain)
        self.assertIsInstance(memory, BaseMemory)
        self.assertEqual(memory.memory_variables, ["history"])

    def test_messages_roundtrip_by_session(self) -> None:
        brain = _LexicalBrain()
        history = FluctlightChatMessageHistory(brain, session_id="chat-1")
        history.add_message(HumanMessage(content="remember the blue heron"))
        history.add_message(AIMessage(content="noted the heron"))

        other = FluctlightChatMessageHistory(brain, session_id="chat-2")
        other.add_message(HumanMessage(content="unrelated session note"))

        texts = [message.content for message in history.messages]
        self.assertEqual(texts, ["remember the blue heron", "noted the heron"])
        self.assertIsInstance(history.messages[0], HumanMessage)
        self.assertIsInstance(history.messages[1], AIMessage)
        self.assertFalse(any("unrelated" in str(text) for text in texts))

    @unittest.skipUnless(_HAS_NATIVE, "fluctlightdb[native] not installed")
    def test_messages_roundtrip_on_native_brain(self) -> None:
        from fluctlightdb import connect_agent

        tmp = tempfile.mkdtemp(prefix="flct-lc-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect_agent(os.path.join(tmp, "brain"))
        history = FluctlightChatMessageHistory(brain, session_id="chat-1")
        history.add_message(HumanMessage(content="remember the blue heron"))
        history.add_message(AIMessage(content="noted the heron"))
        other = FluctlightChatMessageHistory(brain, session_id="chat-2")
        other.add_message(HumanMessage(content="unrelated session note"))

        texts = [message.content for message in history.messages]
        self.assertEqual(texts, ["remember the blue heron", "noted the heron"])
        other_texts = [message.content for message in other.messages]
        self.assertEqual(other_texts, ["unrelated session note"])


if __name__ == "__main__":
    unittest.main()

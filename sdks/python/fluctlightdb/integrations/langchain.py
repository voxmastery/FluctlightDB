"""LangChain memory adapter for FluctlightDB agent brains."""

from __future__ import annotations

import json
from typing import Any, Optional

try:
    from langchain_core.chat_history import BaseChatMessageHistory
    from langchain_core.messages import AIMessage, BaseMessage, HumanMessage, SystemMessage, ToolMessage
    from langchain_core.memory import BaseMemory
except ImportError as exc:
    raise ImportError(
        "LangChain integration requires: pip install 'fluctlightdb[langchain]'"
    ) from exc


def _msg_to_text(message: BaseMessage) -> str:
    content = message.content
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for block in content:
            if isinstance(block, dict) and block.get("type") == "text":
                parts.append(str(block.get("text", "")))
            else:
                parts.append(str(block))
        return "\n".join(parts)
    return str(content)


class FluctlightChatMessageHistory(BaseChatMessageHistory):
    """Persist chat turns in FluctlightDB WM-Ring + hippocampus."""

    def __init__(self, brain: Any, *, session_id: str = "langchain") -> None:
        self.brain = brain
        self.session_id = session_id

    @property
    def messages(self) -> list[BaseMessage]:
        hits = self.brain.recall(f"session:{self.session_id}", mode="episodic", limit=32)
        out: list[BaseMessage] = []
        for hit in hits.get("hits", []):
            text = hit.get("content") or hit.get("snippet") or ""
            ctx = (hit.get("context") or "").lower()
            if "assistant" in ctx or ctx.startswith("ai"):
                out.append(AIMessage(content=text))
            elif "tool" in ctx:
                out.append(ToolMessage(content=text, tool_call_id="fluctlight"))
            else:
                out.append(HumanMessage(content=text))
        return out

    def add_message(self, message: BaseMessage) -> None:
        role = "user"
        if isinstance(message, AIMessage):
            role = "assistant"
        elif isinstance(message, SystemMessage):
            role = "system"
        elif isinstance(message, ToolMessage):
            role = "tool"
        text = _msg_to_text(message)
        if role == "tool":
            self.brain.observe_tool("langchain", text, context=f"session:{self.session_id}")
        else:
            self.brain.wm_push(text, context=f"{role}:{self.session_id}", salience=0.58)

    def clear(self) -> None:
        self.brain.turn_end(flush=False)


class FluctlightMemory(BaseMemory):
    """Drop-in LangChain memory: recall context + store new inputs/outputs."""

    brain: Any
    memory_key: str = "history"
    input_key: str = "input"
    output_key: str = "output"
    recall_limit: int = 8

    @property
    def memory_variables(self) -> list[str]:
        return [self.memory_key]

    def load_memory_variables(self, inputs: dict[str, Any]) -> dict[str, Any]:
        cue = str(inputs.get(self.input_key, "")) or "recent context"
        result = self.brain.recall(cue, mode="auto", limit=self.recall_limit)
        lines = []
        for hit in result.get("hits", []):
            text = hit.get("content") or hit.get("snippet") or ""
            if text:
                lines.append(text)
        return {self.memory_key: "\n".join(lines) if lines else ""}

    def save_context(self, inputs: dict[str, Any], outputs: dict[str, str]) -> None:
        self.brain.turn_begin()
        if self.input_key in inputs:
            self.brain.wm_push(str(inputs[self.input_key]), context="user", salience=0.6)
        if self.output_key in outputs:
            self.brain.wm_push(str(outputs[self.output_key]), context="assistant", salience=0.55)
        self.brain.turn_end(flush=True)

    def clear(self) -> None:
        self.brain.turn_end(flush=False)

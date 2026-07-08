"""LlamaIndex memory adapter for FluctlightDB."""

from __future__ import annotations

from typing import Any, Optional

try:
    from llama_index.core.memory import BaseMemory
    from llama_index.core.llms import ChatMessage, MessageRole
except ImportError as exc:
    raise ImportError(
        "LlamaIndex integration requires: pip install 'fluctlightdb[llamaindex]'"
    ) from exc


class FluctlightLlamaMemory(BaseMemory):
    """LlamaIndex chat memory backed by FluctlightDB WM-Ring + recall."""

    def __init__(self, brain: Any, *, session_id: str = "llamaindex") -> None:
        self.brain = brain
        self.session_id = session_id

    @classmethod
    def class_name(cls) -> str:
        return "FluctlightLlamaMemory"

    def get(self, input: Optional[str] = None, **kwargs: Any) -> list[ChatMessage]:
        cue = input or f"session:{self.session_id}"
        result = self.brain.recall(cue, mode="auto", limit=16)
        messages: list[ChatMessage] = []
        for hit in result.get("hits", []):
            text = hit.get("content") or hit.get("snippet") or ""
            ctx = (hit.get("context") or "").lower()
            role = MessageRole.USER
            if "assistant" in ctx:
                role = MessageRole.ASSISTANT
            elif "system" in ctx:
                role = MessageRole.SYSTEM
            messages.append(ChatMessage(role=role, content=text))
        return messages

    def get_all(self) -> list[ChatMessage]:
        return self.get()

    def put(self, message: ChatMessage) -> None:
        role = message.role.value if hasattr(message.role, "value") else str(message.role)
        salience = 0.62 if role == "user" else 0.55
        self.brain.wm_push(str(message.content), context=f"{role}:{self.session_id}", salience=salience)

    def set(self, messages: list[ChatMessage]) -> None:
        self.brain.turn_begin()
        for msg in messages:
            self.put(msg)
        self.brain.turn_end(flush=True)

    def reset(self) -> None:
        self.brain.turn_end(flush=False)

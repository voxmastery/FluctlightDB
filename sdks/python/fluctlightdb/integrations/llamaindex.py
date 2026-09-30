"""LlamaIndex memory adapter for FluctlightDB."""

from __future__ import annotations

from typing import Any, Optional

try:
    from llama_index.core.bridge.pydantic import ConfigDict, Field
    from llama_index.core.llms import ChatMessage, MessageRole
    from llama_index.core.memory import BaseMemory
except ImportError as exc:
    raise ImportError(
        "LlamaIndex integration requires: pip install 'fluctlightdb[llamaindex]'"
    ) from exc


def _session_marker(session_id: str) -> str:
    return f"session:{session_id}"


def _stamp(session_id: str, text: str) -> str:
    """Put the session cue in stored text so episodic recall can find the turn."""
    marker = _session_marker(session_id) + " "
    if text.startswith(marker):
        return text
    return marker + text


def _unstamp(session_id: str, text: str) -> str:
    marker = _session_marker(session_id) + " "
    if text.startswith(marker):
        return text[len(marker) :]
    return text


def _in_session(hit: dict[str, Any], session_id: str) -> bool:
    ctx = str(hit.get("context") or "")
    _role, sep, sid = ctx.rpartition(":")
    if sep and sid == session_id:
        return True
    text = str(hit.get("content") or hit.get("snippet") or "")
    return text.startswith(_session_marker(session_id) + " ")


class FluctlightLlamaMemory(BaseMemory):
    """LlamaIndex chat memory backed by FluctlightDB WM-Ring + recall."""

    model_config = ConfigDict(arbitrary_types_allowed=True)

    brain: Any = Field(exclude=True)
    session_id: str = "llamaindex"

    def __init__(self, brain: Any = None, **kwargs: Any) -> None:
        # Documented call is positional: FluctlightLlamaMemory(connect_agent()).
        if brain is not None and "brain" not in kwargs:
            kwargs["brain"] = brain
        super().__init__(**kwargs)

    @classmethod
    def class_name(cls) -> str:
        return "FluctlightLlamaMemory"

    @classmethod
    def from_defaults(cls, **kwargs: Any) -> "FluctlightLlamaMemory":
        """Build a memory. ``brain`` is required; ``session_id`` defaults to ``llamaindex``."""
        brain = kwargs.pop("brain", None)
        if brain is None:
            raise ValueError("FluctlightLlamaMemory.from_defaults requires brain")
        session_id = kwargs.pop("session_id", "llamaindex")
        if kwargs:
            raise ValueError(f"Unexpected kwargs: {kwargs}")
        return cls(brain=brain, session_id=session_id)

    def get(self, input: Optional[str] = None, **kwargs: Any) -> list[ChatMessage]:
        cue = input or _session_marker(self.session_id)
        result = self.brain.recall(cue, mode="auto", limit=16)
        messages: list[ChatMessage] = []
        for hit in result.get("hits", []):
            if not _in_session(hit, self.session_id):
                continue
            text = _unstamp(self.session_id, hit.get("content") or hit.get("snippet") or "")
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
        self.brain.wm_push(
            _stamp(self.session_id, str(message.content)),
            context=f"{role}:{self.session_id}",
            salience=salience,
        )

    def set(self, messages: list[ChatMessage]) -> None:
        self.brain.turn_begin()
        for msg in messages:
            self.put(msg)
        self.brain.turn_end(flush=True)

    def reset(self) -> None:
        self.brain.turn_end(flush=False)

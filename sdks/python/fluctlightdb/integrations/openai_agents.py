"""OpenAI Agents SDK memory tools for FluctlightDB."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Optional


@dataclass
class FluctlightAgentsMemory:
    """Memory backend exposing OpenAI Agents-style search/remember callables."""

    brain: Any
    namespace: str = "openai-agents"

    def remember(self, text: str, *, salience: float = 0.65, context: Optional[str] = None) -> dict[str, Any]:
        ctx = context or self.namespace
        return self.brain.experience(text, context=ctx, salience=salience)

    def search_memory(self, query: str, *, limit: int = 8) -> list[dict[str, Any]]:
        result = self.brain.recall(query, mode="auto", limit=limit)
        return list(result.get("hits", []))

    def resolve_fact(self, query: str) -> dict[str, Any]:
        return self.brain.resolve(query)

    def as_tools(self) -> list[dict[str, Any]]:
        """JSON-schema tool defs for Agents SDK `tools=` registration."""

        mem = self

        def remember_memory(text: str, salience: float = 0.65) -> str:
            out = mem.remember(text, salience=salience)
            return str(out.get("engram_id", out))

        def search_memory(query: str, limit: int = 8) -> str:
            hits = mem.search_memory(query, limit=limit)
            return "\n".join(
                f"- {(h.get('content') or h.get('snippet') or '')[:400]}"
                for h in hits
            )

        return [
            {
                "type": "function",
                "function": {
                    "name": "remember_memory",
                    "description": "Store a durable fact in FluctlightDB agent memory.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": {"type": "string"},
                            "salience": {"type": "number", "default": 0.65},
                        },
                        "required": ["text"],
                    },
                },
                "callable": remember_memory,
            },
            {
                "type": "function",
                "function": {
                    "name": "search_memory",
                    "description": "Recall relevant memories from FluctlightDB by cue.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"},
                            "limit": {"type": "integer", "default": 8},
                        },
                        "required": ["query"],
                    },
                },
                "callable": search_memory,
            },
        ]

    def handlers(self) -> dict[str, Callable[..., str]]:
        """Name → handler map for SDK tool dispatch."""
        return {t["function"]["name"]: t["callable"] for t in self.as_tools()}

"""Framework integrations — LangChain, LlamaIndex, OpenAI Agents SDK."""

from __future__ import annotations

__all__ = [
    "FluctlightChatMessageHistory",
    "FluctlightMemory",
    "get_langchain_memory",
    "get_openai_agents_memory",
    "get_llamaindex_memory",
]


def get_langchain_memory(brain, **kwargs):
    from .langchain import FluctlightChatMessageHistory, FluctlightMemory

    return FluctlightMemory(brain=brain, **kwargs)


def get_openai_agents_memory(brain, **kwargs):
    from .openai_agents import FluctlightAgentsMemory

    return FluctlightAgentsMemory(brain=brain, **kwargs)


def get_llamaindex_memory(brain, **kwargs):
    from .llamaindex import FluctlightLlamaMemory

    return FluctlightLlamaMemory(brain=brain, **kwargs)

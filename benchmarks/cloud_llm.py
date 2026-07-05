"""OpenAI-compatible chat backends for benchmarks (Gemini, Cerebras, Groq, OpenRouter)."""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

UA = "curl/8.5.0 FluctlightDB-bench/1.0"
ENV_CANDIDATES = (
    Path("/home/ambugo/litellm/.env"),
    Path(os.environ.get("LITELLM_ENV_FILE", "")),
)


def load_env_file(path: Path | None = None) -> None:
    paths = [path] if path else [p for p in ENV_CANDIDATES if p and str(p)]
    for p in paths:
        if not p.is_file():
            continue
        for line in p.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, _, v = line.partition("=")
            k, v = k.strip(), v.strip().strip('"').strip("'")
            if k and k not in os.environ:
                os.environ[k] = v


def _all_keys(prefix: str) -> list[str]:
    seen: set[str] = set()
    keys: list[str] = []
    for name in (f"{prefix}_API_KEY", f"{prefix}_KEY"):
        v = os.environ.get(name, "").strip()
        if v and v not in seen:
            seen.add(v)
            keys.append(v)
    for i in range(1, 9):
        v = os.environ.get(f"{prefix}_KEY_{i}", "").strip()
        if v and v not in seen:
            seen.add(v)
            keys.append(v)
    return keys


def _first_key(prefix: str) -> str:
    keys = _all_keys(prefix)
    return keys[0] if keys else ""


_key_rr: dict[str, int] = {}


def _next_key(prefix: str) -> str:
    keys = _all_keys(prefix)
    if not keys:
        return ""
    i = _key_rr.get(prefix, 0) % len(keys)
    _key_rr[prefix] = i + 1
    return keys[i]


def _http_chat(
    url: str,
    *,
    api_key: str,
    model: str,
    prompt: str,
    max_tokens: int = 512,
    timeout_s: int = 120,
    extra_headers: dict[str, str] | None = None,
) -> str:
    body: dict[str, Any] = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0,
        "max_tokens": max_tokens,
    }
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
        "User-Agent": UA,
    }
    if extra_headers:
        headers.update(extra_headers)
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers=headers,
        method="POST",
    )
    last_err: Exception | None = None
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=timeout_s) as resp:
                data = json.loads(resp.read().decode())
            break
        except urllib.error.HTTPError as e:
            detail = e.read().decode()[:500]
            last_err = RuntimeError(f"HTTP {e.code} {url}: {detail}")
            if e.code in (429, 503) and attempt < 4:
                time.sleep(min(60, 2 ** attempt * 5))
                continue
            raise last_err from e
    else:
        assert last_err is not None
        raise last_err
    if "error" in data:
        raise RuntimeError(str(data["error"]))
    msg = data["choices"][0]["message"]
    text = msg.get("content") or msg.get("reasoning") or ""
    return str(text).strip()


def _gemini_thinking_config(model: str) -> dict[str, int] | None:
    """Gemini 2.5+ Flash: thinking tokens consume maxOutputTokens; disable for benchmarks."""
    m = model.lower()
    if "2.5" in m and "flash" in m:
        return {"thinkingBudget": 0}
    return None


def _gemini_chat(
    *,
    api_key: str,
    model: str,
    prompt: str,
    max_tokens: int = 512,
    timeout_s: int = 120,
) -> str:
    url = (
        f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        f"?key={api_key}"
    )
    gen_cfg: dict[str, Any] = {"temperature": 0, "maxOutputTokens": max_tokens}
    thinking = _gemini_thinking_config(model)
    if thinking is not None:
        gen_cfg["thinkingConfig"] = thinking
    body: dict[str, Any] = {
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": gen_cfg,
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "User-Agent": UA},
        method="POST",
    )
    last_err: Exception | None = None
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=timeout_s) as resp:
                data = json.loads(resp.read().decode())
            break
        except urllib.error.HTTPError as e:
            detail = e.read().decode()[:500]
            last_err = RuntimeError(f"HTTP {e.code} gemini/{model}: {detail}")
            if e.code in (429, 503) and attempt < 4:
                time.sleep(min(60, 2 ** attempt * 5))
                continue
            raise last_err from e
    else:
        assert last_err is not None
        raise last_err
    if "error" in data:
        raise RuntimeError(str(data["error"]))
    cands = data.get("candidates") or []
    if not cands:
        pf = data.get("promptFeedback") or {}
        raise RuntimeError(f"gemini blocked/empty: {str(pf)[:300] or str(data)[:300]}")
    parts = (cands[0].get("content") or {}).get("parts") or []
    text = "".join(
        str(p.get("text") or "")
        for p in parts
        if not p.get("thought")  # skip internal reasoning parts if present
    )
    if not text.strip():
        fr = cands[0].get("finishReason") or ""
        raise RuntimeError(f"gemini empty text (finishReason={fr})")
    return text.strip()


PROVIDERS: dict[str, dict[str, str]] = {
    "gemini": {
        "url": "",
        "key_prefix": "GEMINI",
        "default_model": "gemini-2.5-flash",
    },
    "openrouter": {
        "url": "https://openrouter.ai/api/v1/chat/completions",
        "key_prefix": "OPENROUTER",
        "default_model": "openai/gpt-4o",
    },
    "cerebras": {
        "url": "https://api.cerebras.ai/v1/chat/completions",
        "key_prefix": "CEREBRAS",
        "default_model": "gpt-oss-120b",
    },
    "groq": {
        "url": "https://api.groq.com/openai/v1/chat/completions",
        "key_prefix": "GROQ",
        "default_model": "llama-3.3-70b-versatile",
    },
    "openai": {
        "url": "https://api.openai.com/v1/chat/completions",
        "key_prefix": "OPENAI",
        "default_model": "gpt-4o-2024-08-06",
    },
}


def chat(
    prompt: str,
    *,
    provider: str,
    model: str | None = None,
    max_tokens: int = 512,
    timeout_s: int = 120,
) -> str:
    load_env_file()
    cfg = PROVIDERS.get(provider)
    if not cfg:
        raise ValueError(f"unknown provider {provider!r}; use {list(PROVIDERS)}")
    m = model or os.environ.get(f"{provider.upper()}_MODEL") or cfg["default_model"]
    if provider == "gemini":
        key = _next_key(cfg["key_prefix"])
        if not key:
            raise RuntimeError("No GEMINI_API_KEY (Colab Secrets or litellm .env)")
        return _gemini_chat(api_key=key, model=m, prompt=prompt, max_tokens=max_tokens, timeout_s=timeout_s)
    key = _next_key(cfg["key_prefix"])
    if not key and provider == "openai":
        key = os.environ.get("OPENAI_API_KEY", "").strip()
    if not key:
        raise RuntimeError(f"No API key for provider {provider} (check litellm .env or Colab Secrets)")
    extra: dict[str, str] = {}
    if provider == "openrouter":
        extra["HTTP-Referer"] = os.environ.get("OPENROUTER_REFERER", "https://github.com/voxmastery/FluctlightDB")
        extra["X-Title"] = "FluctlightDB LongMemEval E2E"
    return _http_chat(
        cfg["url"],
        api_key=key,
        model=m,
        prompt=prompt,
        max_tokens=max_tokens,
        timeout_s=timeout_s,
        extra_headers=extra,
    )


def smoke_test(provider: str) -> str:
    # Gemini 2.5 Flash needs headroom even for one-word replies if thinking is on.
    max_t = 128 if provider == "gemini" else 16
    return chat(f"Reply with exactly: {provider}-ok", provider=provider, max_tokens=max_t, timeout_s=60)

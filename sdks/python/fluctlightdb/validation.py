"""Input validation for project brain writes."""

from __future__ import annotations

import os
import re
from typing import Optional

DEFAULT_MAX_CONTENT = 8192

_SECRET_PATTERNS = (
    re.compile(r"ghp_[A-Za-z0-9]{20,}"),
    re.compile(r"github_pat_[A-Za-z0-9_]{20,}"),
    re.compile(r"sk-[A-Za-z0-9]{20,}"),
    re.compile(r"AKIA[0-9A-Z]{16}"),
    re.compile(r"-----BEGIN (?:RSA |EC )?PRIVATE KEY-----"),
)


def max_content_length() -> int:
    raw = os.environ.get("FLUCTLIGHT_MAX_CONTENT", "")
    if raw.isdigit():
        return max(256, int(raw))
    return DEFAULT_MAX_CONTENT


def allow_secrets() -> bool:
    return os.environ.get("FLUCTLIGHT_ALLOW_SECRETS", "").lower() in ("1", "true", "yes")


def validate_content(content: str, *, field: str = "content") -> None:
    limit = max_content_length()
    if len(content) > limit:
        raise ValueError(f"{field} exceeds {limit} bytes (set FLUCTLIGHT_MAX_CONTENT to raise)")
    if allow_secrets():
        return
    for pattern in _SECRET_PATTERNS:
        if pattern.search(content):
            raise ValueError(
                f"{field} looks like a secret/credential — refused "
                "(set FLUCTLIGHT_ALLOW_SECRETS=1 to override)"
            )


def warn_serve_embedded_conflict() -> Optional[str]:
    if os.environ.get("FLUCTLIGHT_SERVE_URL", "").strip():
        return (
            "FLUCTLIGHT_SERVE_URL is set — embedded project brains may contend with "
            "fluctlight-serve locks. See docs/MULTI_AGENT.md#serve-vs-embedded."
        )
    return None

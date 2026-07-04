"""Cursor Cloud Agents HTTP API (CURSOR_API_KEY + Auto). No SDK, no OpenAI."""

from __future__ import annotations

import base64
import json
import os
import time
import urllib.error
import urllib.request
from pathlib import Path

CURSOR_API_BASE = os.environ.get("CURSOR_API_BASE", "https://api.cursor.com").rstrip("/")
CURSOR_AUTO_MODEL = "default"  # GET /v1/models → displayName Auto, alias auto
CURSOR_ENV_CANDIDATES = (
    Path("/opt/ambugo/serverbrain/.env"),
    Path.home() / ".cursor" / ".env",
)

TERMINAL_RUN_STATUSES = frozenset(
    {"FINISHED", "FAILED", "ERROR", "CANCELLED", "CANCELED", "STOPPED"}
)


def load_cursor_api_key() -> str:
    key = os.environ.get("CURSOR_API_KEY", "").strip()
    if key:
        return key
    env_file = os.environ.get("CURSOR_ENV_FILE", "").strip()
    paths = [Path(env_file)] if env_file else list(CURSOR_ENV_CANDIDATES)
    for path in paths:
        if not path.is_file():
            continue
        for line in path.read_text().splitlines():
            if line.startswith("CURSOR_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return ""


def resolve_cursor_model(model: str) -> str:
    m = (model or "auto").strip().lower()
    if m in ("auto", "default"):
        return CURSOR_AUTO_MODEL
    return model


def _auth_header(api_key: str) -> str:
    token = base64.b64encode(f"{api_key}:".encode()).decode()
    return f"Basic {token}"


def cursor_api_request(
    method: str,
    path: str,
    *,
    api_key: str,
    body: dict | None = None,
    timeout_s: int = 120,
) -> dict:
    url = f"{CURSOR_API_BASE}{path}"
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Authorization": _auth_header(api_key),
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
        method=method,
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout_s) as resp:
            raw = resp.read().decode()
            return json.loads(raw) if raw.strip() else {}
    except urllib.error.HTTPError as e:
        detail = e.read().decode()[:500]
        raise RuntimeError(f"Cursor API HTTP {e.code} {path}: {detail}") from e


def cursor_auto_chat(
    prompt: str,
    *,
    model: str = "auto",
    mode: str = "plan",
    timeout_s: int = 300,
    poll_s: float = 2.0,
    delete_agent: bool = True,
    api_key: str | None = None,
) -> str:
    """One-shot prompt via POST /v1/agents → poll run → return run.result."""
    key = api_key or load_cursor_api_key()
    if not key:
        raise RuntimeError(
            "CURSOR_API_KEY required (export or set CURSOR_ENV_FILE=/opt/ambugo/serverbrain/.env)"
        )
    model_id = resolve_cursor_model(model)
    created = cursor_api_request(
        "POST",
        "/v1/agents",
        api_key=key,
        body={
            "prompt": {"text": prompt},
            "model": {"id": model_id},
            "mode": mode,
        },
        timeout_s=min(120, timeout_s),
    )
    agent = created.get("agent") or {}
    run = created.get("run") or {}
    agent_id = str(agent.get("id") or "")
    run_id = str(run.get("id") or agent.get("latestRunId") or "")
    if not agent_id or not run_id:
        raise RuntimeError(f"Cursor API create agent missing ids: {created!r}")

    deadline = time.monotonic() + timeout_s
    result_text = ""
    last_status = ""
    while time.monotonic() < deadline:
        run_doc = cursor_api_request(
            "GET",
            f"/v1/agents/{agent_id}/runs/{run_id}",
            api_key=key,
            timeout_s=60,
        )
        last_status = str(run_doc.get("status") or "").upper()
        if last_status in TERMINAL_RUN_STATUSES:
            result_text = str(run_doc.get("result") or "").strip()
            if last_status != "FINISHED":
                raise RuntimeError(
                    f"Cursor run {run_id} ended with {last_status}: {result_text[:200]}"
                )
            break
        time.sleep(poll_s)
    else:
        raise RuntimeError(f"Cursor run {run_id} timed out after {timeout_s}s (last={last_status})")

    if delete_agent:
        try:
            cursor_api_request("DELETE", f"/v1/agents/{agent_id}", api_key=key, timeout_s=60)
        except Exception:
            pass
    return result_text

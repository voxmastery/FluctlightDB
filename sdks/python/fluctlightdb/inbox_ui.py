"""Local web UI for handoff inbox and project brain status."""

from __future__ import annotations

import html
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Optional
from urllib.parse import parse_qs, urlparse

from .handoff_index import list_handoffs
from .project import find_project_root

_UI_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>FluctlightDB — {project_id}</title>
  <style>
    :root {{ font-family: system-ui, sans-serif; background: #0f1117; color: #e6edf3; }}
    body {{ max-width: 960px; margin: 0 auto; padding: 1.5rem; }}
    h1 {{ font-size: 1.4rem; margin-bottom: 0.25rem; }}
    .meta {{ color: #8b949e; font-size: 0.9rem; margin-bottom: 1.5rem; }}
    .card {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }}
    .badge {{ display: inline-block; padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.75rem; background: #238636; }}
    .badge.paused {{ background: #9e6a03; }}
    .badge.blocked {{ background: #da3633; }}
    pre {{ white-space: pre-wrap; word-break: break-word; font-size: 0.85rem; }}
    a {{ color: #58a6ff; }}
    .refresh {{ float: right; font-size: 0.85rem; }}
  </style>
</head>
<body>
  <h1>FluctlightDB project brain</h1>
  <p class="meta">{project_id} · {root} · <a class="refresh" href="/">Refresh</a></p>
  <h2>Handoff inbox</h2>
  {handoffs}
  <p class="meta">CLI: <code>fluctlight-project handoffs --json</code> · Sync: <code>fluctlight-project sync pull</code></p>
</body>
</html>
"""

_CARD = """
<div class="card">
  <span class="badge {status}">{status}</span>
  <strong>{agent}</strong> @ {subdir}
  <span class="meta"> · {created_at} · id {handoff_id}</span>
  <pre>{summary}</pre>
  {next_steps}
  {files}
</div>
"""


def _render_handoffs(items: list[Any]) -> str:
    if not items:
        return '<p class="meta">No handoffs yet. Agents write them via MCP, hooks, or <code>connect_project().handoff()</code>.</p>'
    parts = []
    for h in items:
        steps = ""
        if h.next_steps:
            steps = "<p><strong>Next:</strong> " + html.escape("; ".join(h.next_steps[:6])) + "</p>"
        files = ""
        if h.files:
            files = "<p><strong>Files:</strong> " + html.escape(", ".join(h.files[:12])) + "</p>"
        parts.append(
            _CARD.format(
                status=html.escape(h.status),
                agent=html.escape(h.agent),
                subdir=html.escape(h.subdir or "/"),
                created_at=html.escape(h.created_at),
                handoff_id=html.escape(h.handoff_id),
                summary=html.escape(h.summary),
                next_steps=steps,
                files=files,
            )
        )
    return "\n".join(parts)


def build_inbox_html(start: Optional[Path] = None, *, limit: int = 50) -> str:
    root = find_project_root(start)
    fluct = root / ".fluctlight"
    project_id = "project"
    cfg = fluct / "config.yaml"
    if cfg.is_file():
        import yaml

        data = yaml.safe_load(cfg.read_text(encoding="utf-8")) or {}
        project_id = str(data.get("project_id", project_id))
    items = list_handoffs(fluct, limit=limit)
    return _UI_HTML.format(
        project_id=html.escape(project_id),
        root=html.escape(str(root)),
        handoffs=_render_handoffs(items),
    )


class InboxHandler(BaseHTTPRequestHandler):
    start_path: Optional[Path] = None

    def log_message(self, format: str, *args: Any) -> None:
        if os.environ.get("FLUCTLIGHT_UI_QUIET", "").lower() not in ("1", "true"):
            super().log_message(format, *args)

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/api/handoffs":
            root = find_project_root(self.start_path)
            items = list_handoffs(root / ".fluctlight", limit=100)
            payload = [
                {
                    "handoff_id": h.handoff_id,
                    "agent": h.agent,
                    "subdir": h.subdir,
                    "status": h.status,
                    "summary": h.summary,
                    "next_steps": h.next_steps,
                    "files": h.files,
                    "created_at": h.created_at,
                }
                for h in items
            ]
            body = json.dumps(payload, indent=2).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if parsed.path in ("/", "/index.html"):
            body = build_inbox_html(self.start_path).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404)

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path != "/api/sync/pull":
            self.send_error(404)
            return
        from .sync import sync_pull

        result = sync_pull(self.start_path)
        body = json.dumps({"ok": result.ok, "message": result.message}).encode("utf-8")
        self.send_response(200 if result.ok else 500)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body)


def serve_inbox(
    *,
    start: Optional[os.PathLike[str] | str] = None,
    host: str = "127.0.0.1",
    port: int = 8787,
) -> None:
    start_path = Path(start).resolve() if start else None
    find_project_root(start_path)  # fail fast
    InboxHandler.start_path = start_path
    server = ThreadingHTTPServer((host, port), InboxHandler)
    url = f"http://{host}:{port}/"
    print(f"FluctlightDB inbox UI at {url}")
    print("  API: GET /api/handoffs · POST /api/sync/pull")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopped.")

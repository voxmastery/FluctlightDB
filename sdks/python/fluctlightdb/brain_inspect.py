"""Memory observability UI — engram browser + recall audit."""

from __future__ import annotations

import html
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Optional
from urllib.parse import parse_qs, urlparse

_INSPECT_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>FluctlightDB — Brain Inspect</title>
  <style>
    :root {{ font-family: system-ui, sans-serif; background: #0d1117; color: #e6edf3; }}
    body {{ max-width: 1100px; margin: 0 auto; padding: 1.25rem; }}
    h1 {{ font-size: 1.35rem; }}
    .meta {{ color: #8b949e; font-size: 0.9rem; margin-bottom: 1rem; }}
    .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }}
    @media (max-width: 800px) {{ .grid {{ grid-template-columns: 1fr; }} }}
    .card {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }}
    .badge {{ display: inline-block; padding: 0.12rem 0.45rem; border-radius: 4px; font-size: 0.72rem; background: #238636; }}
    .badge.warn {{ background: #9e6a03; }}
    .badge.lane {{ background: #1f6feb; }}
    pre {{ white-space: pre-wrap; word-break: break-word; font-size: 0.82rem; margin: 0.4rem 0 0; }}
    input[type=search] {{ width: 100%; padding: 0.5rem; border-radius: 6px; border: 1px solid #30363d; background: #0d1117; color: inherit; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.85rem; }}
    th, td {{ text-align: left; padding: 0.35rem 0.5rem; border-bottom: 1px solid #21262d; }}
    a {{ color: #58a6ff; }}
  </style>
</head>
<body>
  <h1>FluctlightDB brain inspect</h1>
  <p class="meta">{path} · engrams {engram_count} · WM {wm_len} · CHORUS {chorus_len}</p>
  <form method="get" action="/">
    <input type="search" name="cue" placeholder="Test recall cue…" value="{cue}"/>
  </form>
  {recall_block}
  <div class="grid">
    <div>
      <h2>Engrams</h2>
      {engrams}
    </div>
    <div>
      <h2>Working memory</h2>
      {wm}
      <h2>Audit log</h2>
      {audit}
    </div>
  </div>
</body>
</html>
"""

_RECALL = """
<div class="card">
  <h3>Recall: <code>{cue}</code> <span class="badge lane">{mode}</span></h3>
  <p class="meta">lanes: {lanes}</p>
  {hits}
</div>
"""

_HIT = """
<div class="card">
  <span class="badge lane">{lane}</span> score {score:.3f}
  {verified}
  <pre>{content}</pre>
</div>
"""

_ENGRAM = """
<tr>
  <td><code>{id}</code></td>
  <td>{salience:.2f}</td>
  <td>{verified}</td>
  <td>{preview}</td>
</tr>
"""


def _connect_brain(path: Optional[str] = None) -> Any:
    from .brain import connect_agent

    if path:
        return connect_agent(path, retain_days=None)
    return connect_agent(retain_days=None)


def inspect_payload(brain: Any, *, cue: str = "", limit: int = 40) -> dict[str, Any]:
    st = brain.status() if hasattr(brain, "status") else {}
    query = getattr(brain._brain, "query_json", None)
    engrams: list[dict[str, Any]] = []
    if query:
        raw = query(json.dumps({"op": "list_engrams", "page_size": limit}))
        engrams = list(raw.get("engrams", []))
    recall = brain.recall(cue, mode="auto", limit=8) if cue else {"hits": [], "mode": "auto", "lanes_used": []}
    audit_fn = getattr(brain._brain, "audit_log_json", None)
    audit: list[dict[str, Any]] = audit_fn(20) if audit_fn else []
    wm_len = brain.wm_len() if hasattr(brain, "wm_len") else 0
    chorus_len = brain.chorus_len() if hasattr(brain, "chorus_len") else 0
    return {
        "status": st,
        "engrams": engrams,
        "recall": recall,
        "audit": audit,
        "wm_len": wm_len,
        "chorus_len": chorus_len,
    }


def render_inspect_html(brain: Any, *, path: str = "", cue: str = "") -> str:
    data = inspect_payload(brain, cue=cue)
    recall = data["recall"]
    hits_html = ""
    if cue:
        parts = []
        for h in recall.get("hits", []):
            parts.append(
                _HIT.format(
                    lane=html.escape(str(h.get("lane", "?"))),
                    score=float(h.get("score", 0)),
                    verified='<span class="badge">verified</span>' if h.get("verified") else '<span class="badge warn">unverified</span>',
                    content=html.escape((h.get("content") or h.get("snippet") or "")[:1200]),
                )
            )
        hits_html = "\n".join(parts) if parts else '<p class="meta">No hits.</p>'
    recall_block = (
        _RECALL.format(
            cue=html.escape(cue),
            mode=html.escape(str(recall.get("mode", "auto"))),
            lanes=html.escape(", ".join(recall.get("lanes_used", []))),
            hits=hits_html,
        )
        if cue
        else ""
    )
    rows = []
    for e in data["engrams"]:
        rows.append(
            _ENGRAM.format(
                id=html.escape(str(e.get("engram_id", ""))[:12]),
                salience=float(e.get("salience", 0)),
                verified="✓" if e.get("verified") else "—",
                preview=html.escape(str(e.get("content", ""))[:80]),
            )
        )
    engrams_html = (
        "<table><tr><th>ID</th><th>Sal</th><th>V</th><th>Preview</th></tr>"
        + "".join(rows)
        + "</table>"
        if rows
        else '<p class="meta">No engrams.</p>'
    )
    wm_html = f'<p class="meta">{data["wm_len"]} slots in WM-Ring</p>'
    audit_lines = [
        f'<div class="meta">{html.escape(a.get("action", ""))}: {html.escape(a.get("detail", ""))}</div>'
        for a in data["audit"][-12:]
    ]
    return _INSPECT_HTML.format(
        path=html.escape(path or "(ephemeral)"),
        engram_count=len(data["engrams"]),
        wm_len=data["wm_len"],
        chorus_len=data["chorus_len"],
        cue=html.escape(cue),
        recall_block=recall_block,
        engrams=engrams_html,
        wm=wm_html,
        audit="\n".join(audit_lines) if audit_lines else '<p class="meta">No governance events yet.</p>',
    )


def run_inspect_ui(
    brain_path: Optional[str] = None,
    *,
    host: str = "127.0.0.1",
    port: int = 8788,
) -> None:
    brain = _connect_brain(brain_path)
    path_label = brain_path or ""

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            parsed = urlparse(self.path)
            qs = parse_qs(parsed.query)
            cue = (qs.get("cue") or [""])[0]
            body = render_inspect_html(brain, path=path_label, cue=cue).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, fmt: str, *args: Any) -> None:
            return

    server = ThreadingHTTPServer((host, port), Handler)
    print(f"Brain inspect UI: http://{host}:{port}/")
    server.serve_forever()

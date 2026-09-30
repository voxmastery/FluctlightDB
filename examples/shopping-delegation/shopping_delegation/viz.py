"""One static HTML file. No build step. The page is a picture of a demo session."""

from __future__ import annotations

import html
import json
from pathlib import Path
from typing import Any


def write_session_page(actions: list[dict[str, Any]], path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render(actions), encoding="utf-8")
    return path


def render(actions: list[dict[str, Any]]) -> str:
    cards = "\n".join(_card(action) for action in actions)
    blob = json.dumps(actions, indent=2)
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Shopping delegation trace (synthetic)</title>
<style>
  :root {{
    color-scheme: light;
    --ink: #1c1915;
    --muted: #5c564c;
    --line: #e4ddd0;
    --paper: #faf7f2;
    --buy: #1f6b45;
    --buy-bg: #e5f4eb;
    --stop: #8a341f;
    --stop-bg: #fde8e1;
  }}
  body {{
    margin: 0;
    font: 16px/1.45 "Iowan Old Style", "Palatino Linotype", Palatino, Georgia, serif;
    color: var(--ink);
    background: var(--paper);
  }}
  main {{ max-width: 920px; margin: 0 auto; padding: 32px 20px 64px; }}
  h1 {{ font-size: 1.8rem; font-weight: 600; margin-bottom: 0.2rem; }}
  .banner {{
    display: inline-block;
    letter-spacing: 0.08em;
    font: 12px/1 ui-sans-serif, system-ui, sans-serif;
    border: 1px solid var(--ink);
    padding: 2px 8px;
    margin-bottom: 12px;
  }}
  p.lead {{ color: var(--muted); max-width: 62ch; }}
  article {{
    border: 1px solid var(--line);
    background: white;
    margin: 16px 0;
    padding: 14px 16px 8px;
  }}
  .tag {{
    font: 12px/1 ui-sans-serif, system-ui, sans-serif;
    padding: 2px 8px;
    border-radius: 999px;
  }}
  .buy {{ background: var(--buy-bg); color: var(--buy); }}
  .refuse {{ background: var(--stop-bg); color: var(--stop); }}
  ol {{
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    padding: 0;
    list-style: none;
  }}
  li {{
    border-top: 3px solid var(--ink);
    background: #fffdf8;
    padding: 8px;
    min-height: 92px;
  }}
  li span {{ display: block; font: 11px/1.2 ui-sans-serif, system-ui, sans-serif; color: var(--muted); }}
  code {{ font-size: 0.86rem; }}
  details {{ margin-top: 28px; }}
  pre {{ overflow: auto; background: #fff; border: 1px solid var(--line); padding: 12px; font-size: 12px; }}
  @media (max-width: 720px) {{
    ol {{ grid-template-columns: 1fr; }}
  }}
</style>
</head>
<body>
<main>
  <div class="banner">SYNTHETIC</div>
  <h1>Standing delegation trace</h1>
  <p class="lead">
    Fictional grocery agent. Each card is one purchase attempt. The chain is the
    action, the rule or revocation the memory actually returned, and the consumer
    consent that rule points at. FluctlightDB stores that link as provenance on the
    episode (<code>source_uri</code>, <code>verified</code>, kind). It does not
    invent a separate consent graph.
  </p>
  {cards}
  <details>
    <summary>Raw session JSON</summary>
    <pre>{html.escape(blob)}</pre>
  </details>
</main>
</body>
</html>
"""


def _card(action: dict[str, Any]) -> str:
    decision = action["decision"]
    attempt = action["attempt"]
    cited = action.get("cited_hit") or {}
    kind = "buy" if decision["action"] == "buy" else "refuse"
    consent = decision.get("authorising_consent_id") or "none — not authorised"
    middle_title = "Revocation" if decision["reason"] == "revoked" else "Rule"
    middle = decision.get("cited_record_id") or "not in the recalled set"
    engine = (
        f"engram {cited.get('engram_id') or 'n/a'} · "
        f"verified={cited.get('verified')} · "
        f"{cited.get('provenance_kind') or 'no kind'} · "
        f"{cited.get('source_uri') or 'no source_uri'}"
    )
    return f"""
<article>
  <p><span class="tag {kind}">{html.escape(decision['action'])}</span>
     {html.escape(action['action_id'])}
     · {html.escape(str(decision['reason']))}
     · {html.escape(str(attempt['sku']))}
     · {int(attempt['price_cents'])}¢</p>
  <ol>
    <li><span>Action</span><code>{html.escape(decision['action'])} {html.escape(str(attempt['brand']))}</code></li>
    <li><span>{html.escape(middle_title)}</span><code>{html.escape(str(middle))}</code></li>
    <li><span>Consent</span><code>{html.escape(str(consent))}</code></li>
  </ol>
  <p><span>{html.escape(engine)}</span></p>
</article>
"""

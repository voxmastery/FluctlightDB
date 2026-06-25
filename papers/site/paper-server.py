#!/usr/bin/env python3
"""
FluctlightDB paper draft viewer for search.ambugo.help/paper/

  GET /              — HTML viewer (draft, guide, downloads)
  GET /files/*       — tex, bib, md downloads
  GET /data/*        — frozen benchmark JSON
"""
from __future__ import annotations

import http.server
import os
import socketserver
from pathlib import Path

PORT = int(os.environ.get("PAPER_VIEWER_PORT", "3104"))
HERE = Path(__file__).resolve().parent

_MIME = {
    ".html": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".md": "text/markdown; charset=utf-8",
    ".tex": "text/plain; charset=utf-8",
    ".bib": "text/plain; charset=utf-8",
    ".pdf": "application/pdf",
}


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(HERE), **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()

    def guess_type(self, path):
        ext = os.path.splitext(path)[1].lower()
        return _MIME.get(ext, super().guess_type(path))


def main():
    os.chdir(HERE)
    with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
        print(f"Paper viewer on http://127.0.0.1:{PORT}/", flush=True)
        httpd.serve_forever()


if __name__ == "__main__":
    main()

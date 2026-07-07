#!/usr/bin/env bash
# Sync LaTeX + markdown + metrics into public paper site (GitHub Pages + HF Space).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIGURES="$ROOT/papers/figures"
PUBLIC="$ROOT/papers/public"
ARXIV="$ROOT/papers/arxiv-v1"

mkdir -p "$PUBLIC"/{assets,files,data}

cp "$ARXIV/main.tex" "$PUBLIC/files/main.tex"
cp "$ARXIV/references.bib" "$PUBLIC/files/references.bib"
if [[ -f "$ARXIV/main.pdf" ]]; then
  cp "$ARXIV/main.pdf" "$PUBLIC/files/main.pdf"
fi
for f in 01-brain-architecture 02-benchmark-summary 03-longmemeval-by-type 04-longmemeval-e2e-by-type; do
  cp "$FIGURES/${f}.png" "$PUBLIC/assets/${f}.png" 2>/dev/null || true
  cp "$FIGURES/${f}.pdf" "$PUBLIC/assets/${f}.pdf" 2>/dev/null || true
done
# legacy alias for draft markdown
cp "$FIGURES/01-brain-architecture.png" "$PUBLIC/assets/brain-architecture.png" 2>/dev/null || true
cp "$ROOT/benchmarks/results/paper-2026-07-07.json" "$PUBLIC/data/results.json"
cp "$ROOT/benchmarks/results/paper-2026-07-07.json" "$ROOT/hub/dataset/results.json" 2>/dev/null || true
cp "$ROOT/benchmarks/results/e2e-cert-paper-v2-2026-07-07.json" "$PUBLIC/data/e2e-cert-paper-v2-2026-07-07.json" 2>/dev/null || true
cp "$ROOT/benchmarks/results/longmemeval-preference-v4-mpnet-2026-07-04.json" "$PUBLIC/data/longmemeval-preference-v4-mpnet-2026-07-04.json" 2>/dev/null || true
cp "$ROOT/benchmarks/results/longmemeval-colab-v2-full-2026-07-04.json" "$PUBLIC/data/longmemeval-colab-v2-full-2026-07-04.json" 2>/dev/null || true

# index.html — public preprint (no nginx /paper base path)
cat > "$PUBLIC/index.html" << 'HTML'
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>FluctlightDB — Research Paper (Preprint)</title>
  <meta name="description" content="FluctlightDB: a brain-native memory engine for AI agents. 98.1% LoCoMo evidence recall, 97.6% LongMemEval-S session recall@8, 97.4% LongMemEval E2E QA." />
  <meta property="og:title" content="FluctlightDB: A Memory Model of Data for AI Agents" />
  <meta property="og:description" content="Third data model for agent memory — 98.1% LoCoMo, 97.6% LongMemEval-S retrieval, 97.4% E2E QA, BEIR SciFact parity, FAMB 97–98%." />
  <meta property="og:type" content="article" />
  <link rel="stylesheet" href="assets/style.css" />
</head>
<body>
  <header class="topbar">
    <div>
      <h1>FluctlightDB · Research Paper</h1>
      <div class="sub">Preprint · arXiv cs.DB (pending) · July 2026</div>
    </div>
    <nav class="nav" id="nav">
      <a href="#draft" data-page="draft" class="active">Draft</a>
      <a href="#guide" data-page="guide">Guide</a>
      <a href="#downloads" data-page="downloads">Downloads</a>
      <a href="https://github.com/voxmastery/FluctlightDB" target="_blank" rel="noopener">GitHub</a>
      <a href="https://pypi.org/project/fluctlightdb/" target="_blank" rel="noopener">PyPI</a>
    </nav>
  </header>

  <div class="layout">
    <aside class="panel">
      <h2>Frozen metrics</h2>
      <ul id="metrics-sidebar">
        <li>LoCoMo evidence recall <span class="metric">98.1%</span></li>
        <li>LongMemEval-S session@8 <span class="metric">97.6%</span></li>
        <li>LongMemEval-S E2E QA <span class="metric">97.4%</span></li>
        <li>LongMemEval preference@8 <span class="metric">96.7%</span></li>
        <li>BEIR SciFact nDCG@10 <span class="metric">0.645</span></li>
        <li>FAMB macro (index) <span class="metric">98%</span></li>
      </ul>
      <h2 style="margin-top:18px">Cite</h2>
      <p style="font-size:0.75rem;color:#8ba3c7;margin-top:6px">
        See <a href="https://github.com/voxmastery/FluctlightDB/blob/main/CITATION.cff">CITATION.cff</a>
        · ORCID <a href="https://orcid.org/0009-0006-7758-4114">0009-0006-7758-4114</a>
      </p>
      <h2 style="margin-top:18px">Status</h2>
      <ul>
        <li><span class="tag">RETRIEVAL</span> LoCoMo + LongMemEval complete</li>
        <li><span class="tag">E2E</span> LongMemEval 97.4% certified</li>
        <li><span class="tag">PREPRINT</span> Public draft</li>
        <li><span class="tag">ARXIV</span> Submission pending</li>
      </ul>
    </aside>

    <main class="content">
      <section id="page-draft" class="page panel active">
        <div class="paper-body" id="draft-root"></div>
      </section>
      <section id="page-guide" class="page panel">
        <div class="paper-body" id="guide-root"></div>
      </section>
      <section id="page-downloads" class="page panel">
        <h2 class="paper-title" style="font-size:1.2rem">Downloads</h2>
        <p class="paper-meta">Source files · build PDF with pdfLaTeX in papers/arxiv-v1/</p>
        <div class="dl-grid">
          <div class="dl-card">
            <h3>main.pdf</h3>
            <p>Built preprint PDF (arxiv-v1)</p>
            <a href="files/main.pdf" download>Download PDF</a>
          </div>
          <div class="dl-card">
            <h3>main.tex</h3>
            <p>Full LaTeX manuscript (arxiv-v1)</p>
            <a href="files/main.tex" download>Download .tex</a>
          </div>
          <div class="dl-card">
            <h3>references.bib</h3>
            <p>Bibliography</p>
            <a href="files/references.bib" download>Download .bib</a>
          </div>
          <div class="dl-card">
            <h3>results.json</h3>
            <p>Frozen benchmark metrics (paper-2026-07-07)</p>
            <a href="data/results.json" download>Download JSON</a>
          </div>
          <div class="dl-card">
            <h3>Figures (PNG)</h3>
            <p>Architecture + benchmark diagrams</p>
            <a href="assets/01-brain-architecture.png" download>Fig 1 architecture</a><br/>
            <a href="assets/02-benchmark-summary.png" download>Fig 2 benchmarks</a><br/>
            <a href="assets/03-longmemeval-by-type.png" download>Fig 3 retrieval by type</a><br/>
            <a href="assets/04-longmemeval-e2e-by-type.png" download>Fig 4 E2E by type</a><br/>
            <a href="https://github.com/voxmastery/FluctlightDB/tree/main/papers/figures" target="_blank" rel="noopener">All figures on GitHub →</a>
          </div>
          <div class="dl-card">
            <h3>Repository</h3>
            <p>Engine, harnesses, reproduce scripts</p>
            <a href="https://github.com/voxmastery/FluctlightDB" target="_blank" rel="noopener">GitHub →</a>
          </div>
        </div>
      </section>
    </main>
  </div>
  <script src="assets/app.js"></script>
</body>
</html>
HTML

echo "Synced public paper site → $PUBLIC"

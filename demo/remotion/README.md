# Fluctlight Swarm Memory — Remotion demo

This composition recreates a continuous Codex terminal session using the real behavioral results from `scripts/demo_codex_swarm.py`. It is deliberately a terminal-first video, not a slide deck.

```bash
cd demo/remotion
npm install
npm test
npm run typecheck
npm run render
npm run still
```

The render overwrites the public demo assets referenced by the repository README:

- `docs/demo/fluctlight-swarm-memory-demo.mp4`
- `docs/demo/fluctlight-swarm-memory-preview.png`

The terminal interaction is a scripted visualization. The four PASS results are produced by the runnable repository demo and are verified separately before rendering.

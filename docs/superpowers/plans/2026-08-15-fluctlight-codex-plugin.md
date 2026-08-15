# Fluctlight stock-Codex plugin implementation plan

> Execute after the swarm HTTP contract is stable. This plan owns the MIT plugin, MCP adapter, lifecycle hooks, Skill, and their tests. It does not modify Codex core.

**Goal:** Install Fluctlight Swarm Memory into stock open-source Codex using its existing plugin manifest, MCP-backed `SubagentStart`/`SubagentStop` hooks, and root orchestration Skill.

**Architecture:** The root Skill declares a complete roster before any spawn. The start hook claims one precomputed slot and returns bounded `additionalContext`. The stop hook records a pending attempt. All calls go to the single Rust coordinator; the Python MCP process never opens an embedded brain.

**Tech stack:** Python 3.9+, FastMCP, pytest, Codex plugin manifest/hooks, SKILL.md.

---

## Task 1: Add a typed remote swarm client

**Files:**

- Create: `sdks/python/fluctlightdb/swarm_client.py`
- Create: `sdks/python/tests/test_swarm_client.py`
- Modify: `sdks/python/fluctlightdb/__init__.py`

1. Write failing tests with an in-process fake HTTP server for every coordinator endpoint, non-2xx errors, timeouts, and idempotency headers.
2. Implement dataclasses for roster slots, bundles, citations, attempts, evidence, and summaries.
3. Implement `SwarmClient` using the Python standard library so the base package gains no new HTTP dependency.
4. Require explicit coordinator URL and worker/verifier token; never fall back to direct embedded access.
5. Run `pytest sdks/python/tests/test_swarm_client.py -q` and commit: `feat(python): add remote swarm coordinator client`.

## Task 2: Add the dedicated swarm MCP server

**Files:**

- Create: `sdks/python/fluctlightdb/swarm_mcp.py`
- Create: `sdks/python/tests/test_swarm_mcp.py`
- Modify: `sdks/python/pyproject.toml`

1. Write failing tool-contract tests using a fake `SwarmClient`.
2. Expose only:

```text
fluctlight_swarm_begin
fluctlight_swarm_claim
fluctlight_swarm_cite
fluctlight_swarm_report_attempt
fluctlight_swarm_finish
fluctlight_swarm_get
```

3. Keep evidence submission out of the worker MCP surface.
4. Return structured JSON objects, stable error codes, and bounded context text.
5. Add the `fluctlight-swarm-mcp` console entry point and commit: `feat(mcp): add capability-safe swarm tools`.

## Task 3: Scaffold and validate the Codex plugin

**Files:**

- Create via the official plugin scaffold: `plugins/fluctlight-swarm/.codex-plugin/plugin.json`
- Create: `plugins/fluctlight-swarm/.mcp.json`
- Create: `plugins/fluctlight-swarm/hooks/hooks.json`
- Create: `plugins/fluctlight-swarm/README.md`
- Test: `sdks/python/tests/test_codex_plugin.py`

1. Use the local `plugin-creator` scaffold script, then replace generated examples with this plugin's files.
2. Add failing schema tests against the current Codex plugin manifest and hook declarations audited at Codex commit `85fc4def358b7df21883e72ae8dda43a0f572f32`.
3. Configure MCP-backed hooks:

```text
SubagentStart -> fluctlight_swarm_claim
SubagentStop  -> fluctlight_swarm_report_attempt
```

4. Pass lifecycle identity fields from the hook event templates; do not accept model-provided substitutes.
5. Set strict per-hook timeouts and context limits. A missing swarm or slot must fail closed with actionable context.
6. Run the plugin validator from `plugin-creator` and commit: `feat(codex): package stock swarm-memory plugin`.

## Task 4: Create the orchestration Skill

**Files:**

- Create via the skill scaffold: `plugins/fluctlight-swarm/skills/fluctlight-swarm/SKILL.md`
- Test: `sdks/python/tests/test_codex_plugin.py`

1. Use `skill-creator`'s `init_skill.py` so frontmatter and layout are valid.
2. Write tests that assert the Skill's mandatory ordering and tool names.
3. The Skill must require:
   - determine the complete roster;
   - call `fluctlight_swarm_begin` once;
   - stop if begin fails;
   - only then spawn agents;
   - require memory citations in worker completion;
   - finish the swarm after trusted verification.
4. Keep instructions concise and reference the plugin README for operational detail.
5. Run skill validation and commit: `feat(codex): add swarm orchestration skill`.

## Task 5: End-to-end lifecycle tests on stock Codex contracts

**Files:**

- Create: `sdks/python/tests/test_codex_swarm_lifecycle.py`
- Create: `tests/fixtures/codex-hooks/subagent-start.json`
- Create: `tests/fixtures/codex-hooks/subagent-stop.json`

1. Start one temporary coordinator and MCP adapter.
2. Begin a two-slot roster, replay two real hook event fixtures, and assert each start gets a different episodic memory ID but identical truth/warnings.
3. Replay stop events and verify attempts are bound to the hook-supplied agent/worktree.
4. Exercise duplicate hook delivery, restart between begin/claim, missing roster, excess worker, and context-size limits.
5. Run the complete Python suite and commit: `test(codex): verify stock plugin swarm lifecycle`.

## Task 6: Plugin verification gate

1. Run `python -m pytest sdks/python/tests -q`.
2. Run plugin and Skill validators.
3. Install the plugin in a temporary Codex home and confirm Codex discovers its Skill, MCP server, and hooks.
4. Execute two real Codex subagents against a disposable fixture repository.
5. Confirm plugin removal returns Codex to unchanged behavior.
6. Document installation, server startup, expected fail-closed errors, and uninstall in `plugins/fluctlight-swarm/README.md`.


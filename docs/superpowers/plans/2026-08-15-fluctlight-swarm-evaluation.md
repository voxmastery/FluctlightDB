# Fluctlight swarm evaluation and hackathon demo plan

> Execute after the engine and stock-Codex plugin pass their verification gates. This plan owns reproducible proof, demo UX, and the upstream issue package.

**Goal:** Demonstrate that Fluctlight Swarm Memory guarantees memory-ID disjointness and durable attribution, while measuring whether it reduces duplicated approaches and repeated failures without lowering verified task success.

**Architecture:** A frozen task corpus runs with the same Codex version, model, prompts, concurrency, and budgets in two modes: baseline shared recall and Fluctlight global allocation. JSONL run records produce an auditable report and one live two-worker demo.

**Tech stack:** Python, pytest, JSONL, Markdown/JSON reports, Cargo/Python test commands, Git worktrees.

---

## Task 1: Build a deterministic evaluation harness

**Files:**

- Create: `benchmarks/swarm/run_eval.py`
- Create: `benchmarks/swarm/metrics.py`
- Create: `benchmarks/swarm/tasks.json`
- Create: `benchmarks/swarm/tests/test_metrics.py`
- Create: `benchmarks/swarm/README.md`

1. Write failing tests for allocation overlap, semantic overlap, command/file-path overlap, repeated-failure rate, verified success, and restart recovery.
2. Implement pure metric functions first, then the runner.
3. Record the full configuration and raw per-worker exposures/citations/evidence in JSONL.
4. Refuse comparisons if Codex commit, model, budget, task revision, or concurrency differs.
5. Commit: `bench(swarm): add reproducible comparison harness`.

## Task 2: Create frozen tasks with known duplicate-attempt traps

**Files:**

- Create: `benchmarks/swarm/fixtures/`
- Modify: `benchmarks/swarm/tasks.json`
- Test: `benchmarks/swarm/tests/test_fixtures.py`

1. Add at least six small repositories/tasks covering bug diagnosis, API change, concurrency, migration, test repair, and refactor.
2. Give each task multiple viable approaches and seed at least one verified reproduced failure.
3. Store expected verification commands and immutable fixture hashes.
4. Test that all fixtures start failing and their gold fixes pass.
5. Commit: `bench(swarm): add frozen parallel-agent task corpus`.

## Task 3: Run baseline and swarm trials

**Files:**

- Create generated outputs under: `benchmarks/swarm/results/`
- Create: `docs/SWARM_EVALUATION.md`

1. Run at least three seeds per task per mode with two to four workers.
2. Restart the coordinator during a subset of swarm trials.
3. Report medians and raw counts; do not claim statistical significance from the hackathon-sized sample.
4. Clearly separate guaranteed properties from observed outcomes.
5. Commit the configuration, raw results, and report: `bench(swarm): publish baseline comparison`.

## Task 4: Build the live vertical-slice demo

**Files:**

- Create: `examples/codex-swarm-demo/README.md`
- Create: `examples/codex-swarm-demo/seed.py`
- Create: `examples/codex-swarm-demo/verify.py`
- Create: `examples/codex-swarm-demo/run.sh`

1. Seed shared truth, two useful strategy episodes, and one reproduced-failure warning.
2. Launch one coordinator and two real Codex worktree agents through the stock plugin.
3. Display the roster, bundles, citations, evidence, feedback, and restart proof without manual database editing.
4. Keep the complete demo under five minutes and make every step repeatable from a clean checkout.
5. Commit: `demo: add Codex swarm-memory vertical slice`.

## Task 5: Prepare the Codex enhancement proposal

**Files:**

- Create: `docs/CODEX_BATCH_ROSTER_PROPOSAL.md`

1. Document the remaining stock-Codex limitation: roster ordering is protocol-enforced, not scheduler-enforced.
2. Include the source seam, minimal generic batch-roster API, failure modes, compatibility story, and benchmark evidence.
3. State that FluctlightDB remains an external optional provider; the Codex change must not hard-code it.
4. Do not open an unsolicited PR. Open an evidence-backed issue first because current Codex contributions are invitation-only.
5. If maintainers invite a patch, implement it in the Apache-2.0 Codex fork with its existing test and formatting conventions.

## Task 6: Final verification gate

1. Run Rust format, clippy, and full tests.
2. Run the complete Python suite and plugin/Skill validators.
3. Run the demo twice from clean temporary stores, including one restart.
4. Audit logs for tokens, repository secrets, transcript leakage, and absolute local paths.
5. Confirm all committed generated results contain version/config metadata.
6. Tag the hackathon release only after every success gate in the design spec is evidenced.


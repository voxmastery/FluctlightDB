# Shopping agent with durable delegation memory

SYNTHETIC demo. A rule-based grocery agent keeps standing delegations
(item, brand allowlist, price cap, cadence, consent id) in an embedded
FluctlightDB brain. Every purchase attempt is explained as
action, then the rule or revocation, then the consumer consent that
authorised it. No language-model calls. No real orders. The shoppers are invented.

FluctlightDB is beta. This is one embedded process. The numbers below are
from the synthetic set on the machine in [RESULTS.md](RESULTS.md). They do
not generalise to real shoppers.

## What the engine actually does

The public API used here is `connect_embedded`, `experience`, `activate`,
`checkpoint`, and (in the demo printout only) `resolve`.

A delegation is an episode. Provenance on that episode is the real fields
`verified`, `provenance.kind`, `provenance.source_uri`, and
`provenance.confidence`. Consumer consent is `user_explicit` and
`source_uri=consent://…`. A revocation is `ledger_verified`. A past receipt
is `tool_grounded`. Contradictory chat is `chat_assertion` and not verified.

`activate` returns those fields on the episode. The shopping policy then
picks the latest delegation for the item, unless a later revocation is also
in the recalled set. The trace reads `source_uri` back off that hit.

What this repo does not provide, and this demo does not pretend it does:

- There is no delegation type, no consent object, and no engine edge of the
  form action to rule to consent. The chain is application code over provenance
  fields plus the record text.
- `resolve()` returns one provenance-weighted winner (kind, verified flag,
  confidence, salience, activation). A purchase decision needs the live rule,
  a possible revocation, and receipt history at once, so the measured path is
  `activate(limit=8)`, not `resolve()`.
- On cues that look exact (`what is the exact …`, `id:`), the engine runs
  `detect_exact_query` and injects at most three verified engrams ahead of
  associative hits. Older tied verified rows win that injection. A fourth
  verified row (often a revocation) can fall out of the top three and only
  survive if ordinary activation still ranks it inside the limit. That shows
  up in the cohort tables when it happens.
- Verified writes skip the separation gate. Near-duplicate unverified chat
  can be refused. Baselines keep every line. Ingest counts are in the results.

## One-command repro

From this directory, with the in-repo SDK and the native extension installed
(see Requirements):

```bash
./run.sh
```

That runs the policy self-test, the scripted demo, and the benchmark.
Equivalents:

```bash
make selftest   # no native extension required
make demo
make bench
make check      # self-test, regenerate, byte-compare data/sessions.jsonl
```

`make bench` reads the checked-in JSONL. It rewrites `RESULTS.md` and
`results/benchmark.json`.

## Requirements

- Python 3.9+ (developed on 3.12)
- The package in `sdks/python` (`pip install -e sdks/python` from the repo root)
- `fluctlightdb_native`, built from this repo:

```bash
pip install maturin
maturin build --release -o /tmp/fluctlight-wheels --manifest-path crates/fluctlight-py/Cargo.toml
pip install /tmp/fluctlight-wheels/fluctlightdb_native-*.whl
```

No extra scientific packages. The TF-IDF baseline is local code. No API keys.

## CLI

Scripted session (mock catalog, four decisions, restart, static page):

```bash
python -m shopping_delegation demo
```

Manual loop, using `var/demo-brain` unless `--brain` is set:

```bash
python -m shopping_delegation record \
  --item whole_milk --brands meadow_dairy,north_farm \
  --max-price-cents 399 --cadence-days 3 --consent C-milk-1
python -m shopping_delegation buy \
  --item whole_milk --brand meadow_dairy --sku WM-MD-1L --price-cents 349
python -m shopping_delegation revoke \
  --item whole_milk --consent C-milk-1 --rule-id rule-C-milk-1
python -m shopping_delegation explain last
```

`web/session.html` is a single file the demo rewrites. Open it in a browser.
It is labeled SYNTHETIC.

## Benchmark

`data/sessions.jsonl` is SYNTHETIC (seed 20260330, 36 sessions). Also labeled
in `data/SYNTHETIC.md` and on every JSON row. Each session has a focus item,
distractor delegations, cue-heavy chat that contradicts the cap, and purchase
attempts with ground truth from the same policy the agent runs.

Cohorts:

| cohort | what is in memory before the late attempts |
|---|---|
| A_single | one delegation |
| A_cadence | one delegation plus a recent receipt |
| B_update | a second consent with a tighter cap |
| C_stack_revoke | three consents, then a revocation |
| D_revoke | one consent, then a revocation |

Same attempts, same policy, same top-8. Only the memory changes.

| memory | what it ranks on |
|---|---|
| fluctlight | `Brain.activate` after `experience` with provenance |
| chat_log | fraction of cue tokens in the line; later line wins a tie |
| tfidf | TF-IDF cosine, standard library only; earlier line wins a tie |

Metrics (definitions also sit at the top of `RESULTS.md`):

- **Rule recall** — the authoritative row (latest delegation, or the revocation once the rule is dead) is inside the top 8.
- **Policy compliance** — the agent did not buy when price, brand, consent, revocation, or cadence forbids it. Refusals count as compliant.
- **Decision accuracy** — buy/refuse matches ground truth, including legal buys the agent missed.
- **Revocation handling** — on attempts whose truth is `revoked`, the agent did not buy.
- **Revocation detected** — the agent refused because it recalled the revocation, not for some other reason.
- **Trace correctness** — cited authorising consent and revocation id match ground truth.
- **Latency** — milliseconds inside `recall`, p50 and p95, on this machine.

## Results

One run on this machine (Python 3.12.3, Linux 6.12.94+, Intel Xeon, 4 CPUs).
SYNTHETIC, 36 sessions, 150 attempts, recall limit 8. `make bench` rewrites
the table. Quality counts were stable across repeats. Latencies move by
hundredths of a millisecond.

| memory | rule recall | policy compliance | decision accuracy | revocation handling | revocation detected | trace | p50 ms | p95 ms | cold p50 | cold p95 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| fluctlight | 100% | 100% | 100% | 100% | 100% | 100% | 0.068 | 0.499 | 0.467 | 0.517 |
| chat_log | 60% | 100% | 88% | 100% | 50% | 56% | 0.180 | 0.214 | 0.184 | 0.235 |
| tfidf | 100% | 100% | 100% | 100% | 100% | 100% | 0.147 | 0.174 | 0.157 | 0.183 |

Reading that table:

- FluctlightDB **ties TF-IDF** on every quality metric here. It does not beat
  that baseline. At a few dozen lines per shopper, cosine over the whole
  transcript already keeps the authoritative row in the top 8.
- FluctlightDB **beats the chat-log** on rule recall (100% vs 60%), decision
  accuracy (100% vs 88%), trace correctness (100% vs 56%), and revocation
  detection (100% vs 50%). The chat-log's recency tie-break drops the older
  delegation when later chat lines share the cue words. On cadence sessions it
  often returns only the receipt, then refuses with `no_rule` instead of
  applying the cap. That refusal is safe, so policy compliance stays at 100%
  for all three memories. Compliance is saturated. It is not evidence that the
  memories behave the same. Illegal buys were 0, 0, and 0.
- On cohort D, the chat-log sometimes retrieves the revocation and not the
  older rule. The shared policy then says `no_rule` rather than `revoked`, so
  revocation handling (did not buy) is 100% while revocation detection is 0%
  for that cohort.
- Cold recall (a call after a write) is slower for FluctlightDB (p50 0.467 ms)
  than for TF-IDF (0.157 ms) or the chat-log (0.184 ms). The all-recall median
  looks faster only because a repeated cue hits the engine's activation cache.
- Cited `source_uri` from `activate` matched the record on 150/150 Fluctlight
  hits in this run. The scripted demo's `resolve()` call did not: it returned
  the unverified chat line (`best match via ChatAssertion`). The benchmark
  does not use `resolve()` for decisions.

Full cohort breakdown, failure rows, and the raw JSON:
[RESULTS.md](RESULTS.md), [results/benchmark.json](results/benchmark.json).

## Caveats

- Beta, single-agent, embedded, synthetic. Not a shopper study.
- The agent is deterministic rules. A model on top of the same hits could do
  worse or better; that was not measured.
- Chat-log and TF-IDF are small-corpus baselines. They are not a hosted memory API.
- If Fluctlight ties or loses a metric, that is the result. On this set it
  ties TF-IDF on quality and loses on cold latency. Exact-query injection
  keeps three verified hits; a newer revocation can lose to older rules that
  share the same cue words. That did not happen on these 150 attempts.
- Provenance helps the trace only when the cited episode is actually returned
  and its `source_uri` survived `activate`. The results file reports that
  match rate separately from consent-id correctness.
- Price, brand, and cadence checks happen in the policy after recall. Memory
  does not enforce them.

## File map

| path | role |
|---|---|
| `shopping_delegation/policy.py` | shared parser and purchase policy |
| `shopping_delegation/memory.py` | Fluctlight, chat-log, TF-IDF |
| `shopping_delegation/dataset.py` | seeded generator |
| `shopping_delegation/bench.py` | metrics, `RESULTS.md`, raw JSON |
| `shopping_delegation/demo.py` | scripted session and `explain` |
| `shopping_delegation/catalog.py` | mock products and price changes |
| `shopping_delegation/viz.py` | static HTML trace |
| `data/sessions.jsonl` | synthetic sessions and ground truth |
| `results/benchmark.json` | raw run |
| `RESULTS.md` | the numbers from that JSON |
| `web/session.html` | trace page written by `make demo` |
| `run.sh` | self-test + demo + benchmark |

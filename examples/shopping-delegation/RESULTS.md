# Shopping delegation benchmark results

SYNTHETIC data only. These numbers are the output of one run on this machine.
They do not describe real shoppers, and they are not a general claim about
FluctlightDB versus other memory systems.

## Machine

- Python 3.12.3
- Linux-6.12.94+-x86_64-with-glibc2.39
- CPU: Intel(R) Xeon(R) Processor (4 logical CPUs)
- Dataset: `shopping-delegation-synthetic-v1` seed 20260330
- Sessions: 36 · attempts: 150 · recall limit: 8
- SHA256 of `data/sessions.jsonl`: `672e0d7a6593dc8e64bc3c0865ad7cf26c2896cb6a212eb7cf5acbb41d515482`

## Headline

Policy compliance here means the agent did not buy when the true policy
forbids the purchase (price, brand, consent, revocation, cadence). A
refusal is always compliant. Decision accuracy also requires the agent
to buy when the standing rule allows it. Trace correctness means the
cited authorising consent (or the revocation, when the rule is dead)
matches ground truth.

| memory | rule recall | policy compliance | decision accuracy | revocation handling | revocation detected | trace correctness | p50 ms | p95 ms | cold p50 ms | cold p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| fluctlight | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% | 0.068 | 0.499 | 0.467 | 0.517 |
| chat_log | 60.0% | 100.0% | 88.0% | 100.0% | 50.0% | 56.0% | 0.180 | 0.214 | 0.184 | 0.235 |
| tfidf | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% | 0.147 | 0.174 | 0.157 | 0.183 |

Illegal buys (agent bought, ground truth refused): fluctlight=0, chat_log=0, tfidf=0.

`p50`/`p95` cover every recall. `cold` keeps only a recall that followed a write.
FluctlightDB caches activation for a repeated cue, so the all-recall median can be
a cache hit. Baselines recompute every time. Cold p95 is the fairer latency read.

Fluctlight engine `source_uri` matched the cited record on 100.0% of 150 cited hits. Baselines have no engine provenance; their trace uses the consent id
parsed from the retrieved line.

## Ingest (Fluctlight only)

The separation gate can refuse an unverified near-duplicate chat line.
Verified delegations skip that gate. Baselines keep every line.

```
{
  "stored": 510,
  "gate_rejected": 0,
  "deduplicated": 0,
  "chat_rejected": 0,
  "verified_rejected": 0
}
```

## By cohort

### fluctlight

| cohort | n | rule recall | decision accuracy | policy compliance | trace | revoked n | revocation handling | revocation detected |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| A_cadence | 24 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| A_single | 18 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| B_update | 48 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| C_stack_revoke | 36 | 100.0% | 100.0% | 100.0% | 100.0% | 6 | 100.0% | 100.0% |
| D_revoke | 24 | 100.0% | 100.0% | 100.0% | 100.0% | 6 | 100.0% | 100.0% |

### chat_log

| cohort | n | rule recall | decision accuracy | policy compliance | trace | revoked n | revocation handling | revocation detected |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| A_cadence | 24 | 0.0% | 75.0% | 100.0% | 0.0% | 0 | n/a | n/a |
| A_single | 18 | 0.0% | 66.7% | 100.0% | 0.0% | 0 | n/a | n/a |
| B_update | 48 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| C_stack_revoke | 36 | 100.0% | 100.0% | 100.0% | 100.0% | 6 | 100.0% | 100.0% |
| D_revoke | 24 | 25.0% | 75.0% | 100.0% | 0.0% | 6 | 100.0% | 0.0% |

### tfidf

| cohort | n | rule recall | decision accuracy | policy compliance | trace | revoked n | revocation handling | revocation detected |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| A_cadence | 24 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| A_single | 18 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| B_update | 48 | 100.0% | 100.0% | 100.0% | 100.0% | 0 | n/a | n/a |
| C_stack_revoke | 36 | 100.0% | 100.0% | 100.0% | 100.0% | 6 | 100.0% | 100.0% |
| D_revoke | 24 | 100.0% | 100.0% | 100.0% | 100.0% | 6 | 100.0% | 100.0% |

## Where it misses

### fluctlight

No missed recalls, decision mismatches, trace misses, or illegal buys in the sample.

### chat_log

Showing 8 of 66 misses.

- `S00-A1` (A_cadence): truth refuse/cadence, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S00-whole_milk-v1`, saw ['buy-S00-whole_milk']
- `S00-A2` (A_cadence): truth buy/in_policy, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S00-whole_milk-v1`, saw ['buy-S00-whole_milk']
- `S00-A3` (A_cadence): truth refuse/over_price, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S00-whole_milk-v1`, saw ['buy-S00-whole_milk']
- `S00-A4` (A_cadence): truth refuse/brand, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S00-whole_milk-v1`, saw ['buy-S00-whole_milk']
- `S01-A1` (A_cadence): truth refuse/cadence, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S01-oat_milk-v1`, saw ['buy-S01-oat_milk']
- `S01-A2` (A_cadence): truth buy/in_policy, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S01-oat_milk-v1`, saw ['buy-S01-oat_milk']
- `S01-A3` (A_cadence): truth refuse/over_price, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S01-oat_milk-v1`, saw ['buy-S01-oat_milk']
- `S01-A4` (A_cadence): truth refuse/brand, agent refuse/no_rule, rule_recall=False, trace_ok=False, safety_ok=True, needed `rule-S01-oat_milk-v1`, saw ['buy-S01-oat_milk']

### tfidf

No missed recalls, decision mismatches, trace misses, or illegal buys in the sample.

## Same, better, worse

- Fluctlight rule recall minus chat-log: +40.0%
- Fluctlight rule recall minus TF-IDF: +0.0%
- Fluctlight decision accuracy minus chat-log: +12.0%
- Fluctlight decision accuracy minus TF-IDF: +0.0%
- Attempts where chat-log recalled the authoritative record and Fluctlight did not: 0
- Attempts where TF-IDF recalled the authoritative record and Fluctlight did not: 0
- Attempts where Fluctlight recalled it and chat-log did not: 60
- Attempts where Fluctlight recalled it and TF-IDF did not: 0

A positive delta means Fluctlight was higher on that metric in this run.
A zero delta means the memories tied. On this set FluctlightDB matching TF-IDF
is a tie, not a win. Latency is not a quality score. Cold p95 is the
number to read: the all-recall median includes Fluctlight activation-cache hits,
while the Python baselines scan the transcript every time.

## What this does not show

- Beta software, one embedded agent, one process, no network.
- SYNTHETIC sessions from a fixed seed. Not a panel of shoppers.
- The purchase policy is a deterministic rule, not a language model.
- Provenance weighting in this demo is the engine's verified flag,
  provenance kind, confidence, and source URI on `experience` / `activate`.
  Exact-looking cues also hit `detect_exact_query`, which injects at most
  three verified engrams ahead of associative hits. That is not a general
  causal graph from action to consent.
- `resolve()` returns a single provenance-weighted winner. The policy
  needs the live rule, a possible revocation, and receipt history together,
  so the measured path is `activate(limit=8)`, not `resolve()`.
  On the scripted demo in this folder, `resolve()` on the same cue returned
  the unverified chat line (`best match via ChatAssertion`), not the verified
  delegation. Provenance weight did not outrank that cue-stuffed chat.
- Cold recall p50 on this machine: FluctlightDB is slower than both baselines.
  The all-recall median looks faster only because repeated cues hit the activation cache.
- Results will move if the cue, the limit, the cohort mix, or the build changes.

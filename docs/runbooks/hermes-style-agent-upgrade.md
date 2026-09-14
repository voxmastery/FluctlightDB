# Hermes-style agent + Fluctlight upgrade

How to cut a new FluctlightDB version next to an agent (ServerBrain today;
Hermes Agent is the architecture we steal from) without another 40-minute
silent open or a WAL/codec split-brain.

**Incident that forced this note:** 2026-09-08 ServerBrain cutover 0.5.19 →
0.5.21 on `~/.fluctlight/tenants/serverbrain-v2/brain`. Serve sat at 100% CPU
for 45+ minutes with no `:8792`. Offline `fluctlight-rekey` never printed
`opened:`. We recovered by parking `CURRENT` + WAL + `tau.seg` + sidecar and
booting the **root snapshot** (3,066 engrams vs 13,614). Full generation is
still on disk, not loaded.

---

## What Hermes gets right (copy this)

Hermes Agent ([NousResearch/hermes-agent](https://github.com/nousresearch/hermes-agent))
is a **narrow waist + fat edges** agent:

| Layer | Hermes | Lesson for Fluctlight next to an agent |
|-------|--------|----------------------------------------|
| Core | `AIAgent` loop only (`run_agent.py`) | Agent code must not embed a Fluctlight version |
| Memory | `MemoryProvider` ABC + one active plugin | Agent talks **HTTP only**; never `FluctlightBrain::open` on the live path |
| Skills | `SKILL.md` at the edge, imported | ServerBrain's `~/.sbridge/skills/hermes/*` stay independent of engine version |
| Session | SQLite + FTS5, not the memory plugin | `bridge-sessions.json` / chat history ≠ the brain |
| Config | `hermes_cli/config.py` migrates | One pin file, one `FLUCTLIGHT_BIN`, no glob-newest |
| Gateway | one process, many platforms | One `fluctlight-serve`; CLI/mindloop/set-goal must not take the store lock |

Hermes memory plugins (`plugins/memory/<name>/`) implement `initialize`,
`is_available` (no network), and tool schemas. Fluctlight should stay that
kind of **swap-in provider**: pin the binary, health-check `/ready`, then
flip the agent. Do not upgrade by editing six systemd drop-ins and hoping
`_newest_fluctlight_bin()` agrees.

ServerBrain already imports Hermes skills (`sb-import-hermes-skills.sh` →
`serverbrain_learning.py`). That is the skill edge. It is **not** a Hermes
runtime and not a claw/ZeroClaw fork.

---

## The brain is one atom

A v4 tenant is **not** “the directory”. These must move together:

1. `CURRENT` → `generations/gen-…/`
2. That generation’s `*.seg` (especially `hippocampus.seg`, `tau.seg`)
3. `wal/` whose next seq matches the generation manifest `wal_seq`
4. `recall_index.sqlite*` (sidecar; rebuilt on every open if present)

**Never** point serve at root `*.seg` while leaving `wal/` from a newer
generation. 0.5.21 refuses that with:

```text
WAL sequence gap: expected 8381, found 156807
```

Parking `CURRENT` without parking `wal/` is how we hit that on 2026-09-08.

`fluctlight verify` is header-only (fast, safe). It does **not** prove
`open()` will finish or that WAL matches.

---

## Why 0.5.x cutovers hurt

Observed on this host:

- **Many binaries, no single pin.** systemd `0.5.19`, CLI glob → `0.5.21`,
  `fluctlight-current` → `0.5.17`, pip SDK `0.5.10`, `/usr/local/bin` `0.5.17`.
- **CLI opens the live brain.** Mindloop `set-goal` and `fluctlight-rekey`
  take the exclusive flock. Serve cannot bind while they run.
- **`--version` hangs.** Use `fluctlight` with no args (help) or
  `timeout 3 … help`.
- **Open is unbounded.** Active gen had a **400 MB `tau.seg`**. Sidecar
  rebuilds HNSW (`ef_construction=200`) for every `engram_vec` row on open.
  No `/live` until both finish. 0.5.19 then grew to 15 GB with
  `FLUCTLIGHT_FABRIC=1` and OOM-looped (restart 70).
- **Codec is a one-way door.** 0.5.20 can strand a pre-FLCT1 brain. 0.5.21
  fixes the flip. Incremental `drain` still wipes learned synapse weights.
  Offline `target/release/fluctlight-rekey` (HEAD after 0.5.21) is the
  weight-preserving path — and it must finish `open()` first.
- **Named `fluctlight-0.5.20` on disk (Aug 19) is not the published 0.5.20.**
  Do not run it.

---

## Easy upgrade (do this next time)

### 0. Pin (one place)

```bash
# /etc/fluctlight/version  — or the serve drop-in only
FLUCTLIGHT_BIN=/home/ambugo/fluctlightdb/fluctlight-0.5.21
```

Set that env on **serve, stream, drill, and the agent**. Do not use
`glob(fluctlight-*)` for production. `scripts/preflight-serve.sh` fails if
`FLUCTLIGHT_BIN` is unset or not executable.

### 1. Preflight (copy, do not touch live)

```bash
export FLUCTLIGHT_BRAIN_PATH=~/.fluctlight/tenants/serverbrain-v2/brain
export FLUCTLIGHT_BIN=/path/to/fluctlight-X.Y.Z
./scripts/preflight-serve.sh
rsync -a "$FLUCTLIGHT_BRAIN_PATH/" /tmp/fl-upgrade-copy/
# open the COPY on a side port — never the live flock
timeout 180 $FLUCTLIGHT_BIN serve --addr 127.0.0.1:8799 --path /tmp/fl-upgrade-copy
# must print "listening" and /ready 200. If not in 3 minutes, abort.
```

If `tau.seg` in the active generation is hundreds of MB, budget **tens of
minutes** or park it *on the copy only* for a smoke serve. Do not park
`CURRENT` without parking `wal/`.

### 2. Offline codec (only if CHANGELOG says so)

Stop serve. Snapshot. Run `fluctlight-rekey` on a **copy** first, then live.
Never let 0.5.21 serve drain 4-engrams-per-write on a 13k legacy brain
(learned weights collapse). See CHANGELOG 0.5.21.

### 3. Flip

1. `systemctl mask --runtime fluctlight-serve` (prevents 3s restart races).
2. Stop serve. Kill any `fluctlight … --path <live>` CLI (set-goal, status, rekey).
3. Point `ExecStart` at the pinned binary (empty `ExecStart=` then new line).
4. `unmask`, `daemon-reload`, start.
5. Wait for `/live` then `/ready`. Only then restart the agent and watchdog.

Set `MemoryMax=` on the unit. `FLUCTLIGHT_FABRIC=1` on a large brain can
run RSS to 15 GB during open.

### 4. Rollback

Keep the previous binary. Revert the drop-in, start, confirm `/ready`.
Restore from `~/.fluctlight/backups/` if the new binary wrote a generation
the old one cannot read.

---

## Agent wiring (ServerBrain)

| Do | Do not |
|----|--------|
| Agent → `FLUCTLIGHT_SERVE_URL` HTTP | Agent CLI `fluctlight set-goal --path <live>` |
| `FLUCTLIGHT_BIN` in every unit | `_newest_fluctlight_bin()` as the serve pin |
| `FLUCTLIGHT_BRAIN` = serve `--path` | Bridge env pointing at `tenants/default` while serve uses `serverbrain-v2` |
| Hermes skills in `~/.sbridge/skills` | Re-import Hermes as a reason to bump Fluctlight |
| Watchdog only after `/ready` | Watchdog restarting a still-opening serve |

Mindloop / CLOOP must use HTTP or a **copy**. Exclusive `open()` on the live
tenant is a serve outage.

---

## After a degraded boot

If serve is up on the **root snapshot** (no `CURRENT`, WAL parked):

- Recall works but is **stale/small**.
- Parked pieces: `CURRENT.aside-*`, `wal.aside-*`,
  `generations/…/tau.seg.aside-*`, `aside-*/recall_index.sqlite*`.
- Restore is: stop serve → put the four back **together** → start the
  pinned binary → wait `/ready` (may be 15–40 min). Do not restore WAL
  onto the root snapshot alone.

---

## Related

- [PRODUCTION.md](../PRODUCTION.md) — pin policy
- [backup-restore.md](backup-restore.md)
- [serve-crash-recovery.md](serve-crash-recovery.md)
- [migration-v3-v4.md](migration-v3-v4.md)
- `scripts/preflight-serve.sh` — atom + pin check
- `scripts/resolve-brain.sh` — tenant path

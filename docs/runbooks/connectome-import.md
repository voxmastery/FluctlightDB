# Connectome import (FlyWire v783)

## Download (≈53 MB, CC-BY-4.0, no token)

    B=https://storage.googleapis.com/flywire-data/codex/data/fafb/783
    mkdir -p /var/lib/fluctlight/flywire-783 && cd $_
    for f in connections neurons classification; do curl -sO $B/$f.csv.gz && gunzip -f $f.csv.gz; done

Cite: Dorkenwald et al., *Nature* 2024, doi:10.5281/zenodo.10676866.

## Lock rule

`import-connectome` opens the brain with the **exclusive** store lock. It must not run against a
tenant that `fluctlight-serve` has open — stop serve first, or import into a fresh path and swap
(hermes-style-agent-upgrade.md §3). The CLI exits non-zero if the lock is held.

## Run

    export FLUCTLIGHT_STORAGE=v4
    fluctlight import-connectome --path ~/.fluctlight/tenants/fly-783/brain \
      --connections /var/lib/fluctlight/flywire-783/connections.csv \
      --classification /var/lib/fluctlight/flywire-783/classification.csv \
      --neurons /var/lib/fluctlight/flywire-783/neurons.csv

Expect ≈6 s, ≈400 MB peak RSS, ≈145 MB on disk, recall ≈12 ms, and a JSON report with
`pairs ≈ 2,700,513`, `entry_set_len = 5177`. The importer parses everything before touching the
graph: a failed import leaves the brain exactly as it was.

Check the report: `rows_malformed` should be 0 and `inhibitory_synapses` well above 0 (≈1.1M for
v783). A zero there means the wrong `neurons.csv`.

## Compatibility (read this)

A fused tenant opens under an older binary, but that binary does not know negative weights: its
first sleep cycle prunes or rewrites every inhibitory synapse. Once imported, the tenant requires
the version that shipped this feature or newer. `graphprune` likewise prunes by weight ascending
and must not be run against a fused tenant.

## Verify

    curl -s -X POST http://127.0.0.1:8792/api/v1/connectome -d '{}'   # {"present":true,...}
    curl -s -X POST http://127.0.0.1:8792/api/v1/activate -d '{"cue":"odor sugar"}' | jq .connectome_seeds   # 14

## Replace / roll back

`--replace` removes every synapse touching a neuron of the previous connectome, then imports.
To roll back entirely, restore the tenant from `~/.fluctlight/backups/` (backup-restore.md) —
there is no partial undo.

`export-raw` / `import-raw` carry the connectome as of this change; dumps from before it restore
the wiring but not the entry set — re-import.

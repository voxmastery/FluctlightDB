# Open Brain Snapshot format (v1)

Portable interchange for agent memory between frameworks, backups, and migrations.

## Format

```json
{
  "format": "fluctlight-brain-snapshot",
  "version": 1,
  "exported_at_tick": 42,
  "engrams": [ ... ],
  "chronos": { ... },
  "agent_state": { ... },
  "retention_policy": { ... },
  "notes": "..."
}
```

## Python API

```python
from fluctlightdb import connect_agent

brain = connect_agent("/data/agent.brain")
blob = brain.export_snapshot()
brain2 = connect_agent("/data/other.brain")
report = brain2.import_snapshot(blob)
print(report)  # engrams_imported, skipped_duplicates
```

## Use cases

- Export from LangChain agent → import into FluctlightDB native brain
- Backup before schema migration
- Share read-only memory pack across team (no WAL replication)
- Cross-framework migration (Mem0-style JSON → snapshot import via adapter)

## Not included in v1

- CHORUS traces (runtime-only; rebuild via ingest)
- Synapse graph (use `export_raw` with `FLUCTLIGHT_EXPORT_SYNAPSES=1` for research)
- WAL / checkpoint files (use `fluctlight-project replicate` for hot sync)

See also: `fluctlight-project replicate` for incremental primary→replica sync.

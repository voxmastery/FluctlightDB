# Synapse pressure high

## Symptoms

- `synapse_pressure` > 0.7 in status
- Slow activate / experience
- `auto_sleeps` not keeping up

## Actions

1. Check status: `curl -X POST http://127.0.0.1:8792/api/v1/status`
2. Run compact: `curl -X POST http://127.0.0.1:8792/api/v1/compact`
3. Confirm budget wiring active (Phase 0.2) — growth should flatten
4. If still high, backup then run manual sleep loop: `fluctlight tick 10`
5. Review duplicate experiences in bridge feed

Pressure compact triggers automatically at 0.7 during experience.

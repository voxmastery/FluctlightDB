# FluctlightDB — Development Stages

Growth is **automatic**. After Stage 0/1 boot, the brain advances when metrics cross thresholds.

## Stages

| Stage | Biology | Unlocks | Auto-advance requires |
|-------|---------|---------|------------------------|
| **Embryonic** | Neural tube | Reflex encoding only | (boot) |
| **Newborn** | Synaptic overproduction | Full episodic encode | 1 tick (first heartbeat) |
| **Infant** | Blooming | High salience weight | 3 experiences, 5 ticks |
| **Child** | Pruning begins | Schema consolidation in sleep | 15 exp, 1 sleep, 20 ticks |
| **Adolescent** | PFC online | Goals, inhibition | 50 exp, 3 sleep, 100 ticks, 10 pruned |
| **Adult** | Stable engrams | Efficient completion | 150 exp, 8 sleep, 300 ticks, 100 pruned |
| **Expert** | Rich cortex | Cross-domain replay boost | 500 exp, 20 sleep, 1000 ticks, 500 pruned |

## What changes per stage

- **max_synapses** — cap before forced sleep
- **myelination** — activation spread speed
- **prune_threshold** — sleep pruning aggressiveness
- **prefrontal.unlocked** — adolescent+

## Triggers (always on)

Every `experience()` → `on_experience()` → maybe advance  
Every `sleep()` → `on_sleep()` → maybe advance  
Every tick → `maybe_advance()`

**You do not set stage manually.** You live, sleep, and the brain grows.

## Death

`death()` clears ephemeral hippocampal engrams for the ended life. Core memories and cortex facts survive. Death counts toward resilience metrics.

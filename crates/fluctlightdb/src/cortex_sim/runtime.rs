//! Deterministic multi-node placement/fencing simulation runtime.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::placement::{
    DurabilityPolicy, Placement, PlacementError, PlacementReconciler, PlacementState, WriteFence,
};

use super::clock::CortexClock;
use super::fs::CortexFs;
use super::net::{CortexNet, NodeId};
use super::rng::CortexRng;

pub type TraceHash = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimEvent {
    Boot {
        node: NodeId,
    },
    Experience {
        node: NodeId,
        content: String,
        accepted: bool,
    },
    Partition {
        node: NodeId,
    },
    Promote {
        from: NodeId,
        to: NodeId,
        generation: u64,
        ok: bool,
    },
    StaleWriteRejected {
        node: NodeId,
        generation: u64,
    },
    Activate {
        node: NodeId,
        hits: usize,
    },
    TraceNote(String),
}

#[derive(Debug, Clone)]
struct SimNode {
    log: Vec<String>,
    fence_generation: u64,
}

#[derive(Debug)]
pub struct CortexRuntime {
    seed: u64,
    clock: CortexClock,
    rng: CortexRng,
    net: CortexNet,
    fs: CortexFs,
    placement: Placement,
    nodes: BTreeMap<NodeId, SimNode>,
    events: Vec<SimEvent>,
    isolated: BTreeSet<NodeId>,
}

impl CortexRuntime {
    pub fn bootstrap_three_node(seed: u64, tenant_uuid: uuid::Uuid) -> Self {
        let nodes = [1u64, 2, 3];
        let mut map = BTreeMap::new();
        for id in nodes {
            map.insert(
                id,
                SimNode {
                    log: Vec::new(),
                    fence_generation: 1,
                },
            );
        }
        let placement = Placement {
            tenant_uuid,
            generation: 1,
            primary: Some(1),
            members: BTreeSet::from(nodes),
            draining: BTreeSet::new(),
            durable_watermarks: BTreeMap::from([(1, 0), (2, 0), (3, 0)]),
            committed_watermark: 0,
            durability: DurabilityPolicy::Quorum,
        };
        let mut runtime = Self {
            seed,
            clock: CortexClock::new(),
            rng: CortexRng::new(seed),
            net: CortexNet::new(nodes),
            fs: CortexFs::new(),
            placement,
            nodes: map,
            events: Vec::new(),
            isolated: BTreeSet::new(),
        };
        for id in nodes {
            runtime.record(SimEvent::Boot { node: id });
        }
        runtime
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn events(&self) -> &[SimEvent] {
        &self.events
    }

    pub fn trace_hash(&self) -> TraceHash {
        let mut hasher = Sha256::new();
        hasher.update(self.seed.to_le_bytes());
        for event in &self.events {
            hasher.update(format!("{event:?}").as_bytes());
            hasher.update(b"|");
        }
        hasher.finalize().into()
    }

    pub fn experience(
        &mut self,
        node: NodeId,
        content: impl Into<String>,
    ) -> Result<(), PlacementError> {
        let content = content.into();
        self.clock.advance(1_000 + self.rng.gen_range(500));
        let fence = WriteFence {
            tenant_uuid: self.placement.tenant_uuid,
            node_id: node,
            generation: self
                .nodes
                .get(&node)
                .map(|n| n.fence_generation)
                .unwrap_or(0),
        };
        match self.placement.authorize_write(&fence) {
            Ok(()) => {
                let entry = self.nodes.get_mut(&node).expect("node");
                entry.log.push(content.clone());
                let watermark = entry.log.len() as u64;
                self.placement.durable_watermarks.insert(node, watermark);
                self.placement.committed_watermark = watermark;
                self.fs
                    .write(format!("node-{node}/log-{watermark}"), content.as_bytes());
                let _ = self.net.send(node, node, format!("ack:{watermark}"));
                self.record(SimEvent::Experience {
                    node,
                    content,
                    accepted: true,
                });
                Ok(())
            }
            Err(error) => {
                self.record(SimEvent::Experience {
                    node,
                    content,
                    accepted: false,
                });
                if matches!(
                    error,
                    PlacementError::StaleGeneration { .. } | PlacementError::NotPrimary { .. }
                ) {
                    self.record(SimEvent::StaleWriteRejected {
                        node,
                        generation: fence.generation,
                    });
                }
                Err(error)
            }
        }
    }

    pub fn partition(&mut self, node: NodeId) {
        self.clock.advance(5_000);
        self.net.isolate(node);
        self.isolated.insert(node);
        self.record(SimEvent::Partition { node });
    }

    pub fn promote(
        &mut self,
        candidate: NodeId,
        new_generation: u64,
    ) -> Result<(), PlacementError> {
        self.clock.advance(2_000);
        let previous = self.placement.primary.unwrap_or(0);
        let expected = self.placement.generation;
        match self.placement.promote(candidate, expected, new_generation) {
            Ok(next) => {
                self.placement = next;
                let primary_log = self
                    .nodes
                    .get(&candidate)
                    .map(|n| n.log.clone())
                    .unwrap_or_default();
                for (id, node) in self.nodes.iter_mut() {
                    if self.isolated.contains(id) {
                        continue;
                    }
                    node.fence_generation = new_generation;
                    if node.log.len() < primary_log.len() {
                        node.log = primary_log.clone();
                    }
                }
                if let Some(node) = self.nodes.get_mut(&candidate) {
                    node.fence_generation = new_generation;
                }
                self.record(SimEvent::Promote {
                    from: previous,
                    to: candidate,
                    generation: new_generation,
                    ok: true,
                });
                Ok(())
            }
            Err(error) => {
                self.record(SimEvent::Promote {
                    from: previous,
                    to: candidate,
                    generation: new_generation,
                    ok: false,
                });
                Err(error)
            }
        }
    }

    pub fn activate(&mut self, node: NodeId, cue: &str) -> usize {
        self.clock.advance(750);
        let hits = self
            .nodes
            .get(&node)
            .map(|n| n.log.iter().filter(|entry| entry.contains(cue)).count())
            .unwrap_or(0);
        self.record(SimEvent::Activate { node, hits });
        hits
    }

    pub fn local_state(&self, node: NodeId) -> PlacementState {
        PlacementReconciler::new(node)
            .reconcile(Some(&self.placement))
            .state
    }

    pub fn is_isolated(&self, node: NodeId) -> bool {
        self.isolated.contains(&node)
    }

    pub fn heal(&mut self, node: NodeId) {
        self.net.heal(node);
        self.isolated.remove(&node);
        self.record(SimEvent::TraceNote(format!("heal:{node}")));
    }

    pub fn run_failover_scenario(&mut self, content: &str) {
        self.experience(1, content)
            .expect("primary must accept first experience");
        // Replicate to followers before partition (quorum path).
        let log = self
            .nodes
            .get(&1)
            .map(|n| n.log.clone())
            .unwrap_or_default();
        for id in [2u64, 3] {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.log = log.clone();
                self.placement
                    .durable_watermarks
                    .insert(id, log.len() as u64);
            }
        }
        self.placement.committed_watermark = log.len() as u64;
        self.partition(1);
        self.promote(2, 2).expect("caught-up follower promotes");
        let stale = self.experience(1, "stale after fence");
        assert!(stale.is_err(), "partitioned primary must be fenced");
        self.experience(2, "post-failover")
            .expect("new primary accepts writes");
        let hits = self.activate(2, content);
        assert!(hits >= 1, "new primary must recall pre-failover engram");
    }

    fn record(&mut self, event: SimEvent) {
        self.events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_replay_produces_identical_trace_hash() {
        let tenant = uuid::Uuid::from_u128(42);
        let mut a = CortexRuntime::bootstrap_three_node(0xC0_87_E7_01, tenant);
        a.run_failover_scenario("reflex: cortex seed");
        let mut b = CortexRuntime::bootstrap_three_node(0xC0_87_E7_01, tenant);
        b.run_failover_scenario("reflex: cortex seed");
        assert_eq!(a.trace_hash(), b.trace_hash());
        assert_ne!(
            a.trace_hash(),
            CortexRuntime::bootstrap_three_node(0xDEAD_BEEF, tenant).trace_hash()
        );
    }

    #[test]
    fn failover_scenario_fences_stale_primary_and_preserves_experience() {
        let mut runtime = CortexRuntime::bootstrap_three_node(7, uuid::Uuid::from_u128(99));
        runtime.run_failover_scenario("agent learned dark mode");
        assert_eq!(runtime.local_state(2), PlacementState::Primary);
        assert!(runtime
            .events()
            .iter()
            .any(|event| matches!(event, SimEvent::StaleWriteRejected { node: 1, .. })));
        assert!(runtime.activate(2, "dark mode") >= 1);
    }
}

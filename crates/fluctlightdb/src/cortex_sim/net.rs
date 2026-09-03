//! In-memory partitionable message fabric.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type NodeId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: NodeId,
    pub to: NodeId,
    pub payload: String,
}

#[derive(Debug, Default)]
pub struct CortexNet {
    queues: BTreeMap<NodeId, VecDeque<Envelope>>,
    /// Undirected partitions: nodes that cannot exchange messages.
    isolated: BTreeSet<NodeId>,
}

impl CortexNet {
    pub fn new(nodes: impl IntoIterator<Item = NodeId>) -> Self {
        let mut queues = BTreeMap::new();
        for node in nodes {
            queues.insert(node, VecDeque::new());
        }
        Self {
            queues,
            isolated: BTreeSet::new(),
        }
    }

    pub fn isolate(&mut self, node: NodeId) {
        self.isolated.insert(node);
    }

    pub fn heal(&mut self, node: NodeId) {
        self.isolated.remove(&node);
    }

    pub fn send(&mut self, from: NodeId, to: NodeId, payload: impl Into<String>) -> bool {
        if self.isolated.contains(&from) || self.isolated.contains(&to) {
            return false;
        }
        if let Some(queue) = self.queues.get_mut(&to) {
            queue.push_back(Envelope {
                from,
                to,
                payload: payload.into(),
            });
            true
        } else {
            false
        }
    }

    pub fn recv(&mut self, node: NodeId) -> Option<Envelope> {
        self.queues.get_mut(&node)?.pop_front()
    }
}

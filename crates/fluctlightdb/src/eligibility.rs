//! Eligibility tags — wake experiences tagged for CaptureGate sleep capture (Frey & Morris).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EligibilityStore {
    pub tags: HashSet<Uuid>,
}

impl EligibilityStore {
    pub fn tag(&mut self, id: Uuid) {
        self.tags.insert(id);
    }

    pub fn is_tagged(&self, id: Uuid) -> bool {
        self.tags.contains(&id)
    }

    pub fn clear(&mut self) {
        self.tags.clear();
    }

    pub fn take_tagged(&mut self) -> HashSet<Uuid> {
        std::mem::take(&mut self.tags)
    }

    pub fn retain_untagged_only(&mut self, keep: &HashSet<Uuid>) {
        self.tags = self.tags.intersection(keep).copied().collect();
    }
}

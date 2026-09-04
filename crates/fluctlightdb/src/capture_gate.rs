//! CaptureGate — interleaved capture + CF probes (Phase B).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::engram::Engram;
use crate::error::Result;
use crate::schema::{crystallize_from_engrams, SchemaStore};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureReport {
    pub captured: usize,
    pub rolled_back: bool,
    pub interference_breach: bool,
}

/// Capture only eligibility-tagged engrams into schemas; snapshot/restore on CF breach.
pub fn capture_schemas(
    store: &mut SchemaStore,
    engrams: &[Engram],
    tagged: &HashSet<Uuid>,
    cf_probe_keys: &[&str],
) -> Result<CaptureReport> {
    let snapshot = store.clone();
    let before_active: Vec<_> = store
        .active()
        .filter(|s| {
            cf_probe_keys
                .iter()
                .any(|k| s.key == *k || s.statement.to_lowercase().contains(k))
        })
        .map(|s| (s.id, s.statement.clone(), s.support_engram_ids.clone()))
        .collect();

    let tagged_engrams: Vec<Engram> = engrams
        .iter()
        .filter(|e| tagged.contains(&e.id))
        .cloned()
        .collect();

    if tagged_engrams.is_empty() {
        return Ok(CaptureReport {
            captured: 0,
            rolled_back: false,
            interference_breach: false,
        });
    }

    // Interleave: tagged new + supports of existing schemas touching same keys
    let mut pool = tagged_engrams;
    let mut seen: HashSet<Uuid> = pool.iter().map(|e| e.id).collect();
    for s in store.active() {
        for id in &s.support_engram_ids {
            if seen.insert(*id) {
                if let Some(e) = engrams.iter().find(|e| e.id == *id) {
                    pool.push(e.clone());
                }
            }
        }
    }

    let before_count = store.schemas.len();
    crystallize_from_engrams(store, &pool);
    let captured = store.schemas.len().saturating_sub(before_count);

    // CF: old support engrams must still exist; old schema rows retained (active or superseded)
    let mut breach = false;
    for (_id, _stmt, supports) in &before_active {
        for sid in supports {
            if !engrams.iter().any(|e| e.id == *sid) {
                breach = true;
                break;
            }
        }
        if breach {
            break;
        }
    }
    // Also: if we had active probes, at least one schema row (any status) must still mention them
    if !before_active.is_empty() {
        for (id, _, _) in &before_active {
            if store.get(*id).is_none() {
                breach = true;
                break;
            }
        }
    }

    if breach {
        *store = snapshot;
        return Ok(CaptureReport {
            captured: 0,
            rolled_back: true,
            interference_breach: true,
        });
    }

    Ok(CaptureReport {
        captured,
        rolled_back: false,
        interference_breach: false,
    })
}

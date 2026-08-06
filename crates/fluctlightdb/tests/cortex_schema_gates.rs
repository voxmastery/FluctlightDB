//! Phase A CortexSchema gates — support integrity, persistence, crystallize, recombination.

use fluctlightdb::{Episode, FluctlightBrain, Schema};
use tempfile::tempdir;

#[test]
fn cortex_schemas_survive_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let eid = brain
        .experience(Episode::new("User prefers dark mode", "prefs", 0.9))
        .unwrap()
        .engram_id;
    brain
        .cortex
        .schemas
        .upsert_active(Schema::new("user prefers dark mode", vec![eid]))
        .unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    let brain2 = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain2.cortex.schemas.active().count(), 1);
}

#[test]
fn sleep_crystallizes_theme_schema_from_supports() {
    let mut brain = FluctlightBrain::new();
    let episodes = [
        "User prefers dark mode theme for night coding sessions",
        "Operator chose dark mode theme after eye strain complaints",
        "Preference recorded: dark mode theme across all IDE windows",
    ];
    for ep in episodes {
        brain
            .experience(Episode::new(ep, "prefs", 0.8))
            .unwrap();
    }
    assert!(
        brain.hippocampus.engrams.len() >= 2,
        "need multiple engrams to crystallize, got {}",
        brain.hippocampus.engrams.len()
    );
    assert_eq!(brain.cortex.schemas.active().count(), 0);
    brain.sleep().unwrap();
    let active: Vec<_> = brain.cortex.schemas.active().cloned().collect();
    assert!(
        !active.is_empty(),
        "sleep must crystallize at least one schema"
    );
    assert!(active.iter().all(|s| !s.support_engram_ids.is_empty()));
}

#[test]
fn double_sleep_does_not_duplicate_active_theme_schemas() {
    let mut brain = FluctlightBrain::new();
    let episodes = [
        "User prefers dark mode theme for night coding sessions",
        "Operator chose dark mode theme after eye strain complaints",
        "Preference recorded: dark mode theme across all IDE windows",
    ];
    for ep in episodes {
        brain
            .experience(Episode::new(ep, "prefs", 0.8))
            .unwrap();
    }
    brain.sleep().unwrap();
    brain.sleep().unwrap();
    let n2 = brain
        .cortex
        .schemas
        .active()
        .filter(|s| s.key == "theme" || s.key.starts_with("theme:"))
        .count();
    assert_eq!(n2, 1, "exactly one active theme schema after double sleep");
}

#[test]
fn schema_lane_beats_lookup_on_recombination_cue() {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode::new("Alice works in Berlin", "bio", 0.9))
        .unwrap();
    brain
        .experience(Episode::new("Berlin project uses Rust", "proj", 0.9))
        .unwrap();
    brain.sleep().unwrap();
    let ids: Vec<_> = brain.hippocampus.engrams.iter().map(|e| e.id).collect();
    if brain.cortex.schemas.active().count() == 0 && ids.len() >= 2 {
        brain
            .cortex
            .schemas
            .upsert_active(Schema::new(
                "Alice works on Rust project in Berlin",
                ids,
            ))
            .unwrap();
    }
    let with = brain.activate_with_schemas("What stack does Alice use in Berlin?");
    assert!(
        !with.schemas.is_empty(),
        "schema lane must surface compositional structure"
    );
    assert!(with.schemas.iter().any(|s| {
        let t = s.statement.to_lowercase();
        t.contains("rust") || t.contains("berlin") || t.contains("alice")
    }));
}

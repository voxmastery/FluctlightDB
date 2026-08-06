//! Phase B CaptureGate gates.

use fluctlightdb::{Episode, FluctlightBrain, SchemaStatus};

#[test]
fn experience_sets_eligibility_tag() {
    let mut brain = FluctlightBrain::new();
    let id = brain
        .experience(Episode::new("new fact about theme dark", "t", 0.8))
        .unwrap()
        .engram_id;
    assert!(brain.cortex.eligibility.is_tagged(id));
}

#[test]
fn untagged_material_does_not_alter_schemas() {
    // Distinct supports so Claude's stricter separation gate still admits them.
    let mut brain = FluctlightBrain::new();
    let episodes = [
        "User prefers dark mode theme for night coding sessions",
        "Operator chose dark mode theme after eye strain complaints",
        "Preference recorded: dark mode theme across all IDE windows",
    ];
    for ep in episodes {
        brain
            .experience(Episode::new(ep, "prefs", 0.9))
            .unwrap();
    }
    brain.sleep().unwrap();
    let n = brain.cortex.schemas.active().count();
    assert!(
        n >= 1,
        "expected theme schema after sleep, engrams={}",
        brain.hippocampus.engrams.len()
    );
    brain.cortex.eligibility.clear();
    brain.sleep().unwrap();
    assert_eq!(brain.cortex.schemas.active().count(), n);
}

#[test]
fn conflicting_new_experience_does_not_destroy_old_schema_probe() {
    let mut brain = FluctlightBrain::new();
    let dark = [
        "User prefers dark mode theme for night coding sessions",
        "Operator chose dark mode theme after eye strain complaints",
        "Preference recorded: dark mode theme across all IDE windows",
    ];
    for ep in dark {
        brain.experience(Episode::new(ep, "prefs", 0.9)).unwrap();
    }
    brain.sleep().unwrap();
    let old_probe = brain.activate_with_schemas("dark mode theme");
    assert!(
        !old_probe.schemas.is_empty(),
        "expected dark schema probe, engrams={}",
        brain.hippocampus.engrams.len()
    );
    let light = [
        "User prefers light mode theme for daytime outdoor glare",
        "Operator switched to light mode theme for presentation decks",
        "Preference recorded: light mode theme on shared office monitors",
        "Design review chose light mode theme for print proofs",
        "Accessibility audit kept light mode theme for high ambient light",
    ];
    for ep in light {
        brain.experience(Episode::new(ep, "prefs", 0.9)).unwrap();
    }
    brain.sleep().unwrap();
    assert!(
        brain
            .hippocampus
            .engrams
            .iter()
            .any(|e| e.episode.content.contains("dark")),
        "old episodes must not be deleted"
    );
    assert!(
        brain.cortex.schemas.schemas.iter().any(|s| {
            s.statement.to_lowercase().contains("dark")
                || s.status == SchemaStatus::Superseded
                || s.status == SchemaStatus::Active
        }),
        "schema history retained"
    );
    let _ = old_probe;
}

//! Homeostasis metrics + agent prompt packing (no activate ranking changes).

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{Episode, FluctlightBrain};
use tempfile::tempdir;

#[test]
fn status_includes_homeostasis_and_seal_increments() {
    let _guard = EnvGuard::acquire(&["FLUCTLIGHT_STORAGE", "FLUCTLIGHT_SOMNUS"]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let before = brain.status().homeostasis.systems_seals_total;
    brain.systems_seal().unwrap();
    let st = brain.status();
    assert!(st.homeostasis.somnus_enabled);
    assert!(st.homeostasis.systems_seals_total > before);
    assert_eq!(st.homeostasis.ticks_since_systems_seal, 0);
    assert!(st.homeostasis.generation_dirs.is_some());
    assert_eq!(st.homeostasis.generation_count_ok, Some(true));
}

#[test]
fn agent_prompt_records_tokens_without_changing_activate() {
    let mut brain = FluctlightBrain::new();
    for i in 0..6 {
        brain
            .experience(Episode::new(
                format!("user prefers setting {i} dark mode theme"),
                "prefs",
                0.8,
            ))
            .unwrap();
    }
    let before = brain.activate("dark mode");
    let bundle = brain.activate_for_agent_prompt("dark mode");
    let after = brain.activate("dark mode");
    assert_eq!(
        before
            .recalls
            .iter()
            .map(|r| r.engram_id)
            .collect::<Vec<_>>(),
        after
            .recalls
            .iter()
            .map(|r| r.engram_id)
            .collect::<Vec<_>>(),
    );
    assert!(bundle.estimated_tokens > 0);
    assert!(bundle.recalls.len() <= bundle.max_engrams);
    let st = brain.status();
    assert_eq!(st.homeostasis.agent_prompt_calls, 1);
    assert!(st.homeostasis.tokens_within_budget);
}

#[test]
fn session_boot_context_returns_prompt_block() {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode::new(
            "I am ServerBrain continuity organ",
            "identity",
            0.9,
        ))
        .unwrap();
    let boot = brain.session_boot_context(Some("identity"));
    assert!(!boot.prompt_block.is_empty());
}

//! Phase C Aeterna gates — lossless prompt index + session boot.

use fluctlightdb::{Episode, FluctlightBrain};
use std::collections::HashSet;
use tempfile::tempdir;

#[test]
fn prompt_pack_lists_every_activate_id() {
    let mut brain = FluctlightBrain::new();
    for i in 0..12 {
        brain
            .experience(Episode::new(
                format!("dark mode detail {i} with extra words for gist"),
                "p",
                0.7,
            ))
            .unwrap();
    }
    std::env::set_var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET", "64");
    let full = brain.activate("dark mode");
    let bundle = brain.activate_for_agent_prompt("dark mode");
    std::env::remove_var("FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET");
    let line_ids: HashSet<_> = bundle.lines.iter().map(|l| l.engram_id).collect();
    for r in &full.recalls {
        assert!(
            line_ids.contains(&r.engram_id),
            "silent drop forbidden for {}",
            r.engram_id
        );
    }
    assert!(!bundle.truncated);
    if !bundle.expandable_ids.is_empty() {
        let expanded = brain.expand_engrams(&bundle.expandable_ids);
        assert_eq!(expanded.len(), bundle.expandable_ids.len());
    }
}

#[test]
fn session_boot_after_reopen_has_continuity() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("b");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain.core_memories.persist(
        "identity".into(),
        "I am the agent".into(),
        brain.life.life_id,
        None,
    );
    brain
        .experience(Episode::new("prefers dark mode theme", "prefs", 0.9))
        .unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let boot = brain.session_boot_context(Some("dark mode"));
    assert!(!boot.prompt_block.is_empty());
    assert!(
        boot.core_snippets.iter().any(|c| c.contains("agent"))
            || boot.lines.iter().any(|l| l.gist.contains("dark") || l.full_content.as_ref().map(|c| c.contains("dark")).unwrap_or(false))
    );
}

#[test]
fn systems_seal_does_not_change_activate_ids() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    for i in 0..4 {
        brain
            .experience(Episode::new(format!("seal safe memory {i} dark"), "t", 0.7))
            .unwrap();
    }
    let before: Vec<_> = brain
        .activate("dark")
        .recalls
        .iter()
        .map(|r| r.engram_id)
        .collect();
    brain.systems_seal().unwrap();
    let after: Vec<_> = brain
        .activate("dark")
        .recalls
        .iter()
        .map(|r| r.engram_id)
        .collect();
    assert_eq!(before, after);
}

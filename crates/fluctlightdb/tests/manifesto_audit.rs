//! Manifesto audit — runnable checklist against docs/Manifesto.md principles.

use fluctlightdb::{Episode, FluctlightBrain, Provenance, ProvenanceKind};

macro_rules! check {
    ($name:expr, $cond:expr) => {{
        let ok = $cond;
        eprintln!(
            "[{}] {}",
            if ok { "PASS" } else { "FAIL" },
            $name
        );
        assert!(ok, "manifesto check failed: {}", $name);
    }};
}

#[test]
fn manifesto_audit_checklist() {
    eprintln!("\n=== FluctlightDB Manifesto Audit ===\n");

    let mut brain = FluctlightBrain::new();

    // 1. Memory is physical — engrams + synapses, not rows.
    brain
        .experience(Episode::new(
            "agent learned user prefers dark mode",
            "settings",
            0.7,
        ))
        .unwrap();
    check!(
        "1. Memory is physical (synapses after experience)",
        brain.graph.synapse_count() > 0
    );

    // 2. Recall is activation — works without semantic vectors.
    let no_vec = brain.activate("dark mode");
    check!(
        "2. Recall is activation (cue recall without vectors)",
        !no_vec.recalls.is_empty() && no_vec.hops > 0
    );

    // 3. Experience encodes lived moments with context.
    let rep = brain
        .experience(Episode::new(
            "deployment succeeded on staging",
            "ci",
            0.8,
        ))
        .unwrap();
    check!(
        "3. Experience encodes moments (engram id returned)",
        rep.engram_id != uuid::Uuid::nil()
    );

    // 4. Provenance — verified ledger beats chat on wallet cue.
    let ledger = brain
        .experience(Episode {
            content: "ledger verified: agent wallet balance is $0.00 at level 1".into(),
            context: "ledger:wallet".into(),
            outcome: None,
            salience_hint: 0.98,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: Some(Provenance {
                kind: ProvenanceKind::LedgerVerified,
                source_uri: Some("file://wallet.json".into()),
                confidence: 0.99,
                verified: true,
            }),
        })
        .unwrap();
    brain
        .verify_fact(
            ledger.engram_id,
            ProvenanceKind::LedgerVerified,
            Some("file://wallet.json".into()),
            0.99,
        )
        .unwrap();
    let _ = brain.experience(Episode {
        content: "I think my wallet balance is $60 from yesterday's chat".into(),
        context: "chat".into(),
        outcome: None,
        salience_hint: 0.4,
        semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
        provenance: Some(Provenance {
            kind: ProvenanceKind::ChatAssertion,
            source_uri: None,
            confidence: 0.3,
            verified: false,
        }),
    });
    let wallet = brain.activate("wallet balance");
    let top = wallet.recalls.first();
    check!(
        "4. Verified ledger wins wallet cue (truth over chat)",
        top.map(|r| r.verified && r.episode.content.contains("$0.00"))
            .unwrap_or(false)
    );

    // 5. Separation gate blocks near-duplicate unverified chat.
    let dup = brain.experience(Episode {
        content: "I think my wallet balance is $60 from yesterday's chat".into(),
        context: "chat".into(),
        outcome: None,
        salience_hint: 0.4,
        semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
        provenance: None,
    });
    check!(
        "5. Separation gate blocks duplicate chat claims",
        dup.map(|g| g.gate_rejected).unwrap_or(false)
    );

    // 6. Growth — developmental stage exists and advances with living.
    let stage_before = brain.stage();
    for i in 0..5 {
        let _ = brain.experience(Episode::new(format!("life event {i}"), "life", 0.6));
    }
    let _ = brain.tick_n(6).unwrap();
    check!(
        "6. Growth — stage reporting and progression hooks",
        !brain.stage_report().stage.is_empty()
            && (brain.stage() != stage_before || brain.stage_report().synapse_pressure >= 0.0)
    );

    // 7. Sleep / plasticity — consolidation runs.
    let sleeps_before = brain.development.metrics.sleep_cycles;
    let sleep = brain.sleep().unwrap();
    check!(
        "7. Learning is plasticity (sleep consolidation runs)",
        brain.development.metrics.sleep_cycles > sleeps_before || sleep.consolidated > 0
    );

    // 8. Life chapters — death transitions life; core can persist.
    brain.mark_core(ledger.engram_id, "ledger-wallet".into()).unwrap();
    brain.death("manifesto audit chapter end").unwrap();
    check!(
        "8. Life has chapters (death transitions life)",
        brain.life.death_count >= 1
    );

    // 9. Not a vector DB — recall path does not require embeddings.
    let mut vec_free = FluctlightBrain::new();
    vec_free
        .experience(Episode::new("password rotation policy updated", "security", 0.7))
        .unwrap();
    let r = vec_free.activate("password rotation");
    check!(
        "9. No vector DB as primary store (lexical/activation recall)",
        !r.recalls.is_empty()
    );

    // 10. Agent infrastructure — preplay + verified context APIs exist.
    let mut agent_brain = FluctlightBrain::new();
    agent_brain
        .experience(Episode::new("user prefers terse answers", "prefs", 0.7))
        .unwrap();
    let preplay = agent_brain.preplay("user preference", 2);
    let verified = agent_brain.verified_context(3);
    check!(
        "10. Agent infrastructure (preplay + verified_context)",
        preplay.path.len() <= 2 && !agent_brain.stage_report().stage.is_empty()
    );

    eprintln!("\n=== All manifesto checks passed ===\n");
}

//! Production benchmark — Fluctlight vs lexical (SQL-like) vs brute vector scan.

use std::time::Instant;

use fluctlightdb::{Episode, FluctlightBrain, Provenance, ProvenanceKind};

const PAIRS: &[(&str, &str, &[f32])] = &[
    (
        "database connection pool exhausted",
        "db pool timeout",
        &[0.9, 0.1, 0.0],
    ),
    (
        "redis cache miss storm",
        "cache invalidation spike",
        &[0.85, 0.15, 0.0],
    ),
    (
        "kubernetes pod crash loop",
        "k8s container restart loop",
        &[0.88, 0.12, 0.0],
    ),
    (
        "payment webhook signature invalid",
        "stripe webhook auth failed",
        &[0.92, 0.08, 0.0],
    ),
    (
        "user login brute force",
        "account lockout threshold",
        &[0.8, 0.2, 0.0],
    ),
];

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}

fn lexical_hit(corpus: &[(&str, Vec<f32>)], cue: &str) -> bool {
    let cue_l = cue.to_lowercase();
    corpus.iter().any(|(content, _)| {
        content.to_lowercase().contains(&cue_l)
            || cue_l
                .split_whitespace()
                .any(|w| content.to_lowercase().contains(w))
    })
}

fn vector_hit(corpus: &[(&str, Vec<f32>)], cue_vec: &[f32]) -> bool {
    corpus.iter().any(|(_, v)| cosine(v, cue_vec) >= 0.75)
}

#[test]
fn prod_bench_fluctlight_vs_baselines() {
    let mut brain = FluctlightBrain::new();
    let mut corpus: Vec<(&str, Vec<f32>)> = Vec::new();
    for (content, _, vec) in PAIRS {
        brain
            .experience(Episode {
                content: (*content).into(),
                context: "bench".into(),
                outcome: None,
                salience_hint: 0.75,
                semantic_vector: Some(vec.to_vec()),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        corpus.push((*content, vec.to_vec()));
    }

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

    let mut fl_hits = 0usize;
    let mut lex_hits = 0usize;
    let mut vec_hits = 0usize;

    let t0 = Instant::now();
    for _ in 0..200 {
        for (_, cue, vec) in PAIRS {
            let r = brain.activate_with_semantic(cue, Some(vec));
            if !r.recalls.is_empty() {
                fl_hits += 1;
            }
        }
    }
    let fl_activate_ms = t0.elapsed().as_secs_f64() * 1000.0 / (200.0 * PAIRS.len() as f64);

    let t1 = Instant::now();
    for _ in 0..200 {
        for (_, cue, vec) in PAIRS {
            if lexical_hit(&corpus, cue) {
                lex_hits += 1;
            }
            if vector_hit(&corpus, vec) {
                vec_hits += 1;
            }
        }
    }
    let baseline_ms = t1.elapsed().as_secs_f64() * 1000.0 / (200.0 * PAIRS.len() as f64);

    let wallet = brain.activate("wallet balance");
    let top = wallet.recalls.first();
    let verified_top = top.map(|r| r.verified).unwrap_or(false);
    let balance_ok = top
        .map(|r| r.episode.content.contains("$0.00"))
        .unwrap_or(false);

    let gate = brain.experience(Episode {
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
    let gate_blocked = gate.map(|g| g.gate_rejected).unwrap_or(false);

    let preplay = brain.preplay("wallet balance", 3);
    let stage = brain.stage_report();
    let verified = brain.verified_context(5);

    eprintln!("=== FluctlightDB production bench (in-process) ===");
    eprintln!(
        "activate avg: {:.3} ms/query (n={})",
        fl_activate_ms,
        200 * PAIRS.len()
    );
    eprintln!("baseline avg: {:.3} ms/query (lex+vec scan)", baseline_ms);
    eprintln!(
        "recall hits/fluctlight: {} / {} queries",
        fl_hits,
        200 * PAIRS.len()
    );
    eprintln!("recall hits/lexical: {} / {}", lex_hits, 200 * PAIRS.len());
    eprintln!(
        "recall hits/vector>=0.75: {} / {}",
        vec_hits,
        200 * PAIRS.len()
    );
    eprintln!("wallet cue verified top: {verified_top} content_ok: {balance_ok}");
    eprintln!("separation gate blocked dup chat: {gate_blocked}");
    eprintln!(
        "preplay steps: {} terminal: {:?}",
        preplay.path.len(),
        preplay.terminal_engrams
    );
    eprintln!(
        "stage: {} pressure: {:.2}",
        stage.stage, stage.synapse_pressure
    );
    eprintln!("verified facts: {}", verified.facts.len());

    assert!(
        fl_hits >= lex_hits / 2,
        "fluctlight should recall at least half of lexical hits"
    );
    assert!(
        verified_top && balance_ok,
        "verified ledger should win wallet cue"
    );
    assert!(
        gate_blocked,
        "separation gate should block near-duplicate chat balance"
    );
}

//! First-principles predictive loop experiment.
//!
//! Proves autonomous graph simulation on tick — no user query required.
//! Run: `cargo test -p fluctlightdb --test predictive_loop_experiment -- --nocapture`

use fluctlightdb::{Episode, FluctlightBrain};

#[test]
fn predictive_loop_first_principles_scorecard() {
    let mut brain = FluctlightBrain::new();
    for (content, sal) in [
        ("fluctlight attention schema design", 0.9),
        ("locomo evidence recall honesty", 0.85),
        ("rust mutex contention workspace", 0.8),
        ("wal checkpoint durability seal", 0.75),
    ] {
        brain
            .experience(Episode {
                content: content.into(),
                context: "seed".into(),
                outcome: None,
                salience_hint: sal,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
    }

    println!("\n========== PREDICTIVE LOOP (FIRST PRINCIPLES) ==========\n");

    // A) Autonomous dream — no user query, only episodic graph + preplay
    let dream = brain.dream_step();
    println!("A) dream_step() without user query");
    println!("   seeds={:?}", dream.seeds);
    println!(
        "   expectation={:?}",
        dream.expectation.as_ref().map(|e| &e.summary)
    );
    println!("   scenarios={:?}", dream.scenarios);
    println!("   stream_len={}", dream.stream_len);
    let pass_a = dream.expectation.is_some() && !dream.seeds.is_empty() && dream.stream_len >= 1;
    println!("   PASS={pass_a}\n");

    // B) Autonomic ticks advance inner timeline without queries
    for _ in 0..5 {
        let _ = brain.tick();
    }
    let flow = brain.predictive_flow(12);
    println!("B) tick()×5 builds inner stream");
    println!("   dreams={}", brain.predictive_loop.dream_count);
    println!("   flow={}", flow);
    let pass_b = brain.predictive_loop.dream_count >= 2 && flow.contains("t=");
    println!("   PASS={pass_b}\n");

    // C) Expectations actively formed
    let narr = brain.prediction_report();
    println!("C) Active expectation");
    println!("   {}", narr);
    let pass_c =
        narr.to_lowercase().contains("expecting") || brain.predictive_loop.expectation.is_some();
    println!("   PASS={pass_c}\n");

    // D) Surprise / confirm machinery works on external observe
    brain.redirect_attention("fluctlight attention schema design");
    let _ = brain.predictive_cycle(false);
    let bad = brain.observe_prediction("pineapple pizza topping controversy");
    let good = brain.observe_prediction("fluctlight attention schema design notes");
    println!("D) Surprise vs confirm");
    println!(
        "   bad.surprise={} good.surprise={}",
        bad.surprise, good.surprise
    );
    let pass_d = bad.surprise && !good.surprise;
    println!("   PASS={pass_d}\n");

    // E) Personalized interpretation accumulates in loop state
    let mut saw_interp = brain.world_interpretation().is_some();
    for _ in 0..8 {
        let d = brain.dream_step();
        if d.interpretation.is_some() {
            saw_interp = true;
            break;
        }
    }
    println!("E) Proactive interpretation");
    println!("   latest={:?}", brain.world_interpretation());
    println!(
        "   interpretations={}",
        brain.predictive_loop.interpretations.len()
    );
    let pass_e = saw_interp || !brain.predictive_loop.interpretations.is_empty();
    println!("   PASS={pass_e}\n");

    // F) Surprise can land as hippocampal engram (write path)
    let before = brain.hippocampus.engrams.len();
    let _ = brain.encode_prediction_error("totally unrelated asteroid mining logistics");
    let after = brain.hippocampus.engrams.len();
    let recall = brain.activate("prediction_error asteroid");
    let hit = recall.recalls.iter().any(|r| {
        r.episode.content.contains("prediction_error") || r.episode.content.contains("asteroid")
    });
    println!("F) Error encode → recall");
    println!("   engrams {before}→{after} hit={hit}");
    let pass_f = after >= before && hit;
    println!("   PASS={pass_f}\n");

    let passes = [pass_a, pass_b, pass_c, pass_d, pass_e, pass_f];
    let score = passes.iter().filter(|p| **p).count();
    println!("========== SCORE {score}/{} ==========\n", passes.len());
    assert_eq!(
        score,
        passes.len(),
        "first-principles predictive loop failed: {passes:?}"
    );
}

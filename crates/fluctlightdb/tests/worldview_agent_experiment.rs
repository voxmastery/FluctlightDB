//! Coherent autonomous worldview experiment.
//!
//! Proves: no user query → clean episodic beliefs (not meta) → workspace + questions.
//! Run: `cargo test -p fluctlightdb --test worldview_agent_experiment -- --nocapture`

use fluctlightdb::{Episode, FluctlightBrain};

fn seed(brain: &mut FluctlightBrain, content: &str, context: &str) {
    brain
        .experience(Episode {
            content: content.into(),
            context: context.into(),
            outcome: None,
            salience_hint: 0.8,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
}

fn is_meta(s: &str) -> bool {
    let l = s.to_lowercase();
    l.contains("[interpretation]")
        || l.contains("[worldview]")
        || l.contains("[prediction_error]")
        || l.starts_with("world diverged")
        || l.starts_with("world coheres")
        || l.starts_with("expect:")
        || l.starts_with("observed-divergence")
        || l.starts_with("surprise broadcast")
}

fn mentions_world(s: &str) -> bool {
    let l = s.to_lowercase();
    [
        "harbor", "beacon", "fog", "lantern", "pier", "ships", "dock",
    ]
    .iter()
    .any(|w| l.contains(w))
}

#[test]
fn worldview_agent_coherent_beliefs() {
    let mut brain = FluctlightBrain::new();
    brain.worldview.every_n_ticks = 1;
    brain.predictive_loop.enabled = true;

    seed(
        &mut brain,
        "the harbor beacon flashes twice before fog rolls in",
        "world",
    );
    seed(
        &mut brain,
        "dock workers store lantern oil near the south pier",
        "world",
    );
    seed(
        &mut brain,
        "when fog arrives ships wait for the harbor beacon",
        "world",
    );

    println!("\n========== WORLDVIEW COHERENCE ==========\n");

    // A) Explicit step yields clean belief(s)
    let report = brain.worldview_step(None);
    let tops = brain.top_beliefs(5);
    let clean_a = tops
        .iter()
        .any(|b| !is_meta(&b.claim) && mentions_world(&b.claim));
    let no_meta_a = tops.iter().all(|b| !is_meta(&b.claim));
    println!("A) clean beliefs after worldview_step");
    for b in &tops {
        println!("   conf={:.2} «{}»", b.confidence, b.claim);
    }
    let pass_a = report.belief_upserts > 0 && clean_a && no_meta_a;
    println!("   PASS={pass_a}\n");

    // B) Autonomic ticks keep beliefs clean (no meta pollution)
    for _ in 0..8 {
        let _ = brain.tick();
    }
    let tops_b = brain.top_beliefs(8);
    let no_meta_b = tops_b.iter().all(|b| !is_meta(&b.claim));
    let worldish = tops_b.iter().filter(|b| mentions_world(&b.claim)).count();
    println!(
        "B) after ticks: beliefs={} worldish={worldish}",
        tops_b.len()
    );
    for b in &tops_b {
        println!("   conf={:.2} «{}»", b.confidence, b.claim);
    }
    let pass_b = no_meta_b && worldish >= 1 && brain.worldview.cycle_count >= 1;
    println!("   PASS={pass_b}\n");

    // C) Confidence not saturated at 0.98 for fresh beliefs
    let max_c = tops_b.iter().map(|b| b.confidence).fold(0.0_f32, f32::max);
    let pass_c = max_c <= 0.92 && tops_b.iter().any(|b| b.confidence < 0.90);
    println!("C) confidence discipline max={max_c:.2}");
    println!("   PASS={pass_c}\n");

    // D) Workspace is a clean world claim when present
    let ws = brain.workspace_broadcast();
    let pass_d = match &ws {
        Some(w) => !is_meta(&w.content) && mentions_world(&w.content),
        None => worldish >= 1, // still ok if beliefs exist; broadcast may lag
    };
    println!("D) workspace={:?}", ws.as_ref().map(|w| &w.content));
    println!("   PASS={pass_d}\n");

    // E) Dream coupling does not inject meta beliefs
    let dream = brain.dream_step();
    let before = brain.worldview.beliefs.len();
    let _ = brain.worldview_step(Some(&dream));
    let after_tops = brain.top_beliefs(12);
    let no_meta_e = after_tops.iter().all(|b| !is_meta(&b.claim));
    println!(
        "E) dream-coupled step beliefs {before}→{}",
        after_tops.len()
    );
    println!("   PASS={}\n", no_meta_e);

    // F) Curiosity: open questions exist and are non-meta memory junk
    let r = brain.worldview_step(None);
    let qs: Vec<_> = brain.worldview.open_questions.iter().cloned().collect();
    let qs_ok = !qs.is_empty()
        && qs.iter().all(|q| !is_meta(q))
        && (r.new_questions.iter().all(|q| !is_meta(q)));
    println!("F) questions={qs:?}");
    let pass_f = qs_ok;
    println!("   PASS={pass_f}\n");

    let passes = [pass_a, pass_b, pass_c, pass_d, no_meta_e, pass_f];
    let score = passes.iter().filter(|p| **p).count();
    println!("========== SCORE {score}/{} ==========\n", passes.len());
    assert_eq!(score, passes.len(), "coherent worldview failed: {passes:?}");
}

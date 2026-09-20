//! Standalone experiment: does Attention Schema *know* it is focusing?
//!
//! Run: `cargo test -p fluctlightdb --test attention_schema_experiment -- --nocapture`

use fluctlightdb::{Episode, FluctlightBrain};

fn scorecard() {
    let mut brain = FluctlightBrain::new();
    for (content, ctx, sal) in [
        ("quantum entanglement lab notes", "physics", 0.9),
        ("sourdough starter feeding schedule", "kitchen", 0.9),
        ("fluctlight attention schema design", "engine", 0.9),
        ("rust mutex contention global workspace", "engine", 0.8),
    ] {
        brain
            .experience(Episode {
                content: content.into(),
                context: ctx.into(),
                outcome: None,
                salience_hint: sal,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .expect("experience");
    }

    println!("\n========== ATTENTION SCHEMA EXPERIMENT ==========\n");

    // A) Retrieval without a schema model
    let plain = brain.activate("lab notes");
    let idle = brain.attention_report();
    println!("A) Plain activate (retrieve only)");
    println!(
        "   recalls={}  attention_field={:?}  schema.attending={}",
        plain.recalls.len(),
        plain.attention.is_some(),
        idle.attending
    );
    println!(
        "   top={:?}",
        plain.recalls.first().map(|r| &r.episode.content)
    );
    println!("   narration={}", idle.narration);
    let pass_a = !plain.recalls.is_empty() && plain.attention.is_none() && !idle.attending;
    println!("   PASS={pass_a}  (retrieve ≠ know focus)\n");

    // B) Know focus before any successful recall of that subject
    let focus = brain.redirect_attention("fluctlight attention schema design");
    println!("B) Redirect — model focus BEFORE using it to retrieve");
    println!("   attending={}", focus.attending);
    println!("   subject={}", focus.subject);
    println!(
        "   intensity={:.3} confidence={:.3} depth={:.3}",
        focus.intensity, focus.model_confidence, focus.depth
    );
    println!("   owner={:?}", focus.owner);
    println!("   narration={}", focus.narration);
    let pass_b = focus.attending
        && focus.subject.contains("attention schema")
        && focus.narration.to_lowercase().contains("attending")
        && focus.intensity > 0.5;
    println!("   PASS={pass_b}  (knows it is focusing)\n");

    // C) Retrieve + know simultaneously
    let attended = brain.activate_and_attend("schema design");
    let att = attended.attention.as_ref().expect("attention attached");
    println!("C) activate_and_attend — retrieve AND know");
    println!("   recalls={}", attended.recalls.len());
    println!(
        "   top={:?}",
        attended.recalls.first().map(|r| &r.episode.content)
    );
    println!("   attending={} subject={}", att.attending, att.subject);
    println!("   narration={}", att.narration);
    let pass_c = att.attending && !attended.recalls.is_empty() && !att.subject.is_empty();
    println!("   PASS={pass_c}\n");

    // D) Meta-model without a new cue
    let alone = brain.attention_report();
    println!("D) attention_report() with no new cue");
    println!(
        "   attending={} narration={}",
        alone.attending, alone.narration
    );
    let pass_d = alone.attending && alone.narration.to_lowercase().contains("attending");
    println!("   PASS={pass_d}  (model persists as self-focus knowledge)\n");

    // E) Control effect of the schema on ranking
    brain.redirect_attention("fluctlight attention schema design");
    let with_focus = brain.activate("notes design");
    let rank_focus = with_focus
        .recalls
        .iter()
        .position(|r| r.episode.content.contains("attention schema"));
    let act_focus = with_focus
        .recalls
        .iter()
        .find(|r| r.episode.content.contains("attention schema"))
        .map(|r| r.activation);

    brain.release_attention();
    let without = brain.activate("notes design");
    let rank_free = without
        .recalls
        .iter()
        .position(|r| r.episode.content.contains("attention schema"));
    let act_free = without
        .recalls
        .iter()
        .find(|r| r.episode.content.contains("attention schema"))
        .map(|r| r.activation);

    println!("E) Spotlight control on ambiguous cue \"notes design\"");
    println!("   rank_with_focus={rank_focus:?} activation={act_focus:?}");
    println!("   rank_without   ={rank_free:?} activation={act_free:?}");
    let pass_e = match (rank_focus, rank_free, act_focus, act_free) {
        (Some(f), Some(u), Some(af), Some(au)) => f <= u && af + 1e-5 >= au,
        (Some(_), None, _, _) => true,
        _ => false,
    };
    let boost_delta = match (act_focus, act_free) {
        (Some(af), Some(au)) => Some(af - au),
        _ => None,
    };
    println!("   boost_delta={boost_delta:?}");
    println!("   PASS={pass_e}  (schema biases focus, not only narrates)\n");

    // F) Release clears the model
    brain.release_attention();
    let cleared = brain.attention_report();
    println!("F) release_attention()");
    println!(
        "   attending={} narration={}",
        cleared.attending, cleared.narration
    );
    let pass_f = !cleared.attending;
    println!("   PASS={pass_f}\n");

    let passes = [pass_a, pass_b, pass_c, pass_d, pass_e, pass_f];
    let score = passes.iter().filter(|p| **p).count();
    println!("========== SCORE {score}/{} ==========\n", passes.len());
    assert_eq!(
        score,
        passes.len(),
        "attention schema experiment failed: {passes:?}"
    );
}

#[test]
fn attention_schema_experiment_scorecard() {
    scorecard();
}

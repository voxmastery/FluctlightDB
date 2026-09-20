//! Global Neural Broadcasting experiment — multi-receiver + singular now.
//!
//! Spec check: specialists + episodic graph clash → melt into present moment →
//! ignition broadcasts to attention, PFC, neuromod, graph, predictive, worldview.
//!
//! Run: `cargo test -p fluctlightdb --test global_workspace_experiment -- --nocapture`

use fluctlightdb::{Episode, FluctlightBrain, WorkspaceSource};

fn seed(brain: &mut FluctlightBrain, content: &str) {
    brain
        .experience(Episode {
            content: content.into(),
            context: "world".into(),
            outcome: None,
            salience_hint: 0.85,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
}

#[test]
fn global_neural_broadcasting_present_moment() {
    let mut brain = FluctlightBrain::new();
    brain.worldview.every_n_ticks = 1;
    brain.predictive_loop.enabled = true;
    brain.global_workspace.ignition_threshold = 0.40;

    seed(
        &mut brain,
        "the harbor beacon flashes twice before fog rolls in",
    );
    seed(
        &mut brain,
        "dock workers store lantern oil near the south pier",
    );
    seed(
        &mut brain,
        "when fog arrives ships wait for the harbor beacon",
    );
    brain
        .prefrontal
        .add_goal("keep ships safe in fog".into(), 0);

    let _ = brain.worldview_step(None);
    let _ = brain.worldview_step(None);

    println!("\n========== GLOBAL NEURAL BROADCASTING ==========\n");

    let ne_before = brain.neuromodulators.norepinephrine;
    let ach_before = brain.neuromodulators.acetylcholine;
    let syn_before = brain.graph.synapse_count(); // co_activate may not add synapses
    let pred_before = brain.predictive_loop.expectation.is_some();

    // A) Multi-source clash including Memory from activate()
    let report = brain.global_workspace_step(None);
    println!("A) candidates={}", report.candidates.len());
    let mut has_memory = false;
    let mut has_specialist = false;
    for c in &report.candidates {
        println!("   {:?} act={:.2} «{}»", c.source, c.activation, c.content);
        if c.source == WorkspaceSource::Memory {
            has_memory = true;
        }
        if matches!(
            c.source,
            WorkspaceSource::Worldview | WorkspaceSource::Goal | WorkspaceSource::Attention
        ) {
            has_specialist = true;
        }
    }
    let pass_a = report.candidates.len() >= 3 && (has_memory || has_specialist);
    println!("   PASS={pass_a}\n");

    // B) Singular present moment (melted now)
    let now = brain.present_moment();
    println!("B) now={:?}", now.as_ref().map(|n| &n.content));
    let pass_b = now
        .as_ref()
        .map(|n| {
            n.content.to_lowercase().contains("harbor") || n.content.to_lowercase().contains("fog")
        })
        .unwrap_or(false);
    println!("   PASS={pass_b}\n");

    // C) Ignition + multi-receiver broadcast
    let r = &report.receipt;
    println!(
        "C) ignited={} hits={} attn={} pfc={} nm={} graph={} pred={} wv={}",
        report.ignited,
        r.receivers_hit,
        r.attention,
        r.prefrontal,
        r.neuromod,
        r.graph,
        r.predictive,
        r.worldview
    );
    let pass_c = report.ignited
        && r.attention
        && r.prefrontal
        && r.neuromod
        && r.predictive
        && r.receivers_hit >= 4;
    println!("   PASS={pass_c}\n");

    // D) Systems actually changed (not silent)
    let attending = brain.attention_schema.attending;
    let pfc_ctx = brain.prefrontal.task_context.is_some();
    let ne_up = brain.neuromodulators.norepinephrine >= ne_before;
    let ach_up = brain.neuromodulators.acetylcholine >= ach_before;
    let pred_set = brain.predictive_loop.expectation.is_some();
    println!(
        "D) attending={attending} pfc={pfc_ctx} NE {ne_before:.2}→{:.2} ACh {ach_before:.2}→{:.2} pred={pred_before}→{pred_set} synapses={syn_before}",
        brain.neuromodulators.norepinephrine,
        brain.neuromodulators.acetylcholine
    );
    let pass_d = attending && pfc_ctx && (ne_up || ach_up) && pred_set;
    println!("   PASS={pass_d}\n");

    // E) Continuous now stream across ticks (unified present)
    for _ in 0..5 {
        let _ = brain.tick();
    }
    let stream = brain.now_stream(6);
    let stream_len = brain.global_workspace.now_stream.len();
    println!("E) stream_len={stream_len} flow={stream}");
    let pass_e = stream_len >= 3 && !stream.is_empty();
    println!("   PASS={pass_e}\n");

    // F) Graph buzz recorded (co_activate hit) and clashes accumulate
    let pass_f = report.receipt.graph
        || brain.global_workspace.clash_events >= 1
        || brain.global_workspace.merge_events >= 1;
    println!(
        "F) graph={} clashes={} merges={}",
        report.receipt.graph,
        brain.global_workspace.clash_events,
        brain.global_workspace.merge_events
    );
    println!("   PASS={pass_f}\n");

    let passes = [pass_a, pass_b, pass_c, pass_d, pass_e, pass_f];
    let score = passes.iter().filter(|p| **p).count();
    println!("========== SCORE {score}/{} ==========\n", passes.len());
    assert_eq!(
        score,
        passes.len(),
        "global neural broadcasting failed: {passes:?}"
    );
}

//! D1 end-to-end: recall must survive the neuron identity function changing underneath
//! a brain that is already on disk.
//!
//! `NeuronId`s are persisted (`graph.seg`, `hippocampus.seg`) *and* recomputed from token
//! text at query time. Before the codec was frozen these came from `DefaultHasher`, whose
//! algorithm std explicitly declines to guarantee across releases. When it changes, the two
//! sides stop agreeing: `cue_overlap` and `graph_boost` both go to zero, every candidate
//! falls under the `activation > 0.05` filter, and recall returns empty — with no error, no
//! crash and no log line, on a surface `docs/STABILITY.md` calls semver-stable.
//!
//! Simulating a std hash change directly is impossible, so this XORs every persisted
//! `NeuronId` on disk. That is a faithful stand-in: stored ids no longer equal recomputed
//! ids, and nothing else about the brain differs by a byte.

use std::path::Path;

use fluctlightdb::id::NeuronId;
use fluctlightdb::types::Episode;
use fluctlightdb::FluctlightBrain;

const DRIFT: u64 = 0xDEAD_BEEF_CAFE_F00D;

fn corrupt_persisted_neuron_ids(dir: &Path) {
    // Somnus/v4 checkpoints publish under generations/<CURRENT>/ — resolve that dir.
    let dir = fluctlightdb::manifest::active_generation_dir(dir).unwrap();
    // life.seg — the known-answer probes. In real drift these were written by the OLD hash
    // and the running binary recomputes them with the NEW one, so they disagree. Corrupting
    // them alongside the ids is what makes this simulation faithful: everything on disk came
    // from the same (now-superseded) identity function.
    let mut life = fluctlightdb::life::read_life_segment(&dir).unwrap();
    for p in &mut life.codec_probes {
        *p ^= DRIFT;
    }
    fluctlightdb::segment::write_segment(&dir, "life", &life).unwrap();

    // graph.seg — synapse endpoints and the region map.
    let mut graph: fluctlightdb::graph::BrainGraph =
        fluctlightdb::segment::read_segment(&dir, "graph").unwrap();
    for s in &mut graph.synapses {
        s.from = NeuronId(s.from.0 ^ DRIFT);
        s.to = NeuronId(s.to.0 ^ DRIFT);
    }
    graph.neuron_regions = graph
        .neuron_regions
        .into_iter()
        .map(|(n, r)| (NeuronId(n.0 ^ DRIFT), r))
        .collect();
    graph.rebuild_index();
    fluctlightdb::segment::write_segment(&dir, "graph", &graph).unwrap();

    // hippocampus.seg — every engram's three neuron sets.
    let mut hip: fluctlightdb::hippocampus::Hippocampus =
        fluctlightdb::segment::read_segment(&dir, "hippocampus").unwrap();
    for e in &mut hip.engrams {
        for set in [&mut e.neurons, &mut e.ec_neurons, &mut e.dg_neurons] {
            for n in set.iter_mut() {
                *n = NeuronId(n.0 ^ DRIFT);
            }
        }
    }
    fluctlightdb::segment::write_segment(&dir, "hippocampus", &hip).unwrap();
}

#[test]
fn recall_survives_neuron_hash_drift() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brain");
    let stored: u64;

    // Distinct subjects: near-identical text would be (correctly) turned away by the
    // separation gate, leaving nothing to re-key.
    let subjects = [
        "the payment gateway timed out during checkout",
        "nightly backup job exhausted its disk quota",
        "search index rebuild blocked on a stale lock",
        "customer export produced malformed csv rows",
        "auth tokens expired earlier than configured",
        "queue consumer lagged behind by four hours",
        "image resizer leaked file descriptors",
        "webhook retries hit an infinite redirect",
        "schema migration dropped a needed column",
        "cdn purge missed the eu-west region",
    ];
    let baseline = {
        let mut brain = FluctlightBrain::new();
        brain.attach_store_path(path.clone());
        for (i, subject) in subjects.iter().enumerate() {
            brain
                .experience(Episode::new(format!("incident {i}: {subject}"), "ops", 0.6))
                .unwrap();
        }
        brain.checkpoint().unwrap();
        stored = brain.hippocampus.engrams.len() as u64;
        let hits = brain.activate("payment gateway timeout");
        assert!(
            !hits.recalls.is_empty(),
            "precondition: recall works before drift"
        );
        hits.recalls[0].engram_id
    };

    // The identity function moves underneath the stored brain.
    corrupt_persisted_neuron_ids(&path);

    let mut brain = FluctlightBrain::open(&path).unwrap();
    assert!(
        brain.rekey_pending_count() > 0,
        "drift must be detected at load and queued for repair, not discovered by a user \
         noticing that recall went quiet"
    );

    // Without the repair, the pre-fix behaviour: nothing comes back.
    let degraded = brain.activate("payment gateway timeout");
    assert!(
        degraded.recalls.is_empty(),
        "sanity: with drifted ids and no re-key, recall is empty — this is the silent \
         failure the mechanism exists to catch"
    );

    let repaired = brain.rekey_now();
    assert_eq!(repaired, stored, "every stored engram should be re-keyed");
    assert_eq!(brain.rekey_pending_count(), 0);

    let after = brain.activate("payment gateway timeout");
    assert!(
        !after.recalls.is_empty(),
        "recall must be restored after re-key"
    );
    assert_eq!(
        after.recalls[0].engram_id, baseline,
        "the same engram should win the cue as before the drift"
    );
}

/// A brain written before the codec freeze recalls correctly as-is — its stored ids and its
/// recomputed cues agree, because both use the legacy hash. The migration is queued, not
/// forced, so upgrading the binary never degrades an existing brain.
#[test]
fn legacy_brain_recalls_correctly_before_being_rekeyed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brain");

    {
        let mut brain = FluctlightBrain::new();
        brain.attach_store_path(path.clone());
        // Pin it to the pre-freeze codec, as a brain written by an older binary would be.
        brain.life.neuron_codec = fluctlightdb::id::CODEC_LEGACY_STD;
        brain.life.codec_probes =
            fluctlightdb::life::codec_probes_for(fluctlightdb::id::CODEC_LEGACY_STD);
        for i in 0..10 {
            brain
                .experience(Episode::new(
                    format!("legacy note {i} about database connection pooling"),
                    "notes",
                    0.5,
                ))
                .unwrap();
        }
        brain.checkpoint().unwrap();
    }

    let mut brain = FluctlightBrain::open(&path).unwrap();
    assert_eq!(
        brain.status().neuron_codec,
        fluctlightdb::id::CODEC_LEGACY_STD
    );
    let before = brain.activate("database connection pooling");
    assert!(
        !before.recalls.is_empty(),
        "a legacy brain must keep recalling on the new binary — the upgrade is not allowed \
         to break brains at rest"
    );

    // Migration is available but not mandatory, and recall survives it.
    brain.rekey_now();
    assert_eq!(brain.status().neuron_codec, fluctlightdb::id::CURRENT_CODEC);
    let after = brain.activate("database connection pooling");
    assert!(
        !after.recalls.is_empty(),
        "recall must survive the migration too"
    );
}

/// The codec may only flip once the WHOLE re-key queue has drained.
///
/// Regression: `derive::drain` used to flip `life.neuron_codec` after ANY successful batch.
/// On a legacy production copy (12,917 engrams) one ingest drained 4, the shutdown checkpoint
/// persisted codec=FLCT1, and the reopened brain saw "current codec, no drift" — never
/// rebuilding the queue. 12,912 engrams became permanently unreachable, silently.
#[test]
fn partial_drain_must_not_flip_codec_and_reload_requeues() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brain");
    {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        for i in 0..12 {
            brain
                .experience(Episode {
                    content: format!("legacy fact number {i} about topic {i}"),
                    context: "drift".into(),
                    outcome: None,
                    salience_hint: 0.6,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .unwrap();
        }
        brain.checkpoint().unwrap();
    }
    // Force the brain into "legacy codec, full queue" the same way a real legacy load does.
    {
        let gen = fluctlightdb::manifest::active_generation_dir(&path).unwrap();
        let mut life = fluctlightdb::life::read_life_segment(&gen).unwrap();
        life.neuron_codec = 0; // CODEC_LEGACY_STD
        life.codec_probes = fluctlightdb::life::codec_probes_for(0);
        fluctlightdb::segment::write_segment(&gen, "life", &life).unwrap();
    }
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let queued = brain.rekey_pending_count();
    assert!(queued >= 12, "legacy load must queue everything: {queued}");

    // Partial drain: codec must NOT flip while anything is still pending.
    fluctlightdb::derive::drain(&mut brain, 4);
    assert!(brain.rekey_pending_count() > 0);
    assert_eq!(
        brain.life.neuron_codec, 0,
        "codec flipped with {} engrams still pending",
        brain.rekey_pending_count()
    );

    // A checkpoint + reload mid-migration must re-queue the remainder, not strand it.
    brain.checkpoint().unwrap();
    drop(brain);
    let brain = FluctlightBrain::open(&path).unwrap();
    assert!(
        brain.rekey_pending_count() > 0,
        "reload mid-migration lost the re-key queue"
    );

    // Full drain: now the flip happens.
    let mut brain = brain;
    brain.rekey_now();
    assert_eq!(brain.rekey_pending_count(), 0);
    assert_eq!(brain.life.neuron_codec, fluctlightdb::id::CURRENT_CODEC);
}

/// An engram written while migration is in flight must be re-keyed before the flip,
/// or it is stranded under a codec no cue will ever be derived with again.
#[test]
fn engram_written_mid_migration_is_not_stranded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brain");
    {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        for i in 0..8 {
            brain
                .experience(Episode {
                    content: format!("old memory {i} in the archive"),
                    context: "drift".into(),
                    outcome: None,
                    salience_hint: 0.6,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .unwrap();
        }
        brain.checkpoint().unwrap();
    }
    {
        let gen = fluctlightdb::manifest::active_generation_dir(&path).unwrap();
        let mut life = fluctlightdb::life::read_life_segment(&gen).unwrap();
        life.neuron_codec = 0;
        life.codec_probes = fluctlightdb::life::codec_probes_for(0);
        fluctlightdb::segment::write_segment(&gen, "life", &life).unwrap();
    }
    let mut brain = FluctlightBrain::open(&path).unwrap();
    assert!(brain.rekey_pending_count() > 0);

    // Write during migration, then drain to completion.
    brain
        .experience(Episode {
            content: "fresh migration window observation about zebras".into(),
            context: "drift".into(),
            outcome: None,
            salience_hint: 0.8,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    brain.rekey_now();
    assert_eq!(brain.life.neuron_codec, fluctlightdb::id::CURRENT_CODEC);

    let hits = brain.activate("zebras migration observation");
    assert!(
        hits.recalls
            .iter()
            .any(|r| r.episode.content.contains("zebras")),
        "mid-migration engram stranded after codec flip"
    );
}

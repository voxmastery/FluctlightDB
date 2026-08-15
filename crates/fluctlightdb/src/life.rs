use uuid::Uuid;

use serde::{Deserialize, Serialize};

/// One agent life — Return by Death namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifeState {
    pub life_id: Uuid,
    pub started_at_tick: u64,
    pub death_count: u32,
    pub alive: bool,
    /// Which neuron-identity codec this brain's persisted `NeuronId`s were derived under.
    ///
    /// `#[serde(default)]` yields [`crate::id::CODEC_LEGACY_STD`] (0), which is exactly right:
    /// a brain written before the codec freeze has no field, and 0 is the codec it used.
    /// New brains are born on [`crate::id::CURRENT_CODEC`]. Per-brain rather than global
    /// because `serve.rs` serves a pool of brains concurrently.
    #[serde(default)]
    pub neuron_codec: u8,
    /// Known-answer probes recorded under `neuron_codec` at write time, re-checked at load.
    /// A mismatch means the identity function moved underneath stored data.
    #[serde(default)]
    pub codec_probes: Vec<u64>,
}

/// Fixed token shapes used for drift probes. These mirror `dentate::expand_granules`,
/// so a probe mismatch means real cue derivation has moved too.
pub const CODEC_PROBE_TOKENS: &[&str] = &[
    "c:payment",
    "c:gateway",
    "x:ledger",
    "bg:c:payment_gateway",
    "sum@payment_gateway_timeout",
    "c:timeout",
    "o:retried",
    "c:invoice",
];

/// The pre-codec `LifeState` layout.
///
/// **bincode is not self-describing**, so `#[serde(default)]` does *not* rescue a missing
/// trailing field the way it does for JSON — deserializing an old four-field `life.seg`
/// into the current struct fails with "unexpected end of file" and takes the whole brain
/// with it. Every segment-shape change therefore needs an explicit legacy rung, which is
/// the same reason `legacy_hippocampus.rs` exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LifeStatePreCodec {
    life_id: Uuid,
    started_at_tick: u64,
    death_count: u32,
    alive: bool,
}

/// Read the `life` segment, tolerating brains written before the neuron codec was frozen.
///
/// A legacy brain is adopted at [`crate::id::CODEC_LEGACY_STD`] — which is the codec it
/// actually used — so its persisted neuron ids keep matching freshly derived cues and
/// recall is unaffected by the upgrade.
pub fn read_life_segment(dir: &std::path::Path) -> crate::error::Result<LifeState> {
    if let Ok(life) = crate::segment::read_segment::<LifeState>(dir, "life") {
        return Ok(life);
    }
    let legacy: LifeStatePreCodec = crate::segment::read_segment(dir, "life")?;
    Ok(LifeState {
        life_id: legacy.life_id,
        started_at_tick: legacy.started_at_tick,
        death_count: legacy.death_count,
        alive: legacy.alive,
        neuron_codec: crate::id::CODEC_LEGACY_STD,
        codec_probes: codec_probes_for(crate::id::CODEC_LEGACY_STD),
    })
}

/// Compute the probe vector for a codec — the same eight seeds, always in this order.
pub fn codec_probes_for(codec: u8) -> Vec<u64> {
    let nil = Uuid::nil().to_string();
    CODEC_PROBE_TOKENS
        .iter()
        .map(|t| crate::id::NeuronId::from_seeds_with(codec, &["dg", &nil, t, "0"]).0)
        .collect()
}

impl LifeState {
    pub fn birth(tick: u64) -> Self {
        Self {
            life_id: Uuid::new_v4(),
            started_at_tick: tick,
            death_count: 0,
            alive: true,
            neuron_codec: crate::id::CURRENT_CODEC,
            codec_probes: codec_probes_for(crate::id::CURRENT_CODEC),
        }
    }

    pub fn death(&mut self) {
        self.alive = false;
        self.death_count += 1;
    }

    pub fn respawn(&mut self, tick: u64) -> Uuid {
        self.life_id = Uuid::new_v4();
        self.started_at_tick = tick;
        self.alive = true;
        self.life_id
    }
}

/// Survives life reset — identity, values, hard-won lessons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreMemory {
    pub key: String,
    pub content: String,
    pub from_life: Uuid,
    pub engram_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoreMemoryStore {
    pub memories: Vec<CoreMemory>,
}

impl CoreMemoryStore {
    pub fn persist(&mut self, key: String, content: String, life: Uuid, engram_id: Option<Uuid>) {
        if let Some(m) = self.memories.iter_mut().find(|m| m.key == key) {
            m.content = content;
            m.from_life = life;
            m.engram_id = engram_id;
        } else {
            self.memories.push(CoreMemory {
                key,
                content,
                from_life: life,
                engram_id,
            });
        }
    }
}

#[cfg(test)]
mod codec_tests {
    use super::*;

    /// A brain written before the codec freeze has a four-field `life.seg`. Because bincode
    /// is not self-describing, reading it into the current six-field struct fails outright —
    /// `#[serde(default)]` does not backfill missing *trailing* bytes the way it does in a
    /// self-describing format. Without the legacy rung this took down the whole brain load
    /// with "unexpected end of file", not just the codec fields.
    #[test]
    fn legacy_life_segment_loads_as_legacy_codec() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let legacy = LifeStatePreCodec {
            life_id: Uuid::from_u128(7),
            started_at_tick: 42,
            death_count: 1,
            alive: true,
        };
        crate::segment::write_segment(base, "life", &legacy).unwrap();

        // The direct read must fail — this is what makes the fallback load-bearing rather
        // than decorative. If this ever starts succeeding, the format became self-describing
        // and the legacy rung can be revisited.
        assert!(
            crate::segment::read_segment::<LifeState>(base, "life").is_err(),
            "precondition: the new shape cannot read the old bytes"
        );

        let loaded = read_life_segment(base).expect("legacy life.seg must still load");
        assert_eq!(loaded.life_id, Uuid::from_u128(7));
        assert_eq!(loaded.started_at_tick, 42);
        assert_eq!(loaded.death_count, 1);
        assert!(loaded.alive);
        assert_eq!(
            loaded.neuron_codec,
            crate::id::CODEC_LEGACY_STD,
            "a pre-freeze brain must be adopted at the codec it was actually written with, \
             or its persisted neuron ids stop matching freshly derived cues"
        );
    }

    #[test]
    fn current_life_segment_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let life = LifeState::birth(3);
        crate::segment::write_segment(base, "life", &life).unwrap();
        let loaded = read_life_segment(base).unwrap();
        assert_eq!(loaded.neuron_codec, crate::id::CURRENT_CODEC);
        assert_eq!(
            loaded.codec_probes,
            codec_probes_for(crate::id::CURRENT_CODEC)
        );
    }

    /// Two brains on different codecs must coexist in one process without deriving each
    /// other's ids — this is why the codec is per-brain state and not a global.
    #[test]
    fn two_codecs_coexist_in_one_process() {
        let legacy = crate::dentate::cue_to_dg_neurons(
            "payment gateway timeout",
            Uuid::nil(),
            crate::id::CODEC_LEGACY_STD,
        );
        let frozen = crate::dentate::cue_to_dg_neurons(
            "payment gateway timeout",
            Uuid::nil(),
            crate::id::CODEC_FLCT1,
        );
        assert_eq!(legacy.len(), frozen.len());
        assert_ne!(legacy, frozen, "codecs must produce distinct neuron sets");
        // Re-deriving each is stable and unaffected by the other having run.
        assert_eq!(
            legacy,
            crate::dentate::cue_to_dg_neurons(
                "payment gateway timeout",
                Uuid::nil(),
                crate::id::CODEC_LEGACY_STD
            )
        );
    }
}

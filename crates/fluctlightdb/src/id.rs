//! Neuron identity and the codec that produces it.
//!
//! # Why a codec, and why it is frozen
//!
//! A [`NeuronId`] is the hash of a semantic token. Those ids are **written to disk** (in the
//! `graph` and `hippocampus` segments) *and* **re-derived from token text at query time**
//! (`dentate::cue_to_dg_neurons`). Recall only works while both sides agree.
//!
//! The original implementation used `std::collections::hash_map::DefaultHasher`, whose
//! algorithm the standard library explicitly declines to guarantee across releases. A future
//! Rust upgrade would therefore change every recomputed id while the persisted ones stayed
//! put — `cue_overlap` drops to 0, `graph_boost` drops to 0, and the `activation > 0.05`
//! filter empties the result. Total, silent recall loss on brains at rest, with no error and
//! no crash, on a surface `docs/STABILITY.md` lists as semver-stable.
//!
//! [`CODEC_FLCT1`] is the fix: a hash this repository owns and pins with golden vectors, so
//! it cannot drift underneath stored data. The codec is recorded **per brain** on
//! `LifeState`, never as a process global — `serve.rs` keeps a pool of many brains served
//! concurrently by a thread per connection, and a global would let a legacy-pinned tenant
//! and a migrated one compute each other's ids mid-request.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Pre-freeze identity: `std::collections::hash_map::DefaultHasher`. Unstable across Rust
/// releases — retained only so brains written before the freeze still recall correctly
/// until they are re-keyed.
pub const CODEC_LEGACY_STD: u8 = 0;

/// Frozen identity: FNV-1a-64 over length-prefixed parts, finalized with the MurmurHash3
/// `fmix64` avalanche step. Pinned by `flct1_golden_vectors`.
pub const CODEC_FLCT1: u8 = 1;

/// The codec new brains are born with.
pub const CURRENT_CODEC: u8 = CODEC_FLCT1;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn fnv1a_byte(h: u64, b: u8) -> u64 {
    (h ^ b as u64).wrapping_mul(FNV_PRIME)
}

/// MurmurHash3 finalizer — FNV-1a alone has weak avalanche in the high bits, and these ids
/// are used directly as `HashMap` keys and in `from_pair` composition.
#[inline]
fn fmix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
    k ^= k >> 33;
    k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    k ^= k >> 33;
    k
}

/// FLCT1 over an ordered list of parts.
///
/// Each part is length-prefixed (8-byte little-endian) before its bytes, so
/// `["ab", "c"]` and `["a", "bc"]` cannot collide — the separator-free concatenation
/// ambiguity is what would otherwise let a token boundary shift alias two distinct engram
/// codes onto the same neuron.
fn flct1(parts: &[&str]) -> u64 {
    let mut h = FNV_OFFSET;
    for p in parts {
        for b in (p.len() as u64).to_le_bytes() {
            h = fnv1a_byte(h, b);
        }
        for &b in p.as_bytes() {
            h = fnv1a_byte(h, b);
        }
    }
    fmix64(h)
}

fn flct1_u64s(values: &[u64]) -> u64 {
    let mut h = FNV_OFFSET;
    for v in values {
        for b in v.to_le_bytes() {
            h = fnv1a_byte(h, b);
        }
    }
    fmix64(h)
}

/// Sparse neuron identity — hash of a semantic token, not a vector dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct NeuronId(pub u64);

impl NeuronId {
    /// Derive under an explicit codec. Callers hold a brain and pass `life.neuron_codec`.
    pub fn from_token_with(codec: u8, token: &str) -> Self {
        match codec {
            CODEC_FLCT1 => Self(flct1(&[token])),
            _ => Self::legacy_from_token(token),
        }
    }

    pub fn from_pair_with(codec: u8, a: NeuronId, b: NeuronId) -> Self {
        match codec {
            CODEC_FLCT1 => Self(flct1_u64s(&[a.0, b.0])),
            _ => Self::legacy_from_pair(a, b),
        }
    }

    pub fn from_seeds_with(codec: u8, parts: &[&str]) -> Self {
        match codec {
            CODEC_FLCT1 => Self(flct1(parts)),
            _ => Self::legacy_from_seeds(parts),
        }
    }

    /// Legacy (pre-freeze) derivations. Kept byte-for-byte so brains written before the
    /// freeze keep recalling until `derive::drain` re-keys them.
    fn legacy_from_token(token: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        token.hash(&mut hasher);
        Self(hasher.finish())
    }

    fn legacy_from_pair(a: NeuronId, b: NeuronId) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        a.0.hash(&mut hasher);
        b.0.hash(&mut hasher);
        Self(hasher.finish())
    }

    fn legacy_from_seeds(parts: &[&str]) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for p in parts {
            p.hash(&mut hasher);
        }
        Self(hasher.finish())
    }

    /// Derive under the current frozen codec.
    ///
    /// Use only where no brain is in scope (tests, ad-hoc tooling). Anything that touches
    /// persisted state must go through the `*_with` forms so a legacy brain keeps deriving
    /// legacy ids until it has been re-keyed.
    pub fn from_token(token: &str) -> Self {
        Self::from_token_with(CURRENT_CODEC, token)
    }

    pub fn from_pair(a: NeuronId, b: NeuronId) -> Self {
        Self::from_pair_with(CURRENT_CODEC, a, b)
    }

    pub fn from_seeds(parts: &[&str]) -> Self {
        Self::from_seeds_with(CURRENT_CODEC, parts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngramId(pub uuid::Uuid);

impl EngramId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for EngramId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The permanent guard `DefaultHasher` could never provide.
    ///
    /// These literals pin the on-disk identity function. If a change to `flct1` perturbs any
    /// derivation by one byte, this fails here rather than silently emptying recall on every
    /// brain in the field. The seed shapes mirror the real ones in `dentate.rs`.
    #[test]
    fn flct1_golden_vectors() {
        let nil = uuid::Uuid::nil().to_string();
        let cases: &[&[&str]] = &[
            &["ec", "c:payment"],
            &["dg", &nil, "c:payment", "0"],
            &["sep", &nil, "0", "7"],
            &["x:ledger"],
        ];
        let actual: Vec<u64> = cases
            .iter()
            .map(|parts| NeuronId::from_seeds_with(CODEC_FLCT1, parts).0)
            .collect();
        let expected = golden_expected();
        assert_eq!(
            actual, expected,
            "FLCT1 derivation changed — this silently breaks recall on every stored brain. \
             If the change is deliberate, bump to a new CODEC_* constant instead of editing \
             FLCT1 in place."
        );
    }

    fn golden_expected() -> Vec<u64> {
        vec![
            GOLDEN_EC_PAYMENT,
            GOLDEN_DG_NIL_PAYMENT_0,
            GOLDEN_SEP_NIL_0_7,
            GOLDEN_CTX_LEDGER,
        ]
    }

    /// Length-prefixing must make token-boundary shifts non-colliding.
    #[test]
    fn flct1_is_unambiguous_across_part_boundaries() {
        let a = NeuronId::from_seeds_with(CODEC_FLCT1, &["ab", "c"]);
        let b = NeuronId::from_seeds_with(CODEC_FLCT1, &["a", "bc"]);
        assert_ne!(
            a, b,
            "concatenation ambiguity would alias distinct engram codes"
        );
    }

    #[test]
    fn legacy_codec_is_preserved_exactly() {
        // The legacy path must keep producing DefaultHasher output verbatim, or brains
        // written before the freeze stop recalling the moment this ships.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        "c:payment".hash(&mut hasher);
        assert_eq!(
            NeuronId::from_token_with(CODEC_LEGACY_STD, "c:payment").0,
            hasher.finish()
        );
    }

    #[test]
    fn codecs_are_distinct() {
        assert_ne!(
            NeuronId::from_token_with(CODEC_LEGACY_STD, "c:payment"),
            NeuronId::from_token_with(CODEC_FLCT1, "c:payment"),
        );
    }

    #[test]
    fn flct1_avalanches() {
        // One-bit input change should move roughly half the output bits.
        let a = NeuronId::from_token_with(CODEC_FLCT1, "c:payment").0;
        let b = NeuronId::from_token_with(CODEC_FLCT1, "c:paymenu").0;
        let moved = (a ^ b).count_ones();
        assert!(
            (16..=48).contains(&moved),
            "weak avalanche: {moved} bits moved"
        );
    }

    include!("id_golden.rs");
}

// Frozen FLCT1 known-answer vectors.
//
// These are the on-disk identity contract. Do **not** regenerate them to make a failing
// test pass — a mismatch means the derivation moved, which silently breaks recall on every
// brain already written. If a derivation change is genuinely intended, add a new
// `CODEC_*` constant and migrate, leaving FLCT1 untouched.
//
// Shapes mirror the real call sites: `["ec", surface]` from `dentate::separate_episode`,
// `["dg", life_id, surface, granule]` from `dentate::expand_granules`, and
// `["sep", engram_id, attempt, tick]` from the separator loop.

pub(super) const GOLDEN_EC_PAYMENT: u64 = 0x68f4704cec3d64ef;
pub(super) const GOLDEN_DG_NIL_PAYMENT_0: u64 = 0xf4bdae098ecba029;
pub(super) const GOLDEN_SEP_NIL_0_7: u64 = 0x35163e6ac2f9958f;
pub(super) const GOLDEN_CTX_LEDGER: u64 = 0xdc6540aeba1e9db6;

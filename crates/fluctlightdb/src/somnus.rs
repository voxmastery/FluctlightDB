//! Somnus — CLS-native durability: wake traces vs autonomic / sleep systems seals.
//!
//! # Always on (no user toggle required)
//!
//! Somnus is **default ON**. Operators do not enable it. Set `FLUCTLIGHT_SOMNUS=0` only as a
//! temporary debug escape to restore legacy wake checkpoints (unsupported for production).
//!
//! # No quality tradeoff
//!
//! Autonomic Somnus seals call `systems_seal` only (immutable generation + prune obsolete
//! generation dirs). They do **not** run semantic `sleep_cycle` prune/crystallize, so
//! activate/CHORUS/benchmark ranking is unchanged by durability seals.
//!
//! # Crash-gap policy (resolves long-WAL tradeoff)
//!
//! Seals fire on the **earlier** of:
//! - `FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS` (default **120** ≈ 10 min at 5s stream ticks)
//! - `FLUCTLIGHT_SOMNUS_SEAL_EVERY_WAL` wake WAL records since last seal (default **48**)
//!
//! Busy agents seal sooner; idle agents still seal on the tick cadence. Disk stays bounded
//! by `FLUCTLIGHT_SOMNUS_KEEP`.
//!
//! See `docs/superpowers/specs/2026-07-25-somnus-cls-durability-doctrine.md`.

use std::env;

pub fn somnus_enabled() -> bool {
    match env::var("FLUCTLIGHT_SOMNUS") {
        Ok(value) => !(value == "0" || value.eq_ignore_ascii_case("false")),
        Err(_) => true,
    }
}

pub fn somnus_keep() -> usize {
    env::var("FLUCTLIGHT_SOMNUS_KEEP")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3)
        .max(1)
}

/// Tick cadence for autonomic durability seals (0 = disable tick trigger).
pub fn somnus_seal_every_ticks() -> u64 {
    env::var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(120)
}

/// WAL-record cadence since last seal (0 = disable WAL trigger). Default 48.
pub fn somnus_seal_every_wal_records() -> u64 {
    env::var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_WAL")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(48)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::EnvGuard;

    #[test]
    fn somnus_defaults_on_without_user_env() {
        let _guard = EnvGuard::acquire(&["FLUCTLIGHT_SOMNUS"]);
        env::remove_var("FLUCTLIGHT_SOMNUS");
        assert!(somnus_enabled());
    }

    #[test]
    fn somnus_can_disable_for_debug_only() {
        let _guard = EnvGuard::acquire(&["FLUCTLIGHT_SOMNUS"]);
        env::set_var("FLUCTLIGHT_SOMNUS", "0");
        assert!(!somnus_enabled());
    }

    #[test]
    fn keep_at_least_one() {
        let _guard = EnvGuard::acquire(&["FLUCTLIGHT_SOMNUS_KEEP"]);
        env::set_var("FLUCTLIGHT_SOMNUS_KEEP", "0");
        assert_eq!(somnus_keep(), 1);
    }

    #[test]
    fn seal_every_ticks_defaults_shorter_gap() {
        let _guard = EnvGuard::acquire(&["FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS"]);
        env::remove_var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS");
        assert_eq!(somnus_seal_every_ticks(), 120);
    }

    #[test]
    fn seal_every_wal_defaults() {
        let _guard = EnvGuard::acquire(&["FLUCTLIGHT_SOMNUS_SEAL_EVERY_WAL"]);
        env::remove_var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_WAL");
        assert_eq!(somnus_seal_every_wal_records(), 48);
    }
}

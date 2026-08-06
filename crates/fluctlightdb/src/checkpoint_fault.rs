//! Opt-in crash injection for checkpoint durability verification.
//!
//! Injection is inert unless both environment variables are set. It is intentionally
//! process-fatal so release tests exercise reopen behavior after an abrupt boundary failure.

pub const CHECKPOINT_CRASH_EXIT_CODE: i32 = 86;

pub const CHECKPOINT_FAULT_POINTS: &[&str] = &[
    "generation.before_file_write",
    "generation.after_file_write",
    "generation.after_file_fsync",
    "generation.after_file_rename",
    "generation.after_file_dir_fsync",
    "generation.before_rename",
    "generation.after_rename",
    "generations.after_dir_fsync",
    "current.before_write",
    "current.after_write",
    "current.after_fsync",
    "current.before_rename",
    "current.after_rename",
    "current.after_dir_fsync",
    "wal.before_delete",
    "wal.after_delete",
    "wal.after_dir_fsync",
];

pub(crate) fn hit(point: &str) {
    let enabled = std::env::var("FLUCTLIGHT_ENABLE_FAULT_INJECTION")
        .map(|value| value == "1")
        .unwrap_or(false);
    if enabled
        && std::env::var("FLUCTLIGHT_CHECKPOINT_CRASH_AT")
            .map(|configured| configured == point)
            .unwrap_or(false)
    {
        std::process::exit(CHECKPOINT_CRASH_EXIT_CODE);
    }
}

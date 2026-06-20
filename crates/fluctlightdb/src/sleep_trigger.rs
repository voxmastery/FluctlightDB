//! Why a sleep cycle ran — drives autonomic counter updates.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepTrigger {
    /// User/API `sleep()` — consolidation only, not autonomic.
    Manual,
    /// `tick()` autonomic interval or rate-limited window sleep.
    Autonomic,
    /// Synapse cap or pressure during `experience()`.
    Pressure,
}

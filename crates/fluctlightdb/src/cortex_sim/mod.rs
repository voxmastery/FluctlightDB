//! CORTEX deterministic simulation kernel (extreme-production correctness slice).
//!
//! Named `cortex_sim` so it does not collide with the neuroscience [`crate::cortex`] module.
//! Enable with `--features cortex-sim`.

mod clock;
mod fs;
mod net;
mod rng;
mod runtime;

pub use clock::CortexClock;
pub use fs::CortexFs;
pub use net::{CortexNet, NodeId};
pub use rng::CortexRng;
pub use runtime::{CortexRuntime, SimEvent, TraceHash};

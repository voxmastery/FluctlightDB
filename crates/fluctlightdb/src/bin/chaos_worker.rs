//! Subprocess target for chaos_jepsen SIGKILL test.
//! Writes experiences without checkpoint until killed.

use std::env;
use std::path::Path;
use std::thread;
use std::time::Duration;

use fluctlightdb::brain::FluctlightBrain;
use fluctlightdb::types::Episode;

fn main() {
    let path = env::args()
        .nth(1)
        .expect("usage: fluctlight-chaos-worker PATH [N]");
    let n: usize = env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(32);

    let mut brain = FluctlightBrain::open(Path::new(&path)).expect("open brain");
    for i in 0..n {
        brain
            .experience(Episode {
                content: format!("chaos worker write {i}"),
                context: "chaos-worker".into(),
                outcome: None,
                salience_hint: 0.55,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .expect("experience");
        thread::sleep(Duration::from_millis(5));
    }
    brain.checkpoint().expect("checkpoint");
}

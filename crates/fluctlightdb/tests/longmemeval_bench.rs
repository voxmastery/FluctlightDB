//! LongMemEval-style associative recall benchmark (fixture pairs).

use fluctlightdb::{Episode, FluctlightBrain};

const PAIRS: &[(&str, &str, &[f32])] = &[
    ("database connection pool exhausted", "db pool timeout", &[0.9, 0.1, 0.0]),
    ("redis cache miss storm", "cache invalidation spike", &[0.85, 0.15, 0.0]),
    ("kubernetes pod crash loop", "k8s container restart loop", &[0.88, 0.12, 0.0]),
    ("payment webhook signature invalid", "stripe webhook auth failed", &[0.92, 0.08, 0.0]),
    ("user login brute force", "account lockout threshold", &[0.8, 0.2, 0.0]),
    ("nginx upstream timeout", "reverse proxy gateway timeout", &[0.87, 0.13, 0.0]),
    ("postgres replication lag", "db replica delay high", &[0.91, 0.09, 0.0]),
    ("s3 upload multipart failure", "object storage upload aborted", &[0.86, 0.14, 0.0]),
    ("graphql query complexity limit", "api query cost exceeded", &[0.84, 0.16, 0.0]),
    ("mqtt broker disconnect storm", "iot broker connection drop", &[0.83, 0.17, 0.0]),
];

#[test]
fn longmemeval_style_recall_bar() {
    let mut brain = FluctlightBrain::new();
    for (content, _, vec) in PAIRS {
        brain
            .experience(Episode {
                content: (*content).into(),
                context: "longmem".into(),
                outcome: None,
                salience_hint: 0.75,
                semantic_vector: Some(vec.to_vec()),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
    }
    let mut hits = 0usize;
    for (_, cue, vec) in PAIRS {
        let r = brain.activate_with_semantic(cue, Some(vec));
        if !r.recalls.is_empty() {
            hits += 1;
        }
    }
    let rate = hits as f64 / PAIRS.len() as f64;
    eprintln!("LongMemEval-style fixture hit rate: {rate:.0}%");
    assert!(rate >= 0.7, "recall hit rate {rate} below 70% bar");
}

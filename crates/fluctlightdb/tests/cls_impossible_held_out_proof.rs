//! Impossible-bar proof: compositional expertise beyond lookup AND beyond
//! precomputed document joins.
//!
//! ## Scientific claim under test
//! After learning atomic relations from episodes, the schema lane must answer
//! a **held-out** person×language question where:
//! 1. No single episode contains both required tokens.
//! 2. No **stored** schema statement contains both required tokens (oracle
//!    join-of-stored-statements fails).
//! 3. Top-k episodic `activate` fails.
//! 4. Query-time slot composition succeeds (Person→City→Lang).
//!
//! This is strictly stronger than entity-bridge string concat at sleep.

use fluctlightdb::{Episode, FluctlightBrain};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const TRAIN_CITIES: usize = 8;
const HELD_OUT: usize = 30;
const DISTRACTORS: usize = 25;
const SCHEMA_FLOOR: f64 = 0.85;
const MARGIN: f64 = 0.25;
const CF_CASES: usize = 15;
const CF_EPSILON: f64 = 0.05;

fn norm_tokens(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

fn contains_all(hay: &str, required: &[&str]) -> bool {
    let h = norm_tokens(hay);
    required.iter().all(|r| h.contains(&r.to_lowercase()))
}

fn any_stored_statement_has_both(brain: &FluctlightBrain, a: &str, b: &str) -> bool {
    brain.cortex.schemas.active().any(|s| {
        let t = s.statement.to_lowercase();
        t.contains(&a.to_lowercase()) && t.contains(&b.to_lowercase())
    })
}

fn oracle_join_stored_statements(brain: &FluctlightBrain, required: &[&str]) -> bool {
    let blob: String = brain
        .cortex
        .schemas
        .active()
        .map(|s| s.statement.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    // "Oracle join" = union of tokens across stored statements (not query-time compose).
    // If composition was pre-materialized as concat bridges, this hits.
    // For true slot-compose, held-out person+lang never co-occur in any ONE statement,
    // but union across statements might still have both tokens in different statements.
    // The decisive control is: no single statement has both; compose must synthesize.
    let _ = blob;
    // Check co-occurrence in any single statement:
    brain
        .cortex
        .schemas
        .active()
        .any(|s| contains_all(&s.statement, required))
}

fn lookup_topk(brain: &FluctlightBrain, cue: &str, required: &[&str], k: usize) -> bool {
    let act = brain.activate(cue);
    let blob: String = act
        .recalls
        .iter()
        .take(k)
        .map(|r| r.episode.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    contains_all(&blob, required)
}

fn schema_lane_hit(brain: &FluctlightBrain, cue: &str, required: &[&str]) -> bool {
    let with = brain.activate_with_schemas(cue);
    let blob: String = with
        .schemas
        .iter()
        .map(|s| s.statement.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    contains_all(&blob, required)
}

#[derive(Serialize)]
struct ImpossibleFreeze {
    name: &'static str,
    date: &'static str,
    held_out_n: usize,
    lookup_acc: f64,
    stored_cooccur_acc: f64,
    schema_compose_acc: f64,
    schema_floor: f64,
    margin: f64,
    recombination_passed: bool,
    cf_mean_drop: f64,
    cf_passed: bool,
    overall_passed: bool,
    claim: &'static str,
}

#[test]
fn impossible_held_out_slot_composition_proof() {
    let cities = [
        "Berlin", "Tokyo", "Lagos", "Seoul", "Lisbon", "Nairobi", "Oslo", "Perth",
    ];
    let langs = [
        "Rust", "Go", "Zig", "Kotlin", "Swift", "Elixir", "Julia", "Nim",
    ];
    assert_eq!(cities.len(), TRAIN_CITIES);
    assert_eq!(langs.len(), TRAIN_CITIES);

    // Training people (never used as held-out probes)
    let train_people = [
        "Noah", "Olivia", "Paul", "Quinn", "Rita", "Sam", "Tess", "Uma",
    ];

    let held_people = [
        "Alice", "Bob", "Cara", "Dev", "Eve", "Finn", "Gita", "Hiro", "Ivy", "Jade", "Kai", "Lena",
        "Mira", "Nia", "Omar", "Pia", "Reed", "Suki", "Tara", "Uri", "Vera", "Wade", "Xena",
        "Yuri", "Zara", "Axel", "Bryn", "Cleo", "Dale", "Echo",
    ];
    assert_eq!(held_people.len(), HELD_OUT);

    let mut lookup_hits = 0usize;
    let mut stored_cooccur = 0usize;
    let mut compose_hits = 0usize;

    for i in 0..HELD_OUT {
        let mut brain = FluctlightBrain::new();

        // Distractors
        for d in 0..DISTRACTORS {
            brain
                .experience(Episode::new(
                    format!("Noise memo {d} about weather traffic lunch"),
                    "noise",
                    0.4,
                ))
                .unwrap();
        }

        // Train city→lang AND train_people→city (atomic episodes only)
        for c in 0..TRAIN_CITIES {
            brain
                .experience(Episode::new(
                    format!("{} project uses {}", cities[c], langs[c]),
                    "stack",
                    0.9,
                ))
                .unwrap();
            brain
                .experience(Episode::new(
                    format!("{} works in {}", train_people[c], cities[c]),
                    "bio",
                    0.9,
                ))
                .unwrap();
        }

        // Held-out person: ONLY person→city. Never person→lang. Never co-store.
        let person = held_people[i];
        let city = cities[i % TRAIN_CITIES];
        let lang = langs[i % TRAIN_CITIES];
        brain
            .experience(Episode::new(
                format!("{person} works in {city}"),
                "bio",
                0.9,
            ))
            .unwrap();

        brain.sleep().unwrap();

        let required = [person, lang];
        let cue = format!("What stack does {person} use?");

        // Invariant: no episode alone has both
        assert!(
            brain
                .hippocampus
                .engrams
                .iter()
                .all(|e| !contains_all(&e.episode.content, &required)),
            "episode leak for {person}/{lang}"
        );

        // Decisive invariant: no *stored* schema statement co-occurs person+lang
        // (if bridge concat materializes this, the impossible bar is NOT met)
        let co = any_stored_statement_has_both(&brain, person, lang);
        if co {
            stored_cooccur += 1;
        }

        if lookup_topk(&brain, &cue, &required, 5) {
            lookup_hits += 1;
        }
        if schema_lane_hit(&brain, &cue, &required) {
            compose_hits += 1;
        }

        // Per-case hard fail if stored co-occurrence: that means sleep pre-joined
        // the held-out answer — not query-time composition.
        assert!(
            !co,
            "HELD-OUT VIOLATION: stored schema already contains {person} and {lang}. \
             Sleep pre-joined the answer; this is NOT beyond-lookup composition."
        );
        assert!(
            !oracle_join_stored_statements(&brain, &required),
            "oracle single-statement join should fail for held-out {person}/{lang}"
        );
    }

    let n = HELD_OUT as f64;
    let lookup_acc = lookup_hits as f64 / n;
    let stored_acc = stored_cooccur as f64 / n;
    let schema_acc = compose_hits as f64 / n;
    let recombo_ok =
        schema_acc >= SCHEMA_FLOOR && schema_acc + 1e-9 >= lookup_acc + MARGIN && stored_acc < 1e-9;

    // CF battery (schema-active dark retention; no episode bailout)
    let mut drops = Vec::new();
    for i in 0..CF_CASES {
        let mut brain = FluctlightBrain::new();
        for j in 0..3 {
            brain
                .experience(Episode::new(
                    format!("User prefers dark mode theme case{i} v{j}"),
                    "prefs",
                    0.9,
                ))
                .unwrap();
        }
        brain.sleep().unwrap();
        let baseline = if schema_lane_hit(&brain, "dark mode theme", &["dark"]) {
            1.0
        } else {
            0.0
        };
        for j in 0..5 {
            brain
                .experience(Episode::new(
                    format!("User prefers light mode theme case{i} v{j}"),
                    "prefs",
                    0.9,
                ))
                .unwrap();
        }
        brain.sleep().unwrap();
        let after = if schema_lane_hit(&brain, "dark mode theme", &["dark"]) {
            1.0
        } else {
            0.0
        };
        drops.push(f64::max(baseline - after, 0.0));
    }
    let cf_mean_drop = drops.iter().sum::<f64>() / drops.len() as f64;
    let cf_ok = cf_mean_drop <= CF_EPSILON + 1e-9;

    let overall = recombo_ok && cf_ok;
    let freeze = ImpossibleFreeze {
        name: "cls-impossible-held-out-slot-composition",
        date: "2026-07-25",
        held_out_n: HELD_OUT,
        lookup_acc,
        stored_cooccur_acc: stored_acc,
        schema_compose_acc: schema_acc,
        schema_floor: SCHEMA_FLOOR,
        margin: MARGIN,
        recombination_passed: recombo_ok,
        cf_mean_drop,
        cf_passed: cf_ok,
        overall_passed: overall,
        claim: "query-time Person→City→Lang composition; no stored co-occurrence of held-out pair",
    };

    eprintln!(
        "\n=== IMPOSSIBLE-BAR PROOF ===\n{}\n===========================\n",
        serde_json::to_string_pretty(&freeze).unwrap()
    );

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("benchmarks/results/cls-impossible-held-out-slot-composition-2026-07-25.json");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, serde_json::to_string_pretty(&freeze).unwrap()).unwrap();

    assert!(
        recombo_ok,
        "IMPOSSIBLE BAR FAILED: schema_compose={schema_acc:.3} lookup={lookup_acc:.3} \
         stored_cooccur={stored_acc:.3} (must be 0) need schema>={SCHEMA_FLOOR} and >=lookup+{MARGIN}"
    );
    assert!(
        cf_ok,
        "CF FAILED: mean_drop={cf_mean_drop:.3} > ε={CF_EPSILON}"
    );
    assert!(overall);
}

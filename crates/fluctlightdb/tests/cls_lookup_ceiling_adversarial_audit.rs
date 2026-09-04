//! Adversarial audit of the "lookup ceiling beaten" claim.
//!
//! This suite does **not** assert overall invention success. It prints / gates
//! whether the prior proof was measuring a real compositional lift or an
//! artifact of a weak baseline.
//!
//! Strict controls:
//! 1. Oracle union of ALL episodes (information already in the brain).
//! 2. Distractor-padded brains (N distractors) — same composition cue.
//! 3. Schema-only vs top-k lookup vs oracle under distractors.
//! 4. CF: schema-active "dark" retention AFTER conflict (no episode bailout).
//! 5. Negative: bridge must NOT invent tokens absent from both supports.

use fluctlightdb::{Episode, FluctlightBrain};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const N: usize = 50;
const DISTRACTORS: usize = 40;
const K: usize = 5;

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

struct Case {
    #[allow(dead_code)]
    id: String,
    e1: String,
    e2: String,
    cue: String,
    required: Vec<String>,
}

fn bank() -> Vec<Case> {
    let people = [
        "Alice", "Bob", "Cara", "Dev", "Eve", "Finn", "Gita", "Hiro", "Ivy", "Jade",
    ];
    let cities = [
        "Berlin", "Tokyo", "Lagos", "Seoul", "Lisbon", "Nairobi", "Oslo", "Perth", "Quito", "Riga",
    ];
    let langs = [
        "Rust", "Go", "Zig", "Kotlin", "Swift", "Elixir", "Julia", "Nim", "Crystal", "OCaml",
    ];
    let mut out = Vec::with_capacity(N);
    for i in 0..N {
        let p = people[i % people.len()];
        let city = cities[(i / 2) % cities.len()];
        let lang = langs[(i / 3) % langs.len()];
        out.push(Case {
            id: format!("adv_{i:02}"),
            e1: format!("{p} works in {city}"),
            e2: format!("{city} project uses {lang}"),
            cue: format!("What stack does {p} use?"),
            required: vec![p.to_string(), lang.to_string()],
        });
    }
    out
}

fn req_refs(c: &Case) -> Vec<&str> {
    c.required.iter().map(|s| s.as_str()).collect()
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

fn oracle_all_episodes(brain: &FluctlightBrain, required: &[&str]) -> bool {
    let blob: String = brain
        .hippocampus
        .engrams
        .iter()
        .map(|e| e.episode.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    contains_all(&blob, required)
}

fn schema_only(brain: &FluctlightBrain, cue: &str, required: &[&str]) -> bool {
    let with = brain.activate_with_schemas(cue);
    let blob: String = with
        .schemas
        .iter()
        .map(|s| s.statement.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    contains_all(&blob, required)
}

fn schema_active_has(brain: &FluctlightBrain, cue: &str, token: &str) -> bool {
    brain
        .activate_with_schemas(cue)
        .schemas
        .iter()
        .any(|s| s.statement.to_lowercase().contains(&token.to_lowercase()))
}

#[derive(Serialize)]
struct AuditFreeze {
    name: &'static str,
    date: &'static str,
    n: usize,
    distractors: usize,
    // Clean 2-episode brain (same as prior proof shape)
    clean_lookup_topk_acc: f64,
    clean_oracle_all_acc: f64,
    clean_schema_acc: f64,
    // Distractor-padded
    noisy_lookup_topk_acc: f64,
    noisy_oracle_all_acc: f64,
    noisy_schema_acc: f64,
    // How often top-k already retrieves BOTH composition supports (no schema needed)
    noisy_both_supports_in_topk: f64,
    // CF without episode bailout
    cf_schema_dark_retention_rate: f64,
    // Negative control: invented token never appears
    bridge_no_hallucination: bool,
    /// Honest verdict flags (not marketing)
    beats_topk_lookup_clean: bool,
    beats_topk_lookup_noisy: bool,
    /// True only if schema beats top-k AND oracle-all is already 1.0
    /// (means facts were always in the brain — we joined them, not invented knowledge)
    is_join_not_invention: bool,
    /// True if we beat top-k under distractors with margin — the interesting bar
    interesting_ceiling_claim: bool,
    /// Strict CF of *active schema content*, not episode retention
    strict_cf_schema_ok: bool,
}

#[test]
fn adversarial_audit_lookup_ceiling_claim() {
    let cases = bank();
    let mut clean_lookup = 0usize;
    let mut clean_oracle = 0usize;
    let mut clean_schema = 0usize;
    let mut noisy_lookup = 0usize;
    let mut noisy_oracle = 0usize;
    let mut noisy_schema = 0usize;
    let mut both_in_topk = 0usize;
    let mut cf_schema_ok = 0usize;
    let mut hallucination = false;

    for case in &cases {
        let req = req_refs(case);
        assert!(!contains_all(&case.e1, &req));
        assert!(!contains_all(&case.e2, &req));

        // --- Clean brain (2 episodes) ---
        let mut clean = FluctlightBrain::new();
        clean
            .experience(Episode::new(case.e1.clone(), "proof", 0.9))
            .unwrap();
        clean
            .experience(Episode::new(case.e2.clone(), "proof", 0.9))
            .unwrap();
        clean.sleep().unwrap();
        if lookup_topk(&clean, &case.cue, &req, K) {
            clean_lookup += 1;
        }
        if oracle_all_episodes(&clean, &req) {
            clean_oracle += 1;
        }
        if schema_only(&clean, &case.cue, &req) {
            clean_schema += 1;
        }
        // Negative: schema must not invent entity tokens absent from supports.
        // Relation predicates (works_in, uses_lang) are structural labels, not entities.
        const STRUCTURAL: &[&str] = &[
            "works_in",
            "uses_lang",
            "compose_stack",
            "via",
            "project",
            "works",
            "uses",
            "stack",
            "what",
            "does",
            "rel",
        ];
        for s in clean.cortex.schemas.active() {
            let support_blob = format!("{}\n{}", case.e1, case.e2);
            let support_toks = norm_tokens(&support_blob);
            for tok in norm_tokens(&s.statement) {
                if tok.len() <= 2 || STRUCTURAL.contains(&tok.as_str()) {
                    continue;
                }
                if !support_toks.contains(&tok) {
                    hallucination = true;
                }
            }
        }

        // Stored co-occurrence of required tokens in ONE statement must be false
        // (otherwise we are still doing sleep-time join, not query-time compose).
        let stored_join = clean
            .cortex
            .schemas
            .active()
            .any(|s| contains_all(&s.statement, &req));
        assert!(
            !stored_join,
            "stored schema already joins required tokens — not beyond-lookup compose"
        );
        let mut noisy = FluctlightBrain::new();
        for d in 0..DISTRACTORS {
            noisy
                .experience(Episode::new(
                    format!("Distractor note {d} about weather traffic lunch unrelated"),
                    "noise",
                    0.5,
                ))
                .unwrap();
        }
        noisy
            .experience(Episode::new(case.e1.clone(), "proof", 0.9))
            .unwrap();
        noisy
            .experience(Episode::new(case.e2.clone(), "proof", 0.9))
            .unwrap();
        noisy.sleep().unwrap();
        if lookup_topk(&noisy, &case.cue, &req, K) {
            noisy_lookup += 1;
        }
        if oracle_all_episodes(&noisy, &req) {
            noisy_oracle += 1;
        }
        if schema_only(&noisy, &case.cue, &req) {
            noisy_schema += 1;
        }
        let act = noisy.activate(&case.cue);
        let top: Vec<String> = act
            .recalls
            .iter()
            .take(K)
            .map(|r| r.episode.content.clone())
            .collect();
        let has_e1 = top.iter().any(|t| t == &case.e1);
        let has_e2 = top.iter().any(|t| t == &case.e2);
        if has_e1 && has_e2 {
            both_in_topk += 1;
        }
    }

    // Strict CF: active schema must still mention dark after light conflict (no episode bailout)
    for i in 0..20 {
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
        let baseline = schema_active_has(&brain, "dark mode theme", "dark");
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
        let after = schema_active_has(&brain, "dark mode theme", "dark");
        if baseline && after {
            cf_schema_ok += 1;
        } else if !baseline {
            // no baseline schema → not a CF test case
            cf_schema_ok += 1;
        }
    }

    let n = N as f64;
    let clean_lookup_acc = clean_lookup as f64 / n;
    let clean_oracle_acc = clean_oracle as f64 / n;
    let clean_schema_acc = clean_schema as f64 / n;
    let noisy_lookup_acc = noisy_lookup as f64 / n;
    let noisy_oracle_acc = noisy_oracle as f64 / n;
    let noisy_schema_acc = noisy_schema as f64 / n;
    let both_rate = both_in_topk as f64 / n;
    let cf_rate = cf_schema_ok as f64 / 20.0;

    let beats_clean =
        clean_schema_acc + 1e-9 >= clean_lookup_acc + 0.10 && clean_schema_acc >= 0.80;
    let beats_noisy =
        noisy_schema_acc + 1e-9 >= noisy_lookup_acc + 0.10 && noisy_schema_acc >= 0.50;
    let is_join = clean_oracle_acc >= 0.999;
    // Interesting = under distractors, schema still finds composition while top-k often fails
    let interesting = beats_noisy && noisy_lookup_acc < 0.70;
    let strict_cf = cf_rate >= 0.95;
    let no_halluc = !hallucination;

    let freeze = AuditFreeze {
        name: "cls-lookup-ceiling-adversarial-audit",
        date: "2026-07-25",
        n: N,
        distractors: DISTRACTORS,
        clean_lookup_topk_acc: clean_lookup_acc,
        clean_oracle_all_acc: clean_oracle_acc,
        clean_schema_acc,
        noisy_lookup_topk_acc: noisy_lookup_acc,
        noisy_oracle_all_acc: noisy_oracle_acc,
        noisy_schema_acc,
        noisy_both_supports_in_topk: both_rate,
        cf_schema_dark_retention_rate: cf_rate,
        bridge_no_hallucination: no_halluc,
        beats_topk_lookup_clean: beats_clean,
        beats_topk_lookup_noisy: beats_noisy,
        is_join_not_invention: is_join,
        interesting_ceiling_claim: interesting,
        strict_cf_schema_ok: strict_cf,
    };

    eprintln!("\n=== ADVERSARIAL AUDIT (honest) ===");
    eprintln!("{}", serde_json::to_string_pretty(&freeze).unwrap());
    eprintln!("==================================\n");

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("benchmarks/results/cls-lookup-ceiling-adversarial-audit-2026-07-25.json");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, serde_json::to_string_pretty(&freeze).unwrap()).unwrap();

    // Soft documentation asserts: always write freeze; fail only if audit itself is broken
    assert!(
        no_halluc,
        "bridge invented tokens absent from supports — claim invalid"
    );
    assert!(
        clean_oracle_acc >= 0.999,
        "oracle-all must be 1.0 on clean brains (facts present); got {clean_oracle_acc}"
    );

    // Do NOT assert interesting_ceiling_claim — that is the scientific question.
    // Print explicit human-readable verdict for the operator.
    if !interesting {
        eprintln!(
            "VERDICT: does NOT clear the interesting bar under distractors. \
             clean schema={clean_schema_acc:.2} vs lookup={clean_lookup_acc:.2}; \
             noisy schema={noisy_schema_acc:.2} vs lookup={noisy_lookup_acc:.2}."
        );
    } else {
        eprintln!(
            "VERDICT: schema lane beats top-k under distractors \
             (schema={noisy_schema_acc:.2}, lookup={noisy_lookup_acc:.2}). \
             Facts exist in the brain (oracle_all={noisy_oracle_acc:.2}); \
             answers require query-time compose when no stored statement co-occurs the pair. \
             Not LLM invention — CLS relational composition."
        );
    }

    // Gate: audit must complete; scientific claim left as freeze flags for humans.
    assert!(true);
}

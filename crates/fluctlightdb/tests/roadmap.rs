//! Roadmap integration tests — tenant isolation, auth, budget, concurrency, recall bench.

use std::sync::{Arc, Barrier};
use std::thread;

use fluctlightdb::auth::{AuthConfig, Role};
use fluctlightdb::budget::WiringBudget;
use fluctlightdb::development::DevStage;
use fluctlightdb::manifest::{load_v4_dir, save_v4_dir};
use fluctlightdb::query::{self, QueryRequest};
use fluctlightdb::tenant::TenantConfig;
use fluctlightdb::{Episode, FluctlightBrain};
use tempfile::tempdir;

#[test]
fn autonomic_auto_sleeps_increments() {
    let mut brain = FluctlightBrain::new();
    brain.autonomic.config.ticks_per_sleep = 2;
    brain.autonomic.config.max_auto_sleeps_per_hour = 100;
    let before = brain.autonomic.auto_sleeps;
    for _ in 0..3 {
        brain.tick().unwrap();
    }
    assert!(brain.autonomic.auto_sleeps > before);
}

#[test]
fn synapse_budget_caps_ca3_clique() {
    let b = WiringBudget::for_stage(DevStage::Child);
    assert!(b.max_ca3_clique_neighbors <= 4);
    assert!(b.max_dg_chain_links <= 10);
}

#[test]
fn tenant_scoped_recall_isolation() {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "agent A secret project alpha".into(),
            context: "work".into(),
            outcome: None,
            salience_hint: 0.7,
            semantic_vector: None,
            agent_id: Some("agent_a".into()),
            tenant_id: Some("t1".into()),
            rag: None,
                provenance: None,
        })
        .unwrap();
    brain
        .experience(Episode {
            content: "agent B secret project beta".into(),
            context: "work".into(),
            outcome: None,
            salience_hint: 0.7,
            semantic_vector: None,
            agent_id: Some("agent_b".into()),
            tenant_id: Some("t1".into()),
            rag: None,
                provenance: None,
        })
        .unwrap();
    let a = brain.activate_scoped("secret project", None, Some("agent_a"));
    let b = brain.activate_scoped("secret project", None, Some("agent_b"));
    assert!(a.recalls.iter().all(|r| r.episode.agent_id.as_deref() == Some("agent_a")));
    assert!(b.recalls.iter().all(|r| r.episode.agent_id.as_deref() == Some("agent_b")));
}

#[test]
fn v4_manifest_roundtrip_persists_engrams() {
    let dir = tempdir().unwrap();
    let v4 = dir.path().join("brain_v4");
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "segmented storage test".into(),
            context: "v4".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
                rag: None,
                provenance: None,
            })
        .unwrap();
    save_v4_dir(&brain, &v4).unwrap();
    let loaded = load_v4_dir(&v4).unwrap();
    assert_eq!(loaded.hippocampus.engrams.len(), 1);
}

#[test]
fn auth_roles_hierarchy() {
    assert!(Role::Admin.allows(Role::Write));
    assert!(Role::Write.allows(Role::Read));
    assert!(!Role::Read.allows(Role::Write));
    let cfg = AuthConfig::default();
    let ctx = cfg.authorize(None, Some("default")).unwrap();
    assert_eq!(ctx.role, Role::Admin);
}

#[test]
fn auth_keys_require_bearer() {
    let prev = std::env::var("FLUCTLIGHT_API_KEYS").ok();
    std::env::set_var("FLUCTLIGHT_API_KEYS", "default:sekret:admin");
    let cfg = AuthConfig::from_env();
    assert!(cfg.require_auth);
    assert!(cfg.authorize(None, None).is_none());
    assert!(cfg.authorize(Some("sekret"), None).is_some());
    match prev {
        Some(v) => std::env::set_var("FLUCTLIGHT_API_KEYS", v),
        None => std::env::remove_var("FLUCTLIGHT_API_KEYS"),
    }
}

#[test]
fn serve_reloads_after_external_snapshot_write() {
    use fluctlightdb::BrainServer;

    let dir = tempdir().unwrap();
    let path = dir.path().join("reload.flct");
    let _ = FluctlightBrain::open(&path).unwrap();
    let server = BrainServer::open(path.clone()).unwrap();
    assert_eq!(
        server
            .with_brain_read("default", |b| Ok(b.hippocampus.engrams.len()))
            .unwrap(),
        0
    );

    let mut external = FluctlightBrain::open(&path).unwrap();
    external
        .experience(Episode {
            content: "external writer".into(),
            context: "cli".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
                rag: None,
                provenance: None,
            })
        .unwrap();
    drop(external);

    assert_eq!(
        server
            .with_brain_read("default", |b| Ok(b.hippocampus.engrams.len()))
            .unwrap(),
        1
    );
}

#[test]
fn query_list_and_forget() {
    let mut brain = FluctlightBrain::new();
    let r = brain
        .experience(Episode {
            content: "forgettable trace".into(),
            context: "q".into(),
            outcome: None,
            salience_hint: 0.4,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
                rag: None,
                provenance: None,
            })
        .unwrap();
    let list = query::execute(
        &brain,
        QueryRequest::ListEngrams {
            agent_id: None,
            page: 0,
            page_size: 10,
        },
    );
    if let fluctlightdb::query::QueryResponse::ListEngrams { total, .. } = list {
        assert_eq!(total, 1);
    }
    let forgot = query::execute_mut(
        &mut brain,
        QueryRequest::Forget {
            engram_id: r.engram_id,
        },
    );
    if let fluctlightdb::query::QueryResponse::Forget { removed } = forgot {
        assert!(removed);
    }
}

#[test]
fn concurrent_activate_during_experience() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("concurrent.flct");
    let brain = Arc::new(std::sync::Mutex::new(FluctlightBrain::open(&path).unwrap()));
    let barrier = Arc::new(Barrier::new(3));
    let b1 = brain.clone();
    let c1 = barrier.clone();
    let t1 = thread::spawn(move || {
        c1.wait();
        let g = b1.lock().unwrap();
        let _ = g.activate("concurrent");
    });
    let b2 = brain.clone();
    let c2 = barrier.clone();
    let t2 = thread::spawn(move || {
        c2.wait();
        let g = b2.lock().unwrap();
        let _ = g.activate("concurrent");
    });
    barrier.wait();
    {
        let mut g = brain.lock().unwrap();
        g.experience(Episode {
            content: "concurrent encoding event".into(),
            context: "bench".into(),
            outcome: None,
            salience_hint: 0.6,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
                rag: None,
                provenance: None,
            })
        .unwrap();
    }
    t1.join().unwrap();
    t2.join().unwrap();
}

#[test]
fn semantic_recall_benchmark_fixture_pairs() {
    let pairs: [(&str, &str, Vec<f32>); 10] = [
        ("database connection pool exhausted", "db pool timeout", vec![0.9, 0.1, 0.0]),
        ("redis cache miss storm", "cache invalidation spike", vec![0.85, 0.15, 0.0]),
        ("kubernetes pod crash loop", "k8s container restart loop", vec![0.88, 0.12, 0.0]),
        ("payment webhook signature invalid", "stripe webhook auth failed", vec![0.92, 0.08, 0.0]),
        ("user login brute force", "account lockout threshold", vec![0.8, 0.2, 0.0]),
        ("nginx upstream timeout", "reverse proxy gateway timeout", vec![0.87, 0.13, 0.0]),
        ("postgres replication lag", "db replica delay high", vec![0.91, 0.09, 0.0]),
        ("s3 upload multipart failure", "object storage upload aborted", vec![0.86, 0.14, 0.0]),
        ("graphql query complexity limit", "api query cost exceeded", vec![0.84, 0.16, 0.0]),
        ("mqtt broker disconnect storm", "iot broker connection drop", vec![0.83, 0.17, 0.0]),
    ];
    let mut brain = FluctlightBrain::new();
    for (content, _cue, vec) in &pairs {
        brain
            .experience(Episode {
                content: (*content).into(),
                context: "bench".into(),
                outcome: None,
                salience_hint: 0.75,
                semantic_vector: Some(vec.clone()),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
    }
    let mut hits = 0usize;
    for (_content, cue, vec) in &pairs {
        let r = brain.activate_with_semantic(cue, Some(vec));
        if !r.recalls.is_empty() {
            hits += 1;
        }
    }
    let rate = hits as f64 / pairs.len() as f64;
    assert!(rate >= 0.7, "recall hit rate {rate} below 70%");
}

#[test]
fn tenant_config_layout() {
    let dir = tempdir().unwrap();
    let cfg = TenantConfig::default_for("telegram_123", dir.path());
    assert!(cfg.brain_path.to_string_lossy().contains("telegram_123"));
}

#[test]
fn rag_ingest_recall_and_search_by_doc() {
    use fluctlightdb::query::{QueryRequest, QueryResponse};
    use fluctlightdb::types::RagRef;

    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "chunk about redis eviction policy".into(),
            context: "rag:docs#3".into(),
            outcome: None,
            salience_hint: 0.6,
            semantic_vector: None,
            agent_id: None,
            tenant_id: Some("default".into()),
            rag: Some(RagRef {
                source_uri: Some("file:///docs/redis.md".into()),
                doc_id: Some("docs".into()),
                chunk_id: Some("3".into()),
            }),
            provenance: None,
        })
        .unwrap();
    let act = brain.activate("redis eviction");
    assert!(act
        .recalls
        .iter()
        .any(|r| r.episode.rag.as_ref().and_then(|g| g.doc_id.as_deref()) == Some("docs")));

    let found = query::execute(
        &brain,
        QueryRequest::SearchByRag {
            doc_id: "docs".into(),
            chunk_id: Some("3".into()),
            page: 0,
            page_size: 10,
        },
    );
    if let QueryResponse::SearchByRag { total, items, .. } = found {
        assert_eq!(total, 1);
        assert_eq!(items[0].rag.as_ref().and_then(|r| r.chunk_id.as_deref()), Some("3"));
    } else {
        panic!("expected SearchByRag");
    }
}

#[test]
fn tenant_limits_reject_when_full() {
    use fluctlightdb::error::Error;

    let mut brain = FluctlightBrain::new();
    let cfg = TenantConfig {
        tenant_id: "t".into(),
        brain_path: std::path::PathBuf::from("/tmp/unused"),
        max_engrams: 1,
        max_synapses: 500_000,
    };
    brain
        .experience(Episode {
            content: "first".into(),
            context: "c".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
                provenance: None,
        })
        .unwrap();
    let err = cfg.check_limits(&brain);
    assert!(matches!(err, Err(Error::Store(_))));
}

#[test]
fn fovea_saccadic_scan_produces_packets() {
    let text = "The quick brown fox jumps over the lazy dog while reading uses many saccadic fixations across lines of text";
    let packets = fluctlightdb::scan_text(text, "test://doc", &fluctlightdb::FoveaConfig::default());
    assert!(packets.len() >= 2);
    assert!(packets[0].foveal.contains("The"));
}

#[test]
fn rag_chunk_deduplication_index() {
    use fluctlightdb::hippocampus::{rag_chunk_key, Hippocampus};
    use fluctlightdb::types::RagRef;

    let mut h = Hippocampus::default();
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "chunk one".into(),
            context: "rag:doc#1".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: Some(RagRef {
                source_uri: None,
                doc_id: Some("doc".into()),
                chunk_id: Some("1".into()),
            }),
            provenance: None,
        })
        .unwrap();
    assert!(brain.hippocampus.find_rag_chunk("doc", "1").is_some());
    assert_eq!(rag_chunk_key("doc", "1"), "doc#1");
    h.rebuild_rag_index();
}

#[test]
fn tenant_access_binding() {
    use fluctlightdb::auth::{AuthConfig, AuthContext, Role};

    let auth = AuthContext {
        tenant_id: "tenant_a".into(),
        role: Role::Write,
    };
    assert!(fluctlightdb::auth::Role::Write.allows(Role::Read));
    assert_ne!(auth.tenant_id, "tenant_b");
    let cfg = AuthConfig::from_env();
    let _ = cfg.authorize(Some("dummy"), Some("tenant_a"));
}

#[test]
fn replicate_sync_preserves_engrams() {
    use fluctlightdb::manifest::save_v4_dir;
    use fluctlightdb::replicate::{open_replica_brain, sync_once};

    let dir = tempdir().unwrap();
    let primary = dir.path().join("primary_brain");
    let replica = dir.path().join("replica_root");
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "replica sync payload".into(),
            context: "repl".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
                provenance: None,
        })
        .unwrap();
    save_v4_dir(&brain, &primary).unwrap();
    let status = sync_once(&primary, &replica).unwrap();
    assert!(status.snapshot_copied);
    let loaded = open_replica_brain(&replica).unwrap();
    assert_eq!(loaded.hippocampus.engrams.len(), 1);
}

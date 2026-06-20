use std::env;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use fluctlightdb::{default_brain_path, migrate_v3_file_to_v4, BrainServer, Episode, FluctlightBrain};
use fluctlightdb::storage;
use fluctlightdb::store_lock::StoreLock;
use serde::Deserialize;
use uuid::Uuid;

mod display;
mod shell;
mod worker;

fn default_brain_path_cli() -> PathBuf {
    std::env::var("FLUCTLIGHT_BRAIN_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_brain_path())
}

fn dirs_home() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn parse_path(args: &[String]) -> PathBuf {
    args.iter()
        .position(|a| a == "--path")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(default_brain_path_cli)
}

fn parse_addr(args: &[String]) -> String {
    args.iter()
        .position(|a| a == "--addr")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:8792".into())
}

fn serve_addr() -> String {
    env::var("FLUCTLIGHT_SERVE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8792".into())
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string()
}

pub(crate) fn serve_may_be_running() -> bool {
    TcpStream::connect_timeout(
        &serve_addr().parse().unwrap_or_else(|_| "127.0.0.1:8792".parse().unwrap()),
        Duration::from_millis(200),
    )
    .is_ok()
}

fn api_key() -> Option<String> {
    env::var("FLUCTLIGHT_API_KEY").ok()
}

pub(crate) fn http_post_json(path: &str, body: &str) -> Result<String, String> {
    let addr = serve_addr();
    let mut stream = TcpStream::connect(&addr).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let auth = api_key()
        .map(|k| format!("Authorization: Bearer {k}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    Ok(resp[body_start..].to_string())
}

fn wait_for_brain_lock(path: &Path, cmd: &str) {
    if serve_may_be_running() {
        eprintln!(
            "fluctlight: waiting for brain write lock ({cmd} while fluctlight-serve checkpoints)..."
        );
    }
    if let Err(e) = StoreLock::acquire_with_timeout(path, Duration::from_secs(30)) {
        eprintln!("fluctlight: {e}");
        std::process::exit(1);
    }
}

fn is_mutating_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "experience"
            | "experience-json"
            | "mark-core"
            | "sleep"
            | "tick"
            | "run"
            | "compact"
            | "reward"
            | "death"
            | "import-raw"
            | "demo-separate"
            | "fovea-read"
            | "verify-fact"
            | "neurogenesis"
    )
}

fn is_readonly_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "status"
            | "activate"
            | "activate-json"
            | "complete"
            | "export-viz"
            | "export-graph"
            | "export-raw"
            | "preplay"
            | "verified-context"
            | "stage-report"
    )
}

fn try_http_readonly(cmd: &str, args: &[String]) -> Option<String> {
    if !serve_may_be_running() {
        return None;
    }
    let path = match cmd {
        "activate" => "/api/v1/activate",
        "activate-json" => "/api/v1/activate",
        "complete" => "/api/v1/complete",
        "status" => "/api/v1/status",
        "export-viz" => "/api/v1/export-viz",
        "export-graph" => "/api/v1/export-graph",
        "export-raw" => "/api/v1/export-raw",
        "preplay" => "/api/v1/preplay",
        "verified-context" => "/api/v1/verified-context",
        "stage-report" => "/api/v1/stage-report",
        _ => return None,
    };
    let body = match cmd {
        "activate" => serde_json::json!({"cue": args.get(2).cloned().unwrap_or_default()}),
        "preplay" => serde_json::json!({
            "goal": args.get(2).cloned().unwrap_or_default(),
            "steps": args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4u32),
        }),
        "verified-context" => serde_json::json!({
            "limit": args.get(2).and_then(|s| s.parse().ok()).unwrap_or(12usize),
        }),
        "activate-json" => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).ok()?;
            serde_json::from_str(&buf).ok()?
        }
        "complete" => serde_json::json!({"cue": args.get(2).cloned().unwrap_or_default()}),
        _ => serde_json::json!({}),
    };
    let json = serde_json::to_string(&body).ok()?;
    http_post_json(path, &json).ok()
}

fn parse_flag_path(args: &[String], flag: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }

    if args[1] == "worker" {
        let path = parse_path(&args);
        worker::run_worker(path).expect("worker");
        return;
    }

    if args[1] == "shell" {
        let path = parse_path(&args);
        let writable = args.iter().any(|a| a == "--writable");
        let no_http = args.iter().any(|a| a == "--local");
        let mut session = shell::ShellSession::new(shell::ShellOptions {
            path,
            writable,
            use_http: !no_http,
        })
        .expect("shell");
        session.run().expect("shell");
        return;
    }

    if args[1] == "serve" {
        let path = parse_path(&args);
        let addr = parse_addr(&args);
        let read_only = args.iter().any(|a| a == "--replica" || a == "--read-only");
        let server = if read_only {
            BrainServer::open_replica(path).expect("open replica brain for serve")
        } else {
            BrainServer::open(path).expect("open brain for serve")
        };
        if read_only {
            eprintln!("fluctlight serve: read-only replica mode");
        }
        server.serve(&addr).expect("serve");
        return;
    }

    if args[1] == "migrate-v4" {
        let src = parse_flag_path(&args, "--from").unwrap_or_else(|| {
            dirs_home().join(".fluctlight").join("serverbrain.flct")
        });
        let dst = parse_flag_path(&args, "--out")
            .unwrap_or_else(|| fluctlightdb::default_tenant_brain_dir("default"));
        if !src.exists() {
            eprintln!("source not found: {}", src.display());
            std::process::exit(1);
        }
        if storage::is_v4_path(&src) {
            eprintln!("source is already v4: {}", src.display());
            std::process::exit(1);
        }
        fluctlightdb::migrate_v3_file_to_v4(&src, &dst).expect("migrate v4");
        // Copy WAL segments alongside v4 dir for replay on first open.
        let wal_base = src.with_extension("flct.wal");
        let wal_dst = dst.join("wal");
        let _ = std::fs::create_dir_all(&wal_dst);
        if let Ok(read) = std::fs::read_dir(src.parent().unwrap_or(Path::new("."))) {
            let stem = wal_base.file_name().and_then(|s| s.to_str()).unwrap_or("");
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(stem) {
                    let _ = std::fs::copy(entry.path(), wal_dst.join(name));
                }
            }
        }
        println!(
            "{{\"migrated\":\"{}\",\"from\":\"{}\",\"format\":\"v4\"}}",
            dst.display(),
            src.display()
        );
        return;
    }

    if args[1] == "tenant" && args.get(2).map(|s| s.as_str()) == Some("create") {
        let tenant_id = args.get(3).cloned().unwrap_or_else(|| "default".into());
        let cfg = fluctlightdb::tenant::TenantConfig::default_for(
            &tenant_id,
            &fluctlightdb::tenant::default_tenant_root(),
        );
        cfg.ensure_dirs().expect("tenant dirs");
        let _ = FluctlightBrain::open(&cfg.brain_path).expect("init tenant brain");
        println!(
            "{{\"tenant_id\":\"{}\",\"brain_path\":\"{}\"}}",
            tenant_id,
            cfg.brain_path.display()
        );
        return;
    }

    if args[1] == "tenant" && args.get(2).map(|s| s.as_str()) == Some("provision") {
        let tenant_id = args.get(3).cloned().unwrap_or_else(|| "default".into());
        let role = args
            .iter()
            .position(|a| a == "--role")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str())
            .unwrap_or("admin");
        let auth_role = match role {
            "read" => fluctlightdb::auth::Role::Read,
            "write" => fluctlightdb::auth::Role::Write,
            _ => fluctlightdb::auth::Role::Admin,
        };
        let cfg = fluctlightdb::tenant::TenantConfig::default_for(
            &tenant_id,
            &fluctlightdb::tenant::default_tenant_root(),
        );
        cfg.ensure_dirs().expect("tenant dirs");
        let _ = FluctlightBrain::open(&cfg.brain_path).expect("init tenant brain");
        let api_key = fluctlightdb::auth::generate_api_key();
        let keys_line =
            fluctlightdb::auth::format_key_entry(&tenant_id, &api_key, auth_role);
        let auth_env = dirs_home().join(".fluctlight").join("auth.env");
        let _ = std::fs::create_dir_all(auth_env.parent().unwrap());
        let mut env_body = String::new();
        if auth_env.exists() {
            env_body = std::fs::read_to_string(&auth_env).unwrap_or_default();
        }
        if !env_body.contains("FLUCTLIGHT_API_KEYS=") {
            env_body.push_str(&format!(
                "FLUCTLIGHT_REQUIRE_AUTH=true\nFLUCTLIGHT_API_KEYS={keys_line}\nFLUCTLIGHT_API_KEY={api_key}\n"
            ));
        } else {
            let lines: Vec<String> = env_body.lines().map(String::from).collect();
            env_body = lines
                .into_iter()
                .map(|line| {
                    if line.starts_with("FLUCTLIGHT_API_KEYS=") {
                        format!("{line},{keys_line}")
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !env_body.ends_with('\n') {
                env_body.push('\n');
            }
            if !env_body.contains("FLUCTLIGHT_API_KEY=") {
                env_body.push_str(&format!("FLUCTLIGHT_API_KEY={api_key}\n"));
            }
        }
        std::fs::write(&auth_env, env_body).expect("write auth.env");
        println!(
            "{{\"tenant_id\":\"{}\",\"brain_path\":\"{}\",\"api_key\":\"{}\",\"role\":\"{}\",\"keys_entry\":\"{}\",\"auth_env\":\"{}\"}}",
            tenant_id,
            cfg.brain_path.display(),
            api_key,
            role,
            keys_line,
            auth_env.display()
        );
        return;
    }

    if args[1] == "replicate" {
        let primary = parse_flag_path(&args, "--primary")
            .unwrap_or_else(|| fluctlightdb::default_brain_path());
        let replica = parse_flag_path(&args, "--replica")
            .unwrap_or_else(|| dirs_home().join(".fluctlight").join("replica"));
        let interval_secs: u64 = args
            .iter()
            .position(|a| a == "--interval")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        eprintln!(
            "replicating {} -> {} every {}s",
            primary.display(),
            replica.display(),
            interval_secs
        );
        fluctlightdb::replicate::run_tail_loop(
            &primary,
            &replica,
            Duration::from_secs(interval_secs),
        )
        .expect("replicate");
        return;
    }

    if args[1] == "verify" {
        let path = parse_path(&args);
        let report = fluctlightdb::verify_path(&path).expect("verify");
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        if !report.ok {
            std::process::exit(1);
        }
        return;
    }

    let path = parse_path(&args);
    if is_readonly_command(&args[1]) {
        if let Some(out) = try_http_readonly(&args[1], &args) {
            println!("{out}");
            return;
        }
        let brain = FluctlightBrain::open_readonly(&path).expect("open brain (read-only)");
        match args[1].as_str() {
            "status" => {
                println!("{}", serde_json::to_string_pretty(&brain.status()).unwrap());
            }
            "activate" => {
                let cue = args.get(2).cloned().unwrap_or_default();
                let result = brain.activate(&cue);
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            "activate-json" => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf).expect("read stdin");
                let req: ActivateRequest = serde_json::from_str(&buf).expect("parse activate json");
                let result = brain.activate_with_semantic(
                    &req.cue.unwrap_or_default(),
                    req.semantic_vector.as_deref(),
                );
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            "complete" => {
                let cue = args.get(2).cloned().unwrap_or_default();
                match brain.complete(&cue) {
                    Some(e) => println!("{}", serde_json::to_string_pretty(&e).unwrap()),
                    None => println!("null"),
                }
            }
            "preplay" => {
                let goal = args.get(2).cloned().unwrap_or_default();
                let steps: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);
                let result = brain.preplay(&goal, steps);
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            "verified-context" => {
                let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(12);
                let ctx = brain.verified_context(limit);
                println!("{}", serde_json::to_string_pretty(&ctx).unwrap());
            }
            "stage-report" => {
                let rep = brain.stage_report();
                println!("{}", serde_json::to_string_pretty(&rep).unwrap());
            }
            "export-viz" => {
                let out = args
                    .get(2)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| dirs_home().join(".fluctlight").join("brain-viz.json"));
                let viz = brain.export_viz();
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).expect("mkdir");
                }
                std::fs::write(&out, serde_json::to_string_pretty(&viz).unwrap()).expect("write");
                println!("{{\"exported\":\"{}\"}}", out.display());
            }
            "export-graph" => {
                let out = args
                    .get(2)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| dirs_home().join(".fluctlight").join("brain-graph.json"));
                let graph = brain.export_graph();
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).expect("mkdir");
                }
                std::fs::write(&out, serde_json::to_string_pretty(&graph).unwrap()).expect("write");
                println!("{{\"exported\":\"{}\"}}", out.display());
            }
            "export-raw" => {
                let out = args.get(2).map(PathBuf::from);
                let raw = brain.export_raw();
                let json = serde_json::to_string_pretty(&raw).unwrap();
                if let Some(path) = out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).expect("mkdir");
                    }
                    std::fs::write(&path, &json).expect("write");
                    println!(
                        "{{\"exported\":\"{}\",\"engrams\":{},\"synapses_total\":{}}}",
                        path.display(),
                        raw.engrams.len(),
                        raw.synapses_total
                    );
                } else {
                    println!("{json}");
                }
            }
            _ => print_usage(),
        }
        return;
    }

    if args[1] == "fovea-read" {
        let file = args.get(2).expect("fovea-read FILE");
        let dry_run = args.iter().any(|a| a == "--dry-run");
        if dry_run {
            let cfg = fluctlightdb::FoveaConfig::default();
            let packets = fluctlightdb::scan_file(Path::new(file), &cfg).expect("scan");
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "dry_run": true,
                    "packets": packets.len(),
                    "preview": &packets[..packets.len().min(5)],
                }))
                .unwrap()
            );
            return;
        }
        if serve_may_be_running() {
            if let Ok(out) = http_post_json(
                "/api/v1/fovea-read",
                &serde_json::json!({"file_path": file}).to_string(),
            ) {
                println!("{out}");
                return;
            }
        }
        wait_for_brain_lock(&path, "fovea-read");
        let mut brain = FluctlightBrain::open(&path).expect("open brain");
        let cfg = fluctlightdb::FoveaConfig::default();
        let reports = brain
            .fovea_ingest(Path::new(file), &cfg)
            .expect("fovea-read");
        brain.save().expect("save");
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "packets": reports.len(),
                "deduplicated": reports.iter().filter(|r| r.deduplicated).count(),
            }))
            .unwrap()
        );
        return;
    }

    if is_mutating_command(&args[1]) {
        wait_for_brain_lock(&path, &args[1]);
    }
    let mut brain = FluctlightBrain::open(&path).expect("open brain");

    match args[1].as_str() {
        "experience" => {
            let content = args.get(2).cloned().unwrap_or_else(|| "event".into());
            let context = args.get(3).cloned().unwrap_or_else(|| "default".into());
            let path_pos = args.iter().position(|a| a == "--path");
            let end = path_pos.unwrap_or(args.len());
            let rest: Vec<String> = args.get(4..end).unwrap_or(&[]).iter().cloned().collect();
            let (outcome, salience) = match rest.len() {
                0 => (None, 0.5_f32),
                1 => {
                    if let Ok(s) = rest[0].parse::<f32>() {
                        (None, s)
                    } else {
                        (Some(rest[0].clone()), 0.5)
                    }
                }
                _ => (
                    Some(rest[0].clone()),
                    rest[1].parse::<f32>().unwrap_or(0.5),
                ),
            };
            let report = brain
                .experience(Episode {
                    content,
                    context,
                    outcome,
                    salience_hint: salience,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                })
                .expect("experience");
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "experience-json" => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).expect("read stdin");
            let episode: Episode = serde_json::from_str(&buf).expect("parse episode json");
            let report = brain.experience(episode).expect("experience");
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "mark-core" => {
            let engram_id = args.get(2).expect("mark-core ENGRAM_ID KEY");
            let key = args.get(3).cloned().unwrap_or_else(|| "core".into());
            let id = Uuid::parse_str(engram_id).expect("valid engram uuid");
            brain.mark_core(id, key).expect("mark-core");
            brain.save().expect("save");
            println!("{{\"ok\":true}}");
        }
        "verify-fact" => {
            let engram_id = args.get(2).expect("verify-fact ENGRAM_ID");
            let id = Uuid::parse_str(engram_id).expect("valid engram uuid");
            let kind = args.get(3).cloned().unwrap_or_else(|| "ledger_verified".into());
            let pk = match kind.as_str() {
                "chat_assertion" => fluctlightdb::ProvenanceKind::ChatAssertion,
                "file_observation" => fluctlightdb::ProvenanceKind::FileObservation,
                "tool_grounded" => fluctlightdb::ProvenanceKind::ToolGrounded,
                "user_explicit" => fluctlightdb::ProvenanceKind::UserExplicit,
                _ => fluctlightdb::ProvenanceKind::LedgerVerified,
            };
            brain
                .verify_fact(id, pk, None, 0.95)
                .expect("verify-fact");
            brain.save().expect("save");
            println!("{{\"ok\":true,\"engram_id\":\"{engram_id}\"}}");
        }
        "neurogenesis" => {
            let report = brain.neurogenesis_pulse().expect("neurogenesis");
            brain.save().expect("save");
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "sleep" => {
            let report = brain.sleep().expect("sleep");
            brain.save().expect("save");
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "tick" => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let reports = brain.tick_n(n).expect("tick");
            brain.save().expect("save");
            println!("{}", serde_json::to_string_pretty(&reports).unwrap());
        }
        "compact" => {
            let report = brain.compact().expect("compact");
            brain.save().expect("save");
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "run" => {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
            eprintln!("autonomic loop every {secs}s (Ctrl+C to stop)");
            loop {
                let report = brain.tick().expect("tick");
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
                thread::sleep(Duration::from_secs(secs));
            }
        }
        "export-viz" | "export-graph" | "export-raw" => {
            eprintln!("use readonly path — command should not reach here");
            std::process::exit(1);
        }
        "import-raw" => {
            let file = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| dirs_home().join(".fluctlight").join("brain-raw.json"));
            let json = std::fs::read_to_string(&file).expect("read raw json");
            let report = fluctlightdb::import_raw_json(&mut brain, &json).expect("import raw");
            brain.save().expect("save");
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "demo-separate" => {
            run_separation_demo(&mut brain);
        }
        "reward" => {
            let mag: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.5);
            brain.reward(mag).expect("reward");
            brain.save().expect("save");
            println!("{{\"ok\":true}}");
        }
        "death" => {
            let cause = args.get(2).cloned().unwrap_or_else(|| "unknown".into());
            let new_life = brain.death(&cause).expect("death");
            println!("{{\"new_life_id\":\"{new_life}\"}}");
        }
        _ => print_usage(),
    }
}

#[derive(Deserialize)]
struct ActivateRequest {
    cue: Option<String>,
    semantic_vector: Option<Vec<f32>>,
}

fn run_separation_demo(brain: &mut FluctlightBrain) {
    let pairs = [
        ("task failed timeout", "task failed retry"),
        ("user asked summarize", "user asked translate"),
    ];
    for (a, b) in pairs {
        let r1 = brain
            .experience(Episode {
                content: a.into(),
                context: "production".into(),
                outcome: None,
                salience_hint: 0.6,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        let r2 = brain
            .experience(Episode {
                content: b.into(),
                context: "production".into(),
                outcome: None,
                salience_hint: 0.6,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        println!(
            "---\nA: {a}\nB: {b}\noverlap_before={:.2} overlap_after={:.2} sep_index={:.2} separators={}\n",
            r2.separation.max_overlap_before,
            r2.separation.max_overlap_after,
            r2.separation.separation_index,
            r2.separation.separators_added
        );
        let _ = r1;
    }
    let viz_path = dirs_home().join(".fluctlight").join("brain-viz.json");
    let viz = brain.export_viz();
    let _ = std::fs::create_dir_all(viz_path.parent().unwrap());
    std::fs::write(&viz_path, serde_json::to_string_pretty(&viz).unwrap()).unwrap();
    println!("viz exported to {}", viz_path.display());
}

fn print_usage() {
    eprintln!(
        "fluctlight — brain-native agent database\n\
         \n\
         fluctlight worker [--path FILE]   persistent in-process JSON-RPC (stdin/stdout)\n\
         fluctlight shell [--path FILE] [--writable] [--local]   interactive REPL\n\
         fluctlight status [--path FILE]\n\
         fluctlight verify [--path FILE]   validate snapshot header + bincode\n\
         fluctlight experience CONTENT [CONTEXT] [OUTCOME] [SALIENCE] [--path FILE]\n\
         fluctlight experience-json [--path FILE]   (read Episode JSON from stdin)\n\
         fluctlight mark-core ENGRAM_ID KEY [--path FILE]\n\
         fluctlight activate CUE [--path FILE]   (HTTP-first when serve running)\n\
         fluctlight fovea-read FILE [--dry-run] [--path FILE]   saccadic file intake\n\
         fluctlight verify-fact ENGRAM_ID [KIND] [--path FILE]   mark ground truth\n\
         fluctlight preplay GOAL [STEPS] [--path FILE]   prospective activation path\n\
         fluctlight verified-context [LIMIT] [--path FILE]   ledger/file ground truth\n\
         fluctlight stage-report [--path FILE]   CLS maturation metrics\n\
         fluctlight neurogenesis [--path FILE]   immature probe pulse\n\
         fluctlight activate-json [--path FILE]     (read cue + semantic_vector from stdin)\n\
         fluctlight complete CUE [--path FILE]\n\
         fluctlight sleep [--path FILE]\n\
         fluctlight tick [N] [--path FILE]     background heartbeat + auto-sleep\n\
         fluctlight compact [--path FILE]      merge engrams + dedupe synapses\n\
         fluctlight serve [--addr HOST:PORT] [--path FILE] [--replica]   in-process HTTP API\n\
         fluctlight replicate --primary PATH --replica DIR [--interval SEC]\n\
         fluctlight run [SECONDS] [--path FILE]  autonomic loop (daemon)\n\
         fluctlight export-viz [FILE] [--path FILE]\n\
         fluctlight export-graph [FILE] [--path FILE]\n\
         fluctlight export-raw [FILE] [--path FILE]   full engrams + synapses (JSON)
         fluctlight import-raw [FILE] [--path FILE]   restore from export-raw JSON\n\
         fluctlight migrate-v4 [--from FILE.flct] [--out DIR]   v3 → v4 tenant brain\n\
         fluctlight tenant create TENANT_ID   provision tenant brain dir\n\
         fluctlight tenant provision TENANT_ID [--role admin|write|read]   tenant + API key\n\
         fluctlight demo-separate [--path FILE]  DG separation demo + viz export\n\
         fluctlight reward [MAGNITUDE] [--path FILE]\n\
         fluctlight death [CAUSE] [--path FILE]"
    );
}

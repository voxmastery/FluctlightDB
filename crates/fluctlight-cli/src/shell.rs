//! Interactive brain-native REPL — `fluctlight shell`.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use fluctlightdb::query::{self, QueryRequest, QueryResponse};
use fluctlightdb::FluctlightBrain;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use uuid::Uuid;

use crate::display::{print_footer, print_json, short_id, truncate, yes_no, Table};

pub struct ShellOptions {
    pub path: PathBuf,
    pub writable: bool,
    pub use_http: bool,
}

enum Backend {
    Local(FluctlightBrain),
    Http,
}

pub struct ShellSession {
    backend: Backend,
    json_mode: bool,
    writable: bool,
}

impl ShellSession {
    pub fn new(opts: ShellOptions) -> Result<Self, String> {
        let backend = if opts.use_http && crate::serve_may_be_running() {
            Backend::Http
        } else {
            let brain = if opts.writable {
                FluctlightBrain::open(&opts.path).map_err(|e| e.to_string())?
            } else {
                FluctlightBrain::open_readonly(&opts.path).map_err(|e| e.to_string())?
            };
            Backend::Local(brain)
        };
        Ok(Self {
            backend,
            json_mode: false,
            writable: opts.writable,
        })
    }

    pub fn run(&mut self) -> Result<(), String> {
        let mut rl = DefaultEditor::new().map_err(|e| e.to_string())?;
        let _ = rl.load_history(".fluctlight_history");
        println!("FluctlightDB shell — brain-native memory (type `help`, `quit` to exit)");
        match &self.backend {
            Backend::Http => println!("mode: HTTP (fluctlight-serve)"),
            Backend::Local(_) => println!("mode: local brain"),
        }
        loop {
            let line = match rl.readline("fluctlight> ") {
                Ok(l) => l,
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
                Err(e) => return Err(e.to_string()),
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let _ = rl.add_history_entry(line);
            if line == "quit" || line == "exit" || line == "\\q" {
                break;
            }
            if let Err(e) = self.dispatch(line) {
                eprintln!("error: {e}");
            }
        }
        let _ = rl.save_history(".fluctlight_history");
        Ok(())
    }

    fn dispatch(&mut self, line: &str) -> Result<(), String> {
        if line.starts_with("\\json ") {
            self.json_mode = line.contains("on");
            println!("json mode: {}", if self.json_mode { "on" } else { "off" });
            return Ok(());
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts[0];
        match cmd {
            "help" | "?" => {
                self.cmd_help();
                Ok(())
            }
            "status" => self.cmd_status(),
            "list" => {
                let n: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
                self.cmd_list(n)
            }
            "get" => {
                let id = parts.get(1).ok_or("usage: get ENGRAM_ID")?;
                self.cmd_get(id)
            }
            "recall" => {
                let cue = parts[1..].join(" ");
                if cue.is_empty() {
                    return Err("usage: recall CUE".into());
                }
                self.cmd_recall(&cue)
            }
            "complete" => {
                let cue = parts[1..].join(" ");
                if cue.is_empty() {
                    return Err("usage: complete CUE".into());
                }
                self.cmd_complete(&cue)
            }
            "verified" => self.cmd_verified(),
            "warnings" => self.cmd_warnings(),
            "rag" => {
                let doc = parts.get(1).ok_or("usage: rag DOC_ID [CHUNK_ID]")?;
                let chunk = parts.get(2).copied();
                self.cmd_rag(doc, chunk)
            }
            "preplay" => {
                let (goal, steps) = parse_preplay_args(&parts[1..])?;
                self.cmd_preplay(&goal, steps)
            }
            "stage" => self.cmd_stage(),
            "forget" => {
                if !self.writable {
                    return Err("forget requires --writable".into());
                }
                let id = parts.get(1).ok_or("usage: forget ENGRAM_ID")?;
                self.cmd_forget(id)
            }
            "export" => {
                let kind = parts.get(1).ok_or("usage: export viz|graph|raw [FILE]")?;
                let file = parts.get(2).map(|s| s.to_string());
                self.cmd_export(kind, file)
            }
            _ => Err(format!("unknown command: {cmd} (try help)")),
        }
    }

    fn cmd_help(&self) {
        println!(
            "Brain-native commands (not SQL):\n\
             status          brain counts and stage\n\
             list [N]        browse engrams (SELECT * LIMIT)\n\
             get UUID        one engram by id\n\
             recall CUE      spreading activation (vector+graph search)\n\
             complete CUE    pattern completion\n\
             verified        ledger/file ground truth\n\
             warnings        unverified factual chat claims\n\
             rag DOC [CHUNK] RAG chunks by doc\n\
             preplay GOAL [N] planning / EXPLAIN path\n\
             stage           CLS maturation metrics\n\
             forget UUID     delete engram (--writable)\n\
             export viz|graph|raw [FILE]\n\
             \\json on|off     toggle JSON output\n\
             quit            exit"
        );
    }

    fn cmd_status(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        match &self.backend {
            Backend::Local(brain) => {
                let s = brain.status();
                if self.json_mode {
                    print_json(&s);
                } else {
                    let mut t = Table::new(&["field", "value"]);
                    t.push(vec!["stage".into(), s.stage]);
                    t.push(vec!["engrams".into(), s.engrams.to_string()]);
                    t.push(vec!["synapses".into(), s.synapses.to_string()]);
                    t.push(vec!["experiences".into(), s.experiences.to_string()]);
                    t.push(vec!["sleep_cycles".into(), s.sleep_cycles.to_string()]);
                    t.push(vec![
                        "synapse_pressure".into(),
                        format!("{:.3}", s.synapse_pressure),
                    ]);
                    t.push(vec!["pfc_unlocked".into(), yes_no(s.pfc_unlocked).into()]);
                    t.push(vec!["wal_seq".into(), s.wal_seq.to_string()]);
                    t.print();
                }
                print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
            }
            Backend::Http => {
                let body = http_post("/api/v1/status", "{}")?;
                if self.json_mode {
                    println!("{body}");
                } else {
                    let v: serde_json::Value =
                        serde_json::from_str(&body).map_err(|e| e.to_string())?;
                    let mut t = Table::new(&["field", "value"]);
                    if let Some(obj) = v.as_object() {
                        for (k, val) in obj {
                            t.push(vec![k.clone(), val.to_string()]);
                        }
                    }
                    t.print();
                }
                print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        Ok(())
    }

    fn cmd_list(&mut self, limit: usize) -> Result<(), String> {
        let t0 = Instant::now();
        let resp = self.query(QueryRequest::ListEngrams {
            agent_id: None,
            page: 0,
            page_size: limit,
        })?;
        self.render_engram_list(resp, t0)
    }

    fn cmd_get(&mut self, id_str: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        let t0 = Instant::now();
        let resp = self.query(QueryRequest::GetEngram { engram_id: id })?;
        if self.json_mode {
            print_json(&resp);
        } else if let QueryResponse::GetEngram { item } = resp {
            match item {
                Some(e) => print_engram_detail(&e),
                None => println!("(not found)"),
            }
        }
        print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    fn cmd_recall(&mut self, cue: &str) -> Result<(), String> {
        let t0 = Instant::now();
        match &self.backend {
            Backend::Local(brain) => {
                let r = brain.activate(cue);
                self.render_recalls(&r.recalls, t0);
            }
            Backend::Http => {
                let body = serde_json::json!({"cue": cue});
                let resp = http_post("/api/v1/activate", &body.to_string())?;
                if self.json_mode {
                    println!("{resp}");
                } else {
                    let v: serde_json::Value =
                        serde_json::from_str(&resp).map_err(|e| e.to_string())?;
                    render_recalls_json(&v, t0);
                }
            }
        }
        Ok(())
    }

    fn cmd_complete(&mut self, cue: &str) -> Result<(), String> {
        let t0 = Instant::now();
        match &self.backend {
            Backend::Local(brain) => {
                if self.json_mode {
                    print_json(&brain.complete(cue));
                } else {
                    match brain.complete(cue) {
                        Some(e) => println!("{}", truncate(&e.episode.content, 200)),
                        None => println!("(no completion)"),
                    }
                }
            }
            Backend::Http => {
                let body = serde_json::json!({"cue": cue});
                let resp = http_post("/api/v1/complete", &body.to_string())?;
                if self.json_mode {
                    println!("{resp}");
                } else {
                    let v: serde_json::Value =
                        serde_json::from_str(&resp).map_err(|e| e.to_string())?;
                    if let Some(content) = v
                        .get("episode")
                        .and_then(|e| e.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        println!("{}", truncate(content, 200));
                    } else {
                        println!("(no completion)");
                    }
                }
            }
        }
        print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    fn cmd_verified(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        match &self.backend {
            Backend::Local(brain) => {
                let ctx = brain.verified_context(20);
                if self.json_mode {
                    print_json(&ctx);
                } else {
                    let mut t = Table::new(&["confidence", "content"]);
                    for f in &ctx.facts {
                        t.push(vec![
                            format!("{:.0}%", f.confidence * 100.0),
                            truncate(&f.content, 60),
                        ]);
                    }
                    t.print();
                }
                print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
            }
            Backend::Http => {
                let resp = http_post("/api/v1/verified-context", r#"{"limit":20}"#)?;
                if self.json_mode {
                    println!("{resp}");
                } else {
                    let v: serde_json::Value =
                        serde_json::from_str(&resp).map_err(|e| e.to_string())?;
                    let mut t = Table::new(&["confidence", "content"]);
                    for f in v
                        .get("facts")
                        .and_then(|x| x.as_array())
                        .into_iter()
                        .flatten()
                    {
                        t.push(vec![
                            format!(
                                "{:.0}%",
                                f.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0) * 100.0
                            ),
                            truncate(f.get("content").and_then(|c| c.as_str()).unwrap_or(""), 60),
                        ]);
                    }
                    t.print();
                }
                print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        Ok(())
    }

    fn cmd_warnings(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        let resp = self.query(QueryRequest::ListUnverified {
            page: 0,
            page_size: 20,
        })?;
        if self.json_mode {
            print_json(&resp);
        } else if let QueryResponse::ListUnverified { items, .. } = resp {
            let mut t = Table::new(&["id", "content"]);
            for e in &items {
                t.push(vec![short_id(&e.engram_id), truncate(&e.content, 60)]);
            }
            let n = t.len();
            t.print();
            print_footer(n, t0.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(())
    }

    fn cmd_rag(&mut self, doc_id: &str, chunk_id: Option<&str>) -> Result<(), String> {
        let t0 = Instant::now();
        let resp = self.query(QueryRequest::SearchByRag {
            doc_id: doc_id.to_string(),
            chunk_id: chunk_id.map(|s| s.to_string()),
            page: 0,
            page_size: 20,
        })?;
        self.render_engram_list(resp, t0)
    }

    fn cmd_preplay(&mut self, goal: &str, steps: u32) -> Result<(), String> {
        let t0 = Instant::now();
        match &self.backend {
            Backend::Local(brain) => {
                let r = brain.preplay(goal, steps);
                if self.json_mode {
                    print_json(&r);
                } else {
                    let mut t = Table::new(&["hop", "activation", "preview"]);
                    for s in &r.path {
                        t.push(vec![
                            s.hop.to_string(),
                            format!("{:.2}", s.activation),
                            s.engram_preview
                                .as_deref()
                                .map(|p| truncate(p, 50))
                                .unwrap_or_default(),
                        ]);
                    }
                    t.print();
                }
            }
            Backend::Http => {
                let body = serde_json::json!({"goal": goal, "steps": steps});
                let resp = http_post("/api/v1/preplay", &body.to_string())?;
                if self.json_mode {
                    println!("{resp}");
                } else {
                    let v: serde_json::Value =
                        serde_json::from_str(&resp).map_err(|e| e.to_string())?;
                    let mut t = Table::new(&["hop", "activation", "preview"]);
                    for s in v
                        .get("path")
                        .and_then(|p| p.as_array())
                        .into_iter()
                        .flatten()
                    {
                        t.push(vec![
                            s.get("hop")
                                .and_then(|h| h.as_u64())
                                .unwrap_or(0)
                                .to_string(),
                            format!(
                                "{:.2}",
                                s.get("activation").and_then(|a| a.as_f64()).unwrap_or(0.0)
                            ),
                            truncate(
                                s.get("engram_preview")
                                    .and_then(|p| p.as_str())
                                    .unwrap_or(""),
                                50,
                            ),
                        ]);
                    }
                    t.print();
                }
            }
        }
        print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    fn cmd_stage(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        match &self.backend {
            Backend::Local(brain) => {
                let r = brain.stage_report();
                if self.json_mode {
                    print_json(&r);
                } else {
                    let mut t = Table::new(&["field", "value"]);
                    t.push(vec!["stage".into(), r.stage]);
                    t.push(vec!["next_stage".into(), r.next_stage.unwrap_or_default()]);
                    t.push(vec!["myelination".into(), format!("{:.2}", r.myelination)]);
                    t.push(vec![
                        "synapse_pressure".into(),
                        format!("{:.3}", r.synapse_pressure),
                    ]);
                    t.push(vec![
                        "progress_to_next".into(),
                        format!("{:.0}%", r.progress_to_next * 100.0),
                    ]);
                    t.push(vec!["pfc_unlocked".into(), yes_no(r.pfc_unlocked).into()]);
                    t.print();
                }
            }
            Backend::Http => {
                let resp = http_post("/api/v1/stage-report", "{}")?;
                if self.json_mode {
                    println!("{resp}");
                } else {
                    let v: serde_json::Value =
                        serde_json::from_str(&resp).map_err(|e| e.to_string())?;
                    let mut t = Table::new(&["field", "value"]);
                    if let Some(obj) = v.as_object() {
                        for (k, val) in obj {
                            t.push(vec![k.clone(), val.to_string()]);
                        }
                    }
                    t.print();
                }
            }
        }
        print_footer(1, t0.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    fn cmd_forget(&mut self, id_str: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
        print!("forget {id}? [y/N] ");
        io::stdout().flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).map_err(|e| e.to_string())?;
        if !buf.trim().eq_ignore_ascii_case("y") {
            println!("cancelled");
            return Ok(());
        }
        match &mut self.backend {
            Backend::Local(brain) => {
                let resp = query::execute_mut(brain, QueryRequest::Forget { engram_id: id });
                brain.save().map_err(|e| e.to_string())?;
                if self.json_mode {
                    print_json(&resp);
                } else if let QueryResponse::Forget { removed } = resp {
                    println!("removed: {removed}");
                }
            }
            Backend::Http => {
                let body =
                    serde_json::json!({"query": {"op": "forget", "engram_id": id.to_string()}});
                let resp = http_post("/api/v1/query", &body.to_string())?;
                println!("{resp}");
            }
        }
        Ok(())
    }

    fn cmd_export(&mut self, kind: &str, file: Option<String>) -> Result<(), String> {
        match &self.backend {
            Backend::Local(brain) => {
                let path = file
                    .map(PathBuf::from)
                    .unwrap_or_else(|| default_export_path(kind));
                match kind {
                    "viz" => {
                        let v = brain.export_viz();
                        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap())
                            .map_err(|e| e.to_string())?;
                    }
                    "graph" => {
                        let g = brain.export_graph();
                        std::fs::write(&path, serde_json::to_string_pretty(&g).unwrap())
                            .map_err(|e| e.to_string())?;
                    }
                    "raw" => {
                        let r = brain.export_raw();
                        std::fs::write(&path, serde_json::to_string_pretty(&r).unwrap())
                            .map_err(|e| e.to_string())?;
                    }
                    _ => return Err("usage: export viz|graph|raw".into()),
                }
                println!("exported → {}", path.display());
            }
            Backend::Http => {
                let path = match kind {
                    "viz" => "/api/v1/export-viz",
                    "graph" => "/api/v1/export-graph",
                    "raw" => "/api/v1/export-raw",
                    _ => return Err("usage: export viz|graph|raw".into()),
                };
                let resp = http_post(path, "{}")?;
                if let Some(f) = file {
                    std::fs::write(&f, &resp).map_err(|e| e.to_string())?;
                    println!("exported → {f}");
                } else {
                    println!("{resp}");
                }
            }
        }
        Ok(())
    }

    fn query(&mut self, req: QueryRequest) -> Result<QueryResponse, String> {
        match &mut self.backend {
            Backend::Local(brain) => {
                if matches!(
                    req,
                    QueryRequest::Forget { .. } | QueryRequest::ForgetBefore { .. }
                ) {
                    Ok(query::execute_mut(brain, req))
                } else {
                    Ok(query::execute(brain, req))
                }
            }
            Backend::Http => {
                let body = serde_json::json!({"query": req});
                let resp = http_post("/api/v1/query", &body.to_string())?;
                serde_json::from_str(&resp).map_err(|e| e.to_string())
            }
        }
    }

    fn render_engram_list(&self, resp: QueryResponse, t0: Instant) -> Result<(), String> {
        let items = match resp {
            QueryResponse::ListEngrams { items, .. }
            | QueryResponse::ListVerified { items, .. }
            | QueryResponse::ListUnverified { items, .. }
            | QueryResponse::SearchByRag { items, .. } => items,
            other => {
                if self.json_mode {
                    print_json(&other);
                }
                return Ok(());
            }
        };
        if self.json_mode {
            print_json(&items);
        } else {
            let mut t = Table::new(&["id", "sal", "verified", "content", "context"]);
            for e in &items {
                t.push(vec![
                    short_id(&e.engram_id),
                    format!("{:.2}", e.salience),
                    yes_no(e.verified).into(),
                    truncate(&e.content, 50),
                    truncate(&e.context, 20),
                ]);
            }
            let n = t.len();
            t.print();
            print_footer(n, t0.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(())
    }

    fn render_recalls(&self, recalls: &[fluctlightdb::RecallResult], t0: Instant) {
        if self.json_mode {
            print_json(&recalls.to_vec());
        } else {
            let mut t = Table::new(&["activation", "verified", "content"]);
            for r in recalls {
                t.push(vec![
                    format!("{:.2}", r.activation),
                    yes_no(r.verified).into(),
                    truncate(&r.episode.content, 60),
                ]);
            }
            let n = t.len();
            t.print();
            print_footer(n, t0.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

fn parse_preplay_args(parts: &[&str]) -> Result<(String, u32), String> {
    if parts.is_empty() {
        return Err("usage: preplay GOAL [STEPS]".into());
    }
    if parts.len() > 1 {
        if let Ok(steps) = parts.last().unwrap().parse::<u32>() {
            let goal = parts[..parts.len() - 1].join(" ");
            if !goal.is_empty() {
                return Ok((goal, steps));
            }
        }
    }
    Ok((parts.join(" "), 4))
}

fn print_engram_detail(e: &fluctlightdb::query::EngramSummary) {
    println!("id:       {}", e.engram_id);
    println!("salience: {:.2}", e.salience);
    println!("verified: {}", yes_no(e.verified));
    if let Some(ref k) = e.provenance_kind {
        println!("provenance: {k}");
    }
    if let Some(ref u) = e.source_uri {
        println!("source: {u}");
    }
    println!("context:  {}", e.context);
    println!("content:  {}", e.content);
}

fn render_recalls_json(v: &serde_json::Value, t0: Instant) {
    let mut t = Table::new(&["activation", "verified", "content"]);
    let n = if let Some(arr) = v.get("recalls").and_then(|r| r.as_array()) {
        for r in arr {
            t.push(vec![
                format!(
                    "{:.2}",
                    r.get("activation").and_then(|a| a.as_f64()).unwrap_or(0.0)
                ),
                if r.get("verified").and_then(|x| x.as_bool()).unwrap_or(false) {
                    "yes".into()
                } else {
                    "no".into()
                },
                truncate(
                    r.get("episode")
                        .and_then(|e| e.get("content"))
                        .and_then(|c| c.as_str())
                        .unwrap_or(""),
                    60,
                ),
            ]);
        }
        arr.len()
    } else {
        0
    };
    t.print();
    print_footer(n, t0.elapsed().as_secs_f64() * 1000.0);
}

fn default_export_path(kind: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".fluctlight")
        .join(format!("brain-{kind}.json"))
}

fn http_post(path: &str, body: &str) -> Result<String, String> {
    crate::http_post_json(path, body)
}

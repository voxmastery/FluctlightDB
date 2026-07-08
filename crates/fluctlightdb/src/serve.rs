//! In-process HTTP API — tenant pool, RwLock reads, auth, metrics, v1 contract.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[cfg(unix)]
extern "C" fn handle_shutdown_signal(_: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, AtomicOrdering::SeqCst);
}

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, AtomicOrdering::SeqCst);
}

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth::{AuthConfig, AuthContext, Role};
use crate::autonomic::TickReport;
use crate::brain::FluctlightBrain;
use crate::compact::CompactReport;
use crate::error::{Error, Result};
use crate::metrics::{Metrics, Timer};
use crate::query::{self, QueryRequest};
use crate::store;
use crate::tenant::{default_tenant_root, TenantConfig};
use crate::types::{ActivationResult, Episode, ExperienceReport};

const MAX_BODY_BYTES: usize = 1_048_576;

/// The built-in Living Brain viewer (single-file SPA, no build step).
const VIEWER_HTML: &str = include_str!("viewer.html");
const MAX_IDEMPOTENCY_KEYS: usize = 10_000;
const DEFAULT_HOT_TENANTS: usize = 256;
const DEFAULT_MAX_CONNECTIONS: usize = 500;

fn max_connections() -> usize {
    std::env::var("FLUCTLIGHT_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_CONNECTIONS)
        .max(32)
}

struct ConnectionGate(Arc<AtomicUsize>);

impl Clone for ConnectionGate {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl ConnectionGate {
    fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }

    fn try_acquire(&self, metrics: &Arc<Metrics>) -> Option<ConnectionGuard> {
        let max = max_connections();
        loop {
            let cur = self.0.load(AtomicOrdering::Acquire);
            if cur >= max {
                metrics
                    .rejected_connections
                    .fetch_add(1, AtomicOrdering::Relaxed);
                return None;
            }
            if self
                .0
                .compare_exchange(
                    cur,
                    cur + 1,
                    AtomicOrdering::AcqRel,
                    AtomicOrdering::Relaxed,
                )
                .is_ok()
            {
                metrics
                    .active_connections
                    .fetch_add(1, AtomicOrdering::Relaxed);
                return Some(ConnectionGuard {
                    gate: self.0.clone(),
                    metrics: metrics.clone(),
                });
            }
        }
    }
}

struct ConnectionGuard {
    gate: Arc<AtomicUsize>,
    metrics: Arc<Metrics>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.gate.fetch_sub(1, AtomicOrdering::Release);
        self.metrics
            .active_connections
            .fetch_sub(1, AtomicOrdering::Relaxed);
    }
}

fn max_hot_tenants() -> usize {
    std::env::var("FLUCTLIGHT_MAX_HOT_TENANTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_HOT_TENANTS)
        .max(16)
}

#[derive(Clone)]
pub struct BrainServer {
    pool: Arc<RwLock<BrainPool>>,
    default_path: PathBuf,
    auth: AuthConfig,
    metrics: Arc<Metrics>,
    idempotency: Arc<RwLock<HashSet<String>>>,
    read_only: bool,
}

struct TenantSlot {
    brain: Arc<RwLock<FluctlightBrain>>,
    path: PathBuf,
    loaded_mtime: SystemTime,
    last_access: Instant,
}

struct BrainPool {
    tenants: HashMap<String, TenantSlot>,
    tenant_root: PathBuf,
    default_tenant: String,
}

impl BrainServer {
    pub fn open(path: PathBuf) -> Result<Self> {
        let tenant = "default".to_string();
        let mut tenants = HashMap::new();
        let brain = FluctlightBrain::open(&path)?;
        let loaded_mtime = store::snapshot_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);
        tenants.insert(
            tenant.clone(),
            TenantSlot {
                brain: Arc::new(RwLock::new(brain)),
                path: path.clone(),
                loaded_mtime,
                last_access: Instant::now(),
            },
        );
        let pool = BrainPool {
            tenants,
            tenant_root: default_tenant_root(),
            default_tenant: tenant,
        };
        let metrics = Metrics::new();
        if let Some(slot) = pool.tenants.get("default") {
            if let Ok(guard) = slot.brain.read() {
                metrics.set_synapses(guard.graph.synapse_count());
            }
        }
        Ok(Self {
            pool: Arc::new(RwLock::new(pool)),
            default_path: path,
            auth: AuthConfig::from_env(),
            metrics,
            idempotency: Arc::new(RwLock::new(HashSet::new())),
            read_only: false,
        })
    }

    pub fn open_replica(path: PathBuf) -> Result<Self> {
        let mut server = Self::open(path)?;
        server.read_only = true;
        Ok(server)
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn brain_path(&self) -> &PathBuf {
        &self.default_path
    }

    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }

    fn tenant_path(&self, tenant_id: &str, pool: &BrainPool) -> PathBuf {
        if tenant_id == pool.default_tenant {
            return self.default_path.clone();
        }
        TenantConfig::default_for(tenant_id, &pool.tenant_root).brain_path
    }

    fn refresh_if_stale(&self, tenant_id: &str) -> Result<()> {
        let disk_mtime = {
            let pool = self
                .pool
                .read()
                .map_err(|_| Error::Store("pool lock poisoned".into()))?;
            let path = pool
                .tenants
                .get(tenant_id)
                .map(|s| s.path.clone())
                .unwrap_or_else(|| self.tenant_path(tenant_id, &pool));
            store::snapshot_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH)
        };

        let mut pool = self
            .pool
            .write()
            .map_err(|_| Error::Store("pool lock poisoned".into()))?;
        let slot = match pool.tenants.get_mut(tenant_id) {
            Some(s) => s,
            None => return Ok(()),
        };
        if disk_mtime <= slot.loaded_mtime {
            return Ok(());
        }
        let brain = FluctlightBrain::open(&slot.path)?;
        slot.brain = Arc::new(RwLock::new(brain));
        slot.loaded_mtime = disk_mtime;
        Ok(())
    }

    fn touch_mtime(&self, tenant_id: &str) {
        let mut pool = match self.pool.write() {
            Ok(p) => p,
            Err(_) => return,
        };
        if let Some(slot) = pool.tenants.get_mut(tenant_id) {
            slot.loaded_mtime = store::snapshot_mtime(&slot.path).unwrap_or(SystemTime::UNIX_EPOCH);
        }
    }

    fn get_brain(&self, tenant_id: &str) -> Result<Arc<RwLock<FluctlightBrain>>> {
        self.refresh_if_stale(tenant_id)?;
        {
            let pool = self
                .pool
                .read()
                .map_err(|_| Error::Store("pool lock poisoned".into()))?;
            if let Some(slot) = pool.tenants.get(tenant_id) {
                return Ok(slot.brain.clone());
            }
        }
        let mut pool = self
            .pool
            .write()
            .map_err(|_| Error::Store("pool lock poisoned".into()))?;
        if let Some(slot) = pool.tenants.get_mut(tenant_id) {
            slot.last_access = Instant::now();
            return Ok(slot.brain.clone());
        }
        self.evict_if_needed(&mut pool);
        let cfg = if tenant_id == pool.default_tenant {
            TenantConfig::with_brain_path(tenant_id, &pool.tenant_root, self.default_path.clone())
        } else {
            TenantConfig::default_for(tenant_id, &pool.tenant_root)
        };
        cfg.ensure_dirs().map_err(Error::Io)?;
        let brain = FluctlightBrain::open(&cfg.brain_path)?;
        let loaded_mtime = store::snapshot_mtime(&cfg.brain_path).unwrap_or(SystemTime::UNIX_EPOCH);
        let arc = Arc::new(RwLock::new(brain));
        pool.tenants.insert(
            tenant_id.to_string(),
            TenantSlot {
                brain: arc.clone(),
                path: cfg.brain_path,
                loaded_mtime,
                last_access: Instant::now(),
            },
        );
        Ok(arc)
    }

    fn evict_if_needed(&self, pool: &mut BrainPool) {
        let max = max_hot_tenants();
        while pool.tenants.len() >= max {
            let lru = pool
                .tenants
                .iter()
                .filter(|(id, _)| *id != &pool.default_tenant)
                .min_by_key(|(_, slot)| slot.last_access)
                .map(|(k, _)| k.clone());
            let Some(key) = lru else { break };
            pool.tenants.remove(&key);
        }
    }

    pub fn with_brain_read<F, T>(&self, tenant_id: &str, f: F) -> Result<T>
    where
        F: FnOnce(&FluctlightBrain) -> Result<T>,
    {
        let brain = self.get_brain(tenant_id)?;
        let guard = brain
            .read()
            .map_err(|_| Error::Store("brain lock poisoned".into()))?;
        f(&guard)
    }

    pub fn with_brain_write<F, T>(&self, tenant_id: &str, f: F) -> Result<T>
    where
        F: FnOnce(&mut FluctlightBrain) -> Result<T>,
    {
        let brain = self.get_brain(tenant_id)?;
        let mut guard = brain
            .write()
            .map_err(|_| Error::Store("brain lock poisoned".into()))?;
        let out = f(&mut guard)?;
        self.metrics.set_synapses(guard.graph.synapse_count());
        self.touch_mtime(tenant_id);
        Ok(out)
    }

    pub fn flush_all_checkpoints(&self) -> Result<()> {
        let pool = self
            .pool
            .read()
            .map_err(|_| Error::Store("pool lock poisoned".into()))?;
        for slot in pool.tenants.values() {
            if let Ok(guard) = slot.brain.read() {
                let _ = guard.checkpoint();
            }
        }
        Ok(())
    }

    pub fn serve(&self, addr: &str) -> Result<()> {
        enforce_bind_auth(addr, &self.auth)?;
        #[cfg(unix)]
        unsafe {
            libc::signal(
                libc::SIGTERM,
                handle_shutdown_signal as *const () as libc::sighandler_t,
            );
            libc::signal(
                libc::SIGINT,
                handle_shutdown_signal as *const () as libc::sighandler_t,
            );
        }
        let listener = TcpListener::bind(addr).map_err(Error::Io)?;
        listener.set_nonblocking(true).map_err(Error::Io)?;
        eprintln!("fluctlight serve listening on http://{addr}");
        let server = self.clone();
        let gate = ConnectionGate::new();
        while !SHUTDOWN_REQUESTED.load(AtomicOrdering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let server = server.clone();
                    let gate = gate.clone();
                    let Some(_guard) = gate.try_acquire(&server.metrics) else {
                        let mut stream = stream;
                        let _ = write_json(
                            &mut stream,
                            503,
                            &serde_json::json!({"error": "server busy"}),
                        );
                        continue;
                    };
                    thread::spawn(move || {
                        let _ = stream.set_nodelay(true);
                        if let Err(e) = handle_connection(stream, &server) {
                            eprintln!("serve error: {e}");
                        }
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(Error::Io(e)),
            }
        }
        eprintln!("fluctlight serve shutting down — flushing checkpoints");
        self.flush_all_checkpoints()?;
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
struct ApiRequest {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    salience: Option<f32>,
    #[serde(default)]
    cue: Option<String>,
    #[serde(default)]
    semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    n: Option<u64>,
    #[serde(default)]
    magnitude: Option<f32>,
    #[serde(default)]
    engram_id: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    cause: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    query: Option<QueryRequest>,
    #[serde(default)]
    source_uri: Option<String>,
    #[serde(default)]
    doc_id: Option<String>,
    #[serde(default)]
    chunk_id: Option<String>,
    #[serde(default)]
    min_salience: Option<f32>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    verified: Option<bool>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    provenance_kind: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    salience_boost: Option<f32>,
    #[serde(default)]
    supersede_similar: Option<bool>,
    #[serde(default)]
    steps: Option<u32>,
    #[serde(default)]
    batch: Option<Vec<ActivateBatchItem>>,
    /// Consensus: claim value for `key`.
    #[serde(default)]
    value: Option<String>,
    /// Chronos: effect event id (engram uuid string).
    #[serde(default)]
    effect_id: Option<String>,
    /// Chronos: focal event id for preceding/ancestors/bucket queries.
    #[serde(default)]
    event_id: Option<String>,
    /// Chronos: other event id for `before` comparison.
    #[serde(default)]
    other_id: Option<String>,
    #[serde(default)]
    from_tick: Option<u64>,
    #[serde(default)]
    to_tick: Option<u64>,
    #[serde(default)]
    scale: Option<u64>,
    /// Consensus: scoped readers (empty = public).
    #[serde(default)]
    scope: Option<Vec<String>>,
    /// Chronos: on experience, link this cause engram/event id → new engram.
    #[serde(default)]
    caused_by: Option<String>,
    /// Muon Lane: bulk session imprints (haystack replacement).
    #[serde(default)]
    sessions: Option<Vec<crate::muon::MuonImprintInput>>,
    #[serde(default)]
    user_keys: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ActivateBatchItem {
    #[serde(default)]
    cue: String,
    #[serde(default)]
    semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    agent_id: Option<String>,
}

struct HttpRequest<'a> {
    method: &'a str,
    path: &'a str,
    body: &'a str,
    auth: Option<&'a str>,
    idempotency: Option<&'a str>,
}

fn handle_connection(mut stream: TcpStream, server: &BrainServer) -> Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(Error::Io)?;
    loop {
        let req_text = match read_http_request(&mut stream) {
            Ok(t) => t,
            Err(_) => break,
        };
        let keep_alive = request_keep_alive(&req_text);
        if !serve_one_request(&mut stream, server, &req_text, keep_alive)? {
            break;
        }
        if !keep_alive {
            break;
        }
    }
    Ok(())
}

fn serve_one_request(
    stream: &mut TcpStream,
    server: &BrainServer,
    req_text: &str,
    keep_alive: bool,
) -> Result<bool> {
    let parsed = parse_http(req_text)?;

    if parsed.body.len() > MAX_BODY_BYTES {
        write_json_conn(
            stream,
            413,
            &serde_json::json!({"error": "payload too large"}),
            keep_alive,
        )?;
        return Ok(false);
    }

    if parsed.method == "GET" && parsed.path == "/metrics" {
        let body = server.metrics().render_prometheus();
        write_text_conn(stream, 200, "text/plain; version=0.0.4", &body, keep_alive)?;
        return Ok(keep_alive);
    }

    // Built-in Living Brain viewer — the god-tier 3D connectome UI ships with the server.
    if parsed.method == "GET"
        && matches!(
            parsed.path,
            "/" | "/brain" | "/viewer" | "/brain/" | "/viewer/"
        )
    {
        write_text_conn(stream, 200, "text/html; charset=utf-8", VIEWER_HTML, keep_alive)?;
        return Ok(keep_alive);
    }

    if parsed.method == "GET"
        && (parsed.path == "/health"
            || parsed.path == "/api/health"
            || parsed.path == "/api/v1/health")
    {
        write_json_conn(stream, 200, &serde_json::json!({"ok": true}), keep_alive)?;
        return Ok(keep_alive);
    }

    if parsed.method != "POST" {
        write_json_conn(
            stream,
            405,
            &serde_json::json!({"error": "method not allowed"}),
            keep_alive,
        )?;
        return Ok(false);
    }

    let (tenant_from_path, subpath) = split_tenant_path(parsed.path);
    let tenant_hint = tenant_from_path.clone().or_else(|| {
        if parsed.body.trim().is_empty() {
            None
        } else {
            serde_json::from_str::<ApiRequest>(parsed.body)
                .ok()
                .and_then(|b| b.tenant_id)
        }
    });
    let auth_ctx = match server.auth.authorize(parsed.auth, tenant_hint.as_deref()) {
        Some(c) => c,
        None => {
            write_json_conn(
                stream,
                401,
                &serde_json::json!({"error": "unauthorized"}),
                keep_alive,
            )?;
            return Ok(false);
        }
    };

    if let Some(key) = parsed.idempotency {
        let scoped = format!("{}:{}", auth_ctx.tenant_id, key);
        let mut seen = server
            .idempotency
            .write()
            .map_err(|_| Error::Store("idempotency lock poisoned".into()))?;
        if seen.len() >= MAX_IDEMPOTENCY_KEYS {
            seen.clear();
        }
        if !seen.insert(scoped) {
            write_json_conn(
                stream,
                409,
                &serde_json::json!({"error": "duplicate idempotency key"}),
                keep_alive,
            )?;
            return Ok(false);
        }
    }

    let api_body: ApiRequest = if parsed.body.trim().is_empty() {
        ApiRequest::default()
    } else {
        serde_json::from_str(parsed.body).map_err(|e| Error::Serde(e.to_string()))?
    };

    let tenant_id = tenant_from_path
        .or(api_body.tenant_id.clone())
        .unwrap_or(auth_ctx.tenant_id.clone());

    if let Err(e) = enforce_tenant_access(&auth_ctx, &tenant_id) {
        write_json_conn(
            stream,
            403,
            &serde_json::json!({"error": e.to_string()}),
            keep_alive,
        )?;
        return Ok(false);
    }

    if !rate_limit_allow(&tenant_id) {
        write_json_conn(
            stream,
            429,
            &serde_json::json!({"error": "rate limit exceeded", "retry_after_secs": 1}),
            keep_alive,
        )?;
        return Ok(false);
    }

    let path = subpath.as_str();
    let (status, response) = match dispatch(server, &auth_ctx, &tenant_id, path, api_body) {
        Ok(v) => (200, v),
        Err(Error::Store(msg)) if msg == "unauthorized" => {
            write_json_conn(
                stream,
                403,
                &serde_json::json!({"error": "forbidden"}),
                keep_alive,
            )?;
            return Ok(false);
        }
        Err(Error::Store(msg)) if msg == "not found" => {
            write_json_conn(
                stream,
                404,
                &serde_json::json!({"error": "not found", "path": path}),
                keep_alive,
            )?;
            return Ok(false);
        }
        Err(Error::Store(msg)) if msg == "read-only replica" => {
            write_json_conn(
                stream,
                503,
                &serde_json::json!({"error": "read-only replica"}),
                keep_alive,
            )?;
            return Ok(false);
        }
        Err(e) => {
            write_json_conn(
                stream,
                500,
                &serde_json::json!({"error": e.to_string()}),
                keep_alive,
            )?;
            return Ok(false);
        }
    };

    write_json_conn(stream, status, &response, keep_alive)?;
    Ok(keep_alive)
}

fn dispatch(
    server: &BrainServer,
    auth: &AuthContext,
    tenant_id: &str,
    path: &str,
    api_body: ApiRequest,
) -> Result<Value> {
    match path {
        "/api/v1/status" | "/status" => {
            require_role(auth, Role::Read)?;
            server.with_brain_read(tenant_id, |b| Ok(serde_json::to_value(b.status()).unwrap()))
        }
        "/api/v1/replica-status" | "/replica-status" => {
            require_role(auth, Role::Read)?;
            Ok(serde_json::json!({
                "read_only": server.read_only,
                "brain_path": server.default_path.display().to_string(),
            }))
        }
        "/api/v1/experience" | "/experience" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let cfg = tenant_config_for(server, tenant_id);
            let timer = Timer::start();
            let rag = rag_from_api(&api_body);
            let provenance = provenance_from_api(&api_body);
            let episode = Episode {
                content: api_body.content.unwrap_or_default(),
                context: api_body.context.unwrap_or_else(|| "api".into()),
                outcome: api_body.outcome,
                salience_hint: api_body.salience.unwrap_or(0.5),
                semantic_vector: api_body.semantic_vector,
                agent_id: api_body.agent_id.clone(),
                tenant_id: Some(tenant_id.to_string()),
                rag,
                provenance,
            };
            let report: ExperienceReport = server.with_brain_write(tenant_id, |b| {
                enforce_tenant_limits(b, &cfg)?;
                let report = b.experience(episode)?;
                if let Some(cause) = api_body.caused_by.as_deref() {
                    if report.engram_id != uuid::Uuid::nil() {
                        b.chronos_link_cause(cause, &report.engram_id.to_string());
                    }
                }
                Ok(report)
            })?;
            server.metrics.record_experience(timer.elapsed_ms());
            server.metrics.record_tenant_experience(tenant_id);
            Ok(serde_json::to_value(report).unwrap())
        }
        "/api/v1/ingest-chunk" | "/ingest-chunk" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let cfg = tenant_config_for(server, tenant_id);
            let content = api_body
                .content
                .ok_or_else(|| Error::Store("missing content".into()))?;
            let doc_id = api_body.doc_id.clone().unwrap_or_else(|| "document".into());
            let chunk_id = api_body.chunk_id.clone().unwrap_or_else(|| "0".into());
            let timer = Timer::start();
            let report: ExperienceReport = server.with_brain_write(tenant_id, |b| {
                if let Some(existing) = b.hippocampus.find_rag_chunk(&doc_id, &chunk_id) {
                    return Ok(ExperienceReport::dedup(existing));
                }
                enforce_tenant_limits(b, &cfg)?;
                let context = format!("rag:{doc_id}#{chunk_id}");
                b.experience(Episode {
                    content,
                    context,
                    outcome: api_body.outcome.clone(),
                    salience_hint: api_body.salience.unwrap_or(0.55),
                    semantic_vector: api_body.semantic_vector.clone(),
                    agent_id: api_body.agent_id.clone(),
                    tenant_id: Some(tenant_id.to_string()),
                    rag: Some(crate::types::RagRef {
                        source_uri: api_body.source_uri.clone(),
                        doc_id: Some(doc_id),
                        chunk_id: Some(chunk_id),
                    }),
                    provenance: Some(crate::types::Provenance {
                        kind: crate::types::ProvenanceKind::FileObservation,
                        source_uri: api_body.source_uri.clone(),
                        confidence: 0.7,
                        verified: false,
                    }),
                })
            })?;
            server.metrics.record_experience(timer.elapsed_ms());
            server.metrics.record_tenant_experience(tenant_id);
            Ok(serde_json::to_value(report).unwrap())
        }
        "/api/v1/activate-lite" | "/activate-lite" => {
            require_role(auth, Role::Read)?;
            let timer = Timer::start();
            let cue = api_body.cue.unwrap_or_default();
            let agent_id = api_body.agent_id.clone();
            let mut result: ActivationResult = server.with_brain_read(tenant_id, |b| {
                Ok(b.activate_scoped(
                    &cue,
                    api_body.semantic_vector.as_deref(),
                    agent_id.as_deref(),
                    1,
                ))
            })?;
            crate::api_slim::slim_activation_for_api(&mut result, Some(1));
            server
                .metrics
                .record_activate(timer.elapsed_us().max(1) / 1000);
            server.metrics.record_tenant_activate(tenant_id);
            let top = result.recalls.first().map(|r| {
                serde_json::json!({
                    "engram_id": r.engram_id,
                    "activation": r.activation,
                    "verified": r.verified,
                    "content": r.episode.content,
                    "trust_note": r.trust_note,
                })
            });
            Ok(serde_json::json!({
                "cue": cue,
                "top": top,
                "count": result.recalls.len(),
            }))
        }
        "/api/v1/activate" | "/activate" => {
            require_role(auth, Role::Read)?;
            let timer = Timer::start();
            let cue = api_body.cue.unwrap_or_default();
            let agent_id = api_body.agent_id.clone();
            let top_k = api_body
                .limit
                .unwrap_or(crate::api_slim::DEFAULT_API_RECALL_LIMIT);
            let mut result: ActivationResult = server.with_brain_read(tenant_id, |b| {
                Ok(b.activate_scoped(
                    &cue,
                    api_body.semantic_vector.as_deref(),
                    agent_id.as_deref(),
                    top_k,
                ))
            })?;
            crate::api_slim::slim_activation_for_api(&mut result, api_body.limit);
            server
                .metrics
                .record_activate(timer.elapsed_us().max(1) / 1000);
            server.metrics.record_tenant_activate(tenant_id);
            Ok(serde_json::to_value(result).unwrap())
        }
        "/api/v1/activate-batch" | "/activate-batch" => {
            require_role(auth, Role::Read)?;
            let timer = Timer::start();
            let batch = api_body.batch.unwrap_or_default();
            if batch.is_empty() {
                return Err(Error::Store("missing batch".into()));
            }
            if batch.len() > 64 {
                return Err(Error::Store("batch too large (max 64)".into()));
            }
            let items: Vec<(String, Option<Vec<f32>>, Option<String>)> = batch
                .into_iter()
                .map(|b| (b.cue, b.semantic_vector, b.agent_id))
                .collect();
            let top_k = api_body
                .limit
                .unwrap_or(crate::api_slim::DEFAULT_API_RECALL_LIMIT);
            let mut results: Vec<ActivationResult> =
                server.with_brain_read(tenant_id, |b| Ok(b.activate_batch(&items, top_k)))?;
            for result in &mut results {
                crate::api_slim::slim_activation_for_api(result, api_body.limit);
            }
            server
                .metrics
                .record_activate(timer.elapsed_us().max(1) / 1000);
            server.metrics.record_tenant_activate(tenant_id);
            Ok(serde_json::json!({"results": results, "count": results.len()}))
        }
        "/api/v1/complete" | "/complete" => {
            require_role(auth, Role::Read)?;
            let cue = api_body.cue.unwrap_or_default();
            server.with_brain_read(tenant_id, |b| {
                Ok(match b.complete(&cue) {
                    Some(e) => serde_json::to_value(e).unwrap(),
                    None => Value::Null,
                })
            })
        }
        "/api/v1/tick" | "/tick" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let n = api_body.n.unwrap_or(1);
            let reports: Vec<TickReport> = server.with_brain_write(tenant_id, |b| b.tick_n(n))?;
            Ok(serde_json::to_value(reports).unwrap())
        }
        "/api/v1/sleep" | "/sleep" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let metrics = server.metrics.clone();
            server.with_brain_write(tenant_id, |b| {
                let r = b.sleep()?;
                metrics
                    .sleeps
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(serde_json::to_value(r).unwrap())
            })
        }
        "/api/v1/compact" | "/compact" => {
            require_writable(server)?;
            require_role(auth, Role::Admin)?;
            let report: CompactReport = server.with_brain_write(tenant_id, |b| b.compact())?;
            server
                .metrics
                .compactions
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(serde_json::to_value(report).unwrap())
        }
        "/api/v1/reward" | "/reward" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            server.with_brain_write(tenant_id, |b| {
                b.reward(api_body.magnitude.unwrap_or(0.5))?;
                Ok(serde_json::json!({"ok": true}))
            })
        }
        "/api/v1/mark-core" | "/mark-core" => {
            require_writable(server)?;
            require_role(auth, Role::Admin)?;
            let id = Uuid::parse_str(api_body.engram_id.as_deref().unwrap_or(""))
                .map_err(|e| Error::Store(e.to_string()))?;
            server.with_brain_write(tenant_id, |b| {
                b.mark_core(id, api_body.key.unwrap_or_else(|| "core".into()))?;
                Ok(serde_json::json!({"ok": true}))
            })
        }
        "/api/v1/death" | "/death" => {
            require_writable(server)?;
            require_role(auth, Role::Admin)?;
            server.with_brain_write(tenant_id, |b| {
                let new_life = b.death(api_body.cause.as_deref().unwrap_or("api"))?;
                Ok(serde_json::json!({"new_life_id": new_life}))
            })
        }
        "/api/v1/export-viz" | "/export-viz" => {
            require_role(auth, Role::Read)?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::to_value(b.export_viz()).unwrap())
            })
        }
        "/api/v1/timeline" | "/timeline" => {
            require_role(auth, Role::Read)?;
            let limit = api_body.limit.unwrap_or(64).min(512);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "events": b.timeline(limit),
                    "crystals": b.crystal_count(),
                    "chronos_len": b.chronos_len(),
                    "fabric_len": b.fabric_len(),
                    "muon_len": b.muon_len(),
                    "tau_shards": b.tau_shard_len(),
                }))
            })
        }
        "/api/v1/chronos/range" | "/chronos/range" => {
            require_role(auth, Role::Read)?;
            let from = api_body.from_tick.unwrap_or(0);
            let to = api_body.to_tick.unwrap_or(u64::MAX);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "events": b.chronos_events_in_range(from, to),
                    "from_tick": from,
                    "to_tick": to,
                }))
            })
        }
        "/api/v1/chronos/preceding" | "/chronos/preceding" => {
            require_role(auth, Role::Read)?;
            let id = api_body
                .event_id
                .as_deref()
                .or(api_body.engram_id.as_deref())
                .ok_or_else(|| Error::Store("missing event_id or engram_id".into()))?;
            let n = api_body.limit.unwrap_or(8).min(64);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "event_id": id,
                    "preceding": b.chronos_preceding(id, n),
                }))
            })
        }
        "/api/v1/chronos/ancestors" | "/chronos/ancestors" => {
            require_role(auth, Role::Read)?;
            let id = api_body
                .event_id
                .as_deref()
                .or(api_body.effect_id.as_deref())
                .ok_or_else(|| Error::Store("missing event_id or effect_id".into()))?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "effect_id": id,
                    "ancestors": b.chronos_causal_ancestors(id),
                }))
            })
        }
        "/api/v1/chronos/before" | "/chronos/before" => {
            require_role(auth, Role::Read)?;
            let a = api_body
                .event_id
                .as_deref()
                .ok_or_else(|| Error::Store("missing event_id (a)".into()))?;
            let b_id = api_body
                .other_id
                .as_deref()
                .or(api_body.effect_id.as_deref())
                .ok_or_else(|| Error::Store("missing other_id or effect_id (b)".into()))?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "before": b.chronos_before(a, b_id),
                    "a": a,
                    "b": b_id,
                }))
            })
        }
        "/api/v1/chronos/link-cause" | "/chronos/link-cause" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let cause = api_body
                .cause
                .as_deref()
                .or(api_body.event_id.as_deref())
                .ok_or_else(|| Error::Store("missing cause or event_id".into()))?;
            let effect = api_body
                .effect_id
                .as_deref()
                .or(api_body.engram_id.as_deref())
                .ok_or_else(|| Error::Store("missing effect_id or engram_id".into()))?;
            server.with_brain_write(tenant_id, |b| {
                b.chronos_link_cause(cause, effect);
                Ok(serde_json::json!({"ok": true, "cause": cause, "effect": effect}))
            })
        }
        "/api/v1/chronos/bucket" | "/chronos/bucket" => {
            require_role(auth, Role::Read)?;
            let id = api_body
                .event_id
                .as_deref()
                .ok_or_else(|| Error::Store("missing event_id".into()))?;
            let scale = api_body.scale.unwrap_or(1000).max(1);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "event_id": id,
                    "scale": scale,
                    "bucket": b.chronos_bucket(id, scale),
                }))
            })
        }
        "/api/v1/consensus/assert" | "/consensus/assert" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let key = api_body
                .key
                .as_deref()
                .ok_or_else(|| Error::Store("missing key".into()))?;
            let value = api_body
                .value
                .as_deref()
                .ok_or_else(|| Error::Store("missing value".into()))?;
            let agent = api_body
                .agent_id
                .clone()
                .unwrap_or_else(|| "api".into());
            let conf = api_body.confidence.unwrap_or(0.7);
            server.with_brain_write(tenant_id, |b| {
                let tick = b.autonomic.total_ticks;
                let mut claim = crate::consensus::Claim::public(&agent, value, conf, tick);
                if let Some(scope) = api_body.scope.clone() {
                    claim = claim.scoped(scope);
                }
                b.consensus_assert_claim(key, claim);
                Ok(serde_json::json!({"ok": true, "key": key}))
            })
        }
        "/api/v1/consensus/resolve" | "/consensus/resolve" => {
            require_role(auth, Role::Read)?;
            let key = api_body
                .key
                .as_deref()
                .ok_or_else(|| Error::Store("missing key".into()))?;
            let viewer = api_body.agent_id.as_deref();
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "key": key,
                    "consensus": b.consensus_resolve(key, viewer),
                    "contested": b.consensus_is_contested(key, viewer),
                }))
            })
        }
        "/api/v1/consensus/claims" | "/consensus/claims" => {
            require_role(auth, Role::Read)?;
            let key = api_body
                .key
                .as_deref()
                .ok_or_else(|| Error::Store("missing key".into()))?;
            let viewer = api_body.agent_id.as_deref();
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "key": key,
                    "claims": b.consensus_claims(key, viewer),
                }))
            })
        }
        "/api/v1/consensus/contested" | "/consensus/contested" => {
            require_role(auth, Role::Read)?;
            let key = api_body
                .key
                .as_deref()
                .ok_or_else(|| Error::Store("missing key".into()))?;
            let viewer = api_body.agent_id.as_deref();
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "key": key,
                    "contested": b.consensus_is_contested(key, viewer),
                }))
            })
        }
        "/api/v1/muon/imprint" | "/muon/imprint" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            if !crate::muon_runtime::muon_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_MUON=1 required for Muon Lane imprint".into(),
                ));
            }
            let sid = api_body
                .doc_id
                .as_deref()
                .or(api_body.key.as_deref())
                .ok_or_else(|| Error::Store("missing doc_id (session_id)".into()))?;
            let body = api_body
                .content
                .as_deref()
                .ok_or_else(|| Error::Store("missing content (session body)".into()))?;
            let date = api_body.context.as_deref().unwrap_or("");
            let keys = api_body.user_keys.as_deref().unwrap_or("");
            server.with_brain_write(tenant_id, |b| {
                b.muon_imprint(sid, date, body, keys);
                Ok(serde_json::json!({"ok": true, "session_id": sid, "muon_len": b.muon_len()}))
            })
        }
        "/api/v1/muon/imprint-batch" | "/muon/imprint-batch" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            if !crate::muon_runtime::muon_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_MUON=1 required for Muon Lane imprint".into(),
                ));
            }
            let sessions = api_body
                .sessions
                .as_deref()
                .ok_or_else(|| Error::Store("missing sessions array".into()))?;
            server.with_brain_write(tenant_id, |b| {
                let imprinted = b.muon_imprint_batch(sessions);
                Ok(serde_json::json!({
                    "ok": true,
                    "imprinted": imprinted,
                    "muon_len": b.muon_len(),
                }))
            })
        }
        "/api/v1/muon/recall" | "/muon/recall" => {
            require_role(auth, Role::Read)?;
            if !crate::muon_runtime::muon_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_MUON=1 required for Muon Lane recall".into(),
                ));
            }
            let cue = api_body
                .cue
                .as_deref()
                .or(api_body.content.as_deref())
                .ok_or_else(|| Error::Store("missing cue".into()))?;
            let k = api_body.limit.unwrap_or(8).min(64);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "hits": b.muon_recall(cue, k),
                    "muon_len": b.muon_len(),
                }))
            })
        }
        "/api/v1/muon/reel" | "/muon/reel" => {
            require_role(auth, Role::Read)?;
            let sid = api_body
                .doc_id
                .as_deref()
                .or(api_body.event_id.as_deref())
                .ok_or_else(|| Error::Store("missing doc_id (session_id)".into()))?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "session_id": sid,
                    "reel": b.muon_reel(sid),
                }))
            })
        }
        "/api/v1/tau/recall" | "/tau/recall" => {
            require_role(auth, Role::Read)?;
            if !crate::tau_runtime::tau_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_TAU=1 required for Tau Lane recall".into(),
                ));
            }
            let cue = api_body
                .cue
                .as_deref()
                .or(api_body.content.as_deref())
                .ok_or_else(|| Error::Store("missing cue".into()))?;
            let k = api_body.limit.unwrap_or(8).min(64);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "hits": b.tau_recall(cue, k),
                    "session_len": b.muon_len(),
                    "shard_len": b.tau_shard_len(),
                }))
            })
        }
        "/api/v1/tau/crystallize" | "/tau/crystallize" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            if !crate::tau_runtime::tau_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_TAU=1 required for Tau crystallize".into(),
                ));
            }
            let shard_id = api_body
                .key
                .as_deref()
                .or(api_body.event_id.as_deref())
                .ok_or_else(|| Error::Store("missing key (shard_id)".into()))?;
            server.with_brain_write(tenant_id, |b| {
                let eid = b.tau_crystallize_shard(shard_id)?;
                Ok(serde_json::json!({"ok": true, "engram_id": eid.to_string()}))
            })
        }
        "/api/v1/export-graph-lite" | "/export-graph-lite" => {
            require_role(auth, Role::Read)?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::to_value(b.export_graph_lite()).unwrap())
            })
        }
        "/api/v1/export-graph" | "/export-graph" => {
            require_role(auth, Role::Read)?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::to_value(b.export_graph()).unwrap())
            })
        }
        "/api/v1/export-raw" | "/export-raw" => {
            require_role(auth, Role::Read)?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::to_value(b.export_raw()).unwrap())
            })
        }
        "/api/v1/consolidate" | "/consolidate" => {
            require_role(auth, Role::Read)?;
            let min_salience = api_body.min_salience.unwrap_or(0.65);
            let limit = api_body.limit.unwrap_or(20).min(100);
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "memories": b.consolidate_episodes(min_salience, limit),
                }))
            })
        }
        "/api/v1/fovea-read" | "/fovea-read" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let file_path = api_body
                .file_path
                .clone()
                .ok_or_else(|| Error::Store("missing file_path".into()))?;
            let path = PathBuf::from(&file_path);
            if !path.is_file() {
                return Err(Error::Store(format!("file not found: {file_path}")));
            }
            let cfg = crate::fovea::FoveaConfig::default();
            if api_body.dry_run.unwrap_or(false) {
                let packets = crate::fovea::scan_file(&path, &cfg)?;
                return Ok(serde_json::json!({
                    "dry_run": true,
                    "packets": packets.len(),
                    "preview": packets.into_iter().take(5).collect::<Vec<_>>(),
                }));
            }
            let reports = server.with_brain_write(tenant_id, |b| b.fovea_ingest(&path, &cfg))?;
            Ok(serde_json::json!({
                "packets": reports.len(),
                "deduplicated": reports.iter().filter(|r| r.deduplicated).count(),
                "reports": reports,
            }))
        }
        "/api/v1/verify-fact" | "/verify-fact" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let engram_id = api_body
                .engram_id
                .clone()
                .ok_or_else(|| Error::Store("missing engram_id".into()))?;
            let id = Uuid::parse_str(&engram_id)
                .map_err(|e| Error::Store(format!("invalid engram_id: {e}")))?;
            let kind = parse_provenance_kind(api_body.provenance_kind.as_deref());
            server.with_brain_write(tenant_id, |b| {
                b.verify_fact(
                    id,
                    kind,
                    api_body.source_uri.clone(),
                    api_body.confidence.unwrap_or(0.95),
                )
            })?;
            Ok(serde_json::json!({"ok": true, "engram_id": engram_id}))
        }
        "/api/v1/reconsolidate" | "/reconsolidate" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let engram_id = api_body
                .engram_id
                .clone()
                .ok_or_else(|| Error::Store("missing engram_id".into()))?;
            let id = Uuid::parse_str(&engram_id)
                .map_err(|e| Error::Store(format!("invalid engram_id: {e}")))?;
            let report = server.with_brain_write(tenant_id, |b| {
                b.reconsolidate(
                    id,
                    api_body.content.clone(),
                    api_body.outcome.clone(),
                    api_body.salience_boost.unwrap_or(0.2),
                    api_body.semantic_vector.clone(),
                    api_body.supersede_similar.unwrap_or(true),
                )
            })?;
            Ok(serde_json::to_value(report).unwrap())
        }
        "/api/v1/set-goal" | "/set-goal" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let goal = api_body
                .goal
                .clone()
                .ok_or_else(|| Error::Store("missing goal".into()))?;
            server.with_brain_write(tenant_id, |b| {
                b.api_set_goal(goal)?;
                Ok(serde_json::json!({
                    "ok": true,
                    "goals": b.prefrontal.goals,
                }))
            })
        }
        "/api/v1/inhibit" | "/inhibit" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let action = api_body
                .action
                .clone()
                .ok_or_else(|| Error::Store("missing action".into()))?;
            server.with_brain_write(tenant_id, |b| {
                b.api_inhibit(action)?;
                Ok(serde_json::json!({
                    "ok": true,
                    "inhibit_actions": b.prefrontal.inhibit_actions,
                }))
            })
        }
        "/api/v1/preplay" | "/preplay" => {
            require_role(auth, Role::Read)?;
            let goal = api_body.goal.or(api_body.cue.clone()).unwrap_or_default();
            let steps = api_body.steps.unwrap_or(4).min(16);
            let result = server.with_brain_read(tenant_id, |b| Ok(b.preplay(&goal, steps)))?;
            Ok(serde_json::to_value(result).unwrap())
        }
        "/api/v1/neurogenesis" | "/neurogenesis" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            let report = server.with_brain_write(tenant_id, |b| b.neurogenesis_pulse())?;
            Ok(serde_json::to_value(report).unwrap())
        }
        "/api/v1/verified-context" | "/verified-context" => {
            require_role(auth, Role::Read)?;
            let limit = api_body.limit.unwrap_or(12);
            let ctx = server.with_brain_read(tenant_id, |b| Ok(b.verified_context(limit)))?;
            Ok(serde_json::to_value(ctx).unwrap())
        }
        "/api/v1/stage-report" | "/stage-report" => {
            require_role(auth, Role::Read)?;
            let rep = server.with_brain_read(tenant_id, |b| Ok(b.stage_report()))?;
            Ok(serde_json::to_value(rep).unwrap())
        }
        "/api/v1/admin/tenants" | "/admin/tenants" => {
            require_role(auth, Role::Admin)?;
            let store =
                crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())?;
            let tenants = store.list_tenants()?;
            Ok(serde_json::json!({"tenants": tenants}))
        }
        "/api/v1/admin/tenant/provision" | "/admin/tenant/provision" => {
            require_role(auth, Role::Admin)?;
            let tid = api_body
                .tenant_id
                .clone()
                .ok_or_else(|| Error::Store("missing tenant_id".into()))?;
            let cfg = TenantConfig::default_for(&tid, &default_tenant_root());
            cfg.ensure_dirs().map_err(Error::Io)?;
            let _ = FluctlightBrain::open(&cfg.brain_path)?;
            let store =
                crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())?;
            let key = store.issue_key(&tid, Role::Write)?;
            Ok(serde_json::to_value(key).unwrap())
        }
        "/api/v1/admin/tenant/revoke" | "/admin/tenant/revoke" => {
            require_role(auth, Role::Admin)?;
            let kid = api_body
                .kid
                .clone()
                .ok_or_else(|| Error::Store("missing kid".into()))?;
            let store =
                crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())?;
            let removed = store.revoke_key(&kid)?;
            Ok(serde_json::json!({"revoked": removed}))
        }
        "/api/v1/admin/metrics/tenants" | "/admin/metrics/tenants" => {
            require_role(auth, Role::Admin)?;
            let snap = server.metrics().tenant_snapshot();
            Ok(serde_json::json!({
                "tenants": serde_json::to_value(&snap).unwrap_or(Value::Null)
            }))
        }
        "/api/v1/shard/route" | "/shard/route" => {
            require_role(auth, Role::Read)?;
            let tid = api_body
                .tenant_id
                .clone()
                .unwrap_or_else(|| tenant_id.to_string());
            let router = crate::shard::ShardRouter::default();
            Ok(serde_json::json!({
                "tenant_id": tid,
                "shard": router.shard_for(&tid),
                "serve_addr": router.serve_addr(&tid),
            }))
        }
        "/api/v1/query" | "/query" => {
            require_role(auth, Role::Read)?;
            let req = api_body
                .query
                .ok_or_else(|| Error::Store("missing query".into()))?;
            let needs_write = matches!(
                req,
                QueryRequest::Forget { .. } | QueryRequest::ForgetBefore { .. }
            );
            if needs_write {
                require_writable(server)?;
                require_role(auth, Role::Admin)?;
                server.with_brain_write(tenant_id, |b| {
                    Ok(serde_json::to_value(query::execute_mut(b, req)).unwrap())
                })
            } else {
                server.with_brain_read(tenant_id, |b| {
                    Ok(serde_json::to_value(query::execute(b, req)).unwrap())
                })
            }
        }
        _ => Err(Error::Store("not found".into())),
    }
}

fn rag_from_api(api_body: &ApiRequest) -> Option<crate::types::RagRef> {
    if api_body.doc_id.is_none() && api_body.chunk_id.is_none() && api_body.source_uri.is_none() {
        return None;
    }
    Some(crate::types::RagRef {
        source_uri: api_body.source_uri.clone(),
        doc_id: api_body.doc_id.clone(),
        chunk_id: api_body.chunk_id.clone(),
    })
}

fn parse_provenance_kind(s: Option<&str>) -> crate::types::ProvenanceKind {
    match s.unwrap_or("ledger_verified") {
        "chat_assertion" => crate::types::ProvenanceKind::ChatAssertion,
        "file_observation" => crate::types::ProvenanceKind::FileObservation,
        "tool_grounded" => crate::types::ProvenanceKind::ToolGrounded,
        "user_explicit" => crate::types::ProvenanceKind::UserExplicit,
        _ => crate::types::ProvenanceKind::LedgerVerified,
    }
}

fn provenance_from_api(api_body: &ApiRequest) -> Option<crate::types::Provenance> {
    if api_body.verified == Some(true) {
        return Some(crate::types::Provenance {
            kind: parse_provenance_kind(api_body.provenance_kind.as_deref()),
            source_uri: api_body.source_uri.clone(),
            confidence: api_body.confidence.unwrap_or(0.95),
            verified: true,
        });
    }
    if api_body.doc_id.is_some() || api_body.file_path.is_some() {
        return Some(crate::types::Provenance {
            kind: crate::types::ProvenanceKind::FileObservation,
            source_uri: api_body.source_uri.clone(),
            confidence: api_body.confidence.unwrap_or(0.6),
            verified: false,
        });
    }
    Some(crate::types::Provenance {
        kind: crate::types::ProvenanceKind::ChatAssertion,
        source_uri: None,
        confidence: api_body.confidence.unwrap_or(0.35),
        verified: false,
    })
}

fn tenant_config_for(server: &BrainServer, tenant_id: &str) -> TenantConfig {
    if tenant_id == "default" {
        TenantConfig::with_brain_path(
            tenant_id,
            &default_tenant_root(),
            server.default_path.clone(),
        )
    } else {
        TenantConfig::default_for(tenant_id, &default_tenant_root())
    }
}

fn enforce_tenant_limits(brain: &FluctlightBrain, cfg: &TenantConfig) -> Result<()> {
    cfg.check_limits(brain)
}

fn enforce_bind_auth(addr: &str, auth: &AuthConfig) -> Result<()> {
    let host = addr
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(addr)
        .trim_start_matches('[')
        .trim_end_matches(']');
    let localhost = matches!(host, "127.0.0.1" | "localhost" | "::1");
    if !localhost && auth.keys.is_empty() {
        return Err(Error::Store(
            "non-localhost bind requires FLUCTLIGHT_API_KEYS (tenant:key:role,...)".into(),
        ));
    }
    Ok(())
}

fn enforce_tenant_access(auth: &AuthContext, tenant_id: &str) -> Result<()> {
    if auth.role == Role::Admin {
        return Ok(());
    }
    if auth.tenant_id != tenant_id {
        return Err(Error::Store("forbidden tenant".into()));
    }
    Ok(())
}

fn require_writable(server: &BrainServer) -> Result<()> {
    if server.read_only {
        return Err(Error::Store("read-only replica".into()));
    }
    Ok(())
}

fn require_role(auth: &AuthContext, required: Role) -> Result<()> {
    if AuthConfig::check_role(auth, required) {
        Ok(())
    } else {
        Err(Error::Store("unauthorized".into()))
    }
}

fn split_tenant_path(path: &str) -> (Option<String>, String) {
    let prefix = "/api/v1/tenants/";
    if let Some(rest) = path.strip_prefix(prefix) {
        if let Some((tenant, sub)) = rest.split_once('/') {
            let subpath = if sub.starts_with("api/") {
                format!("/{sub}")
            } else {
                format!("/api/v1/{sub}")
            };
            return (Some(tenant.to_string()), subpath);
        }
        return (Some(rest.to_string()), "/api/v1/status".to_string());
    }
    (None, path.to_string())
}

fn rate_limit_allow(tenant_id: &str) -> bool {
    crate::rate_limit::allow(tenant_id)
}

fn read_http_request(stream: &mut TcpStream) -> Result<String> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).map_err(Error::Io)?;
    if n == 0 {
        return Err(Error::Store("empty request".into()));
    }
    let mut req = String::from_utf8_lossy(&buf[..n]).into_owned();
    let content_length = parse_content_length(&req);
    let header_end = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
    let mut body_len = req.len().saturating_sub(header_end);
    while body_len < content_length && req.len() < MAX_BODY_BYTES + 8192 {
        let mut extra = [0u8; 4096];
        let got = stream.read(&mut extra).map_err(Error::Io)?;
        if got == 0 {
            break;
        }
        req.push_str(&String::from_utf8_lossy(&extra[..got]));
        body_len += got;
    }
    Ok(req)
}

fn parse_content_length(raw: &str) -> usize {
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            return rest.trim().parse().unwrap_or(0);
        }
        if line.is_empty() {
            break;
        }
    }
    0
}

fn parse_http(raw: &str) -> Result<HttpRequest<'_>> {
    let mut lines = raw.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| Error::Store("empty request".into()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| Error::Store("no method".into()))?;
    let path = parts.next().ok_or_else(|| Error::Store("no path".into()))?;
    let path = path.split('?').next().unwrap_or(path);

    let mut content_length = 0usize;
    let mut auth = None;
    let mut idempotency = None;
    for line in lines.by_ref() {
        if line.is_empty() {
            break;
        }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
        if let Some(rest) = line.strip_prefix("Authorization:") {
            auth = rest
                .trim()
                .strip_prefix("Bearer ")
                .or_else(|| rest.trim().strip_prefix("bearer "));
        }
        if let Some(rest) = line.strip_prefix("X-Idempotency-Key:") {
            idempotency = Some(rest.trim());
        }
    }
    let body_start = raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(raw.len());
    let body = if content_length > 0 && body_start + content_length <= raw.len() {
        &raw[body_start..body_start + content_length]
    } else if body_start < raw.len() {
        &raw[body_start..]
    } else {
        ""
    };
    Ok(HttpRequest {
        method,
        path,
        body,
        auth,
        idempotency,
    })
}

fn request_keep_alive(raw: &str) -> bool {
    let mut http11 = false;
    if let Some(line) = raw.lines().next() {
        http11 = line.contains("HTTP/1.1");
    }
    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("Connection:") {
            let v = v.trim();
            if v.eq_ignore_ascii_case("close") {
                return false;
            }
            if v.eq_ignore_ascii_case("keep-alive") {
                return true;
            }
        }
        if line.is_empty() {
            break;
        }
    }
    http11
}

fn write_json_conn(
    stream: &mut TcpStream,
    status: u16,
    value: &Value,
    keep_alive: bool,
) -> Result<()> {
    let body = serde_json::to_string(value).map_err(|e| Error::Serde(e.to_string()))?;
    write_text_conn(stream, status, "application/json", &body, keep_alive)
}

fn write_json(stream: &mut TcpStream, status: u16, value: &Value) -> Result<()> {
    write_json_conn(stream, status, value, false)
}

fn write_text_conn(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
    keep_alive: bool,
) -> Result<()> {
    let status_text = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        _ => "Error",
    };
    let retry = if status == 429 {
        "Retry-After: 1\r\n"
    } else {
        ""
    };
    let conn = if keep_alive {
        "Connection: keep-alive\r\n"
    } else {
        "Connection: close\r\n"
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{conn}{retry}\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).map_err(Error::Io)?;
    stream.flush().map_err(Error::Io)?;
    Ok(())
}

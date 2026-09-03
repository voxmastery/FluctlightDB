//! In-process HTTP API — tenant pool, RwLock reads, auth, metrics, v1 contract.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use axum::body::{to_bytes, Body};
use axum::error_handling::HandleErrorLayer;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Request, Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::{BoxError, Router};
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, AtomicOrdering::SeqCst);
}

/// Reset serve shutdown flag between integration tests.
#[doc(hidden)]
pub fn reset_shutdown_for_tests() {
    SHUTDOWN_REQUESTED.store(false, AtomicOrdering::SeqCst);
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
const MAX_HEADERS: usize = 64;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The built-in Living Brain viewer (single-file SPA, no build step).
const VIEWER_HTML: &str = include_str!("viewer.html");
const MAX_IDEMPOTENCY_KEYS: usize = 10_000;
const DEFAULT_HOT_TENANTS: usize = 256;
const DEFAULT_MAX_CONNECTIONS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerMode {
    Production,
    Development,
    Invalid,
}

impl ServerMode {
    fn from_env() -> Self {
        match std::env::var("FLUCTLIGHT_SERVER_MODE").as_deref() {
            Ok(value) if value.eq_ignore_ascii_case("development") => Self::Development,
            Ok(value) if value.eq_ignore_ascii_case("production") => Self::Production,
            Ok(_) => Self::Invalid,
            Err(_) => Self::Production,
        }
    }
}

fn max_connections() -> usize {
    std::env::var("FLUCTLIGHT_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_CONNECTIONS)
        .max(1)
}

fn request_timeout() -> Duration {
    std::env::var("FLUCTLIGHT_REQUEST_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_REQUEST_TIMEOUT)
}

fn max_hot_tenants() -> usize {
    std::env::var("FLUCTLIGHT_MAX_HOT_TENANTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_HOT_TENANTS)
        .max(16)
}

#[cfg(feature = "distributed")]
fn policy_required(policy: crate::placement::DurabilityPolicy, assigned: usize) -> usize {
    match policy {
        crate::placement::DurabilityPolicy::Local => 1,
        crate::placement::DurabilityPolicy::Quorum => assigned / 2 + 1,
        crate::placement::DurabilityPolicy::All => assigned,
    }
}

#[derive(Clone)]
pub struct BrainServer {
    pool: Arc<RwLock<BrainPool>>,
    default_path: PathBuf,
    auth: AuthConfig,
    metrics: Arc<Metrics>,
    idempotency: Arc<RwLock<HashSet<String>>>,
    read_only: bool,
    mode: ServerMode,
    fovea_ingestion: bool,
    dispatch_gate: Arc<tokio::sync::Semaphore>,
    #[cfg(feature = "distributed")]
    applied_control: Option<AppliedControl>,
    #[cfg(feature = "distributed")]
    control_node: Option<Arc<crate::control::service::ControlNode>>,
    #[cfg(feature = "distributed")]
    linearizable_request: bool,
    #[cfg(feature = "distributed")]
    control_ready: Arc<AtomicBool>,
    #[cfg(feature = "distributed")]
    tenant_replication: Option<Arc<TenantReplicationRuntime>>,
}

#[cfg(feature = "distributed")]
#[derive(Clone)]
struct AppliedControl {
    node_id: crate::control::types::NodeId,
    state: Arc<RwLock<crate::control::types::ControlState>>,
    observed_at: Arc<RwLock<SystemTime>>,
}

#[cfg(feature = "distributed")]
struct TenantReplicationRuntime {
    client: crate::replicate::TenantReplicationClient,
    targets: std::collections::BTreeMap<
        crate::control::types::NodeId,
        crate::control::types::NodeMetadata,
    >,
    timeout: Duration,
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
        // Prefer the tenant directory name (…/tenants/<id>/brain) so auth-scoped
        // keys like `serverbrain-v2:…:admin` hit the already-open exclusive brain
        // instead of trying to open the same path a second time (self-deadlock).
        let tenant = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .unwrap_or("default")
            .to_string();
        let mut tenants = HashMap::new();
        let brain = FluctlightBrain::open(&path)?;
        let loaded_mtime = store::snapshot_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);
        let primary = Arc::new(RwLock::new(brain));
        tenants.insert(
            tenant.clone(),
            TenantSlot {
                brain: primary.clone(),
                path: path.clone(),
                loaded_mtime,
                last_access: Instant::now(),
            },
        );
        // "default" always addresses the brain this server was opened on, even
        // when the slot key is the tenant directory name. Without the alias an
        // unauthenticated request would resolve "default" against the global
        // tenant root and open (or read) an unrelated brain.
        if tenant != "default" {
            tenants.insert(
                "default".to_string(),
                TenantSlot {
                    brain: primary,
                    path: path.clone(),
                    loaded_mtime,
                    last_access: Instant::now(),
                },
            );
        }
        let pool = BrainPool {
            tenants,
            tenant_root: default_tenant_root(),
            default_tenant: tenant,
        };
        let metrics = Metrics::new();
        if let Some(slot) = pool.tenants.get(&pool.default_tenant) {
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
            mode: ServerMode::from_env(),
            fovea_ingestion: std::env::var("FLUCTLIGHT_FOVEA_INGESTION")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
            dispatch_gate: Arc::new(tokio::sync::Semaphore::new(max_connections())),
            #[cfg(feature = "distributed")]
            applied_control: None,
            #[cfg(feature = "distributed")]
            control_node: None,
            #[cfg(feature = "distributed")]
            linearizable_request: false,
            #[cfg(feature = "distributed")]
            control_ready: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "distributed")]
            tenant_replication: None,
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

    #[cfg(feature = "distributed")]
    pub fn with_applied_control_state(
        mut self,
        node_id: crate::control::types::NodeId,
        state: crate::control::types::ControlState,
    ) -> Self {
        self.applied_control = Some(AppliedControl {
            node_id,
            state: Arc::new(RwLock::new(state)),
            observed_at: Arc::new(RwLock::new(SystemTime::now())),
        });
        self
    }

    #[cfg(feature = "distributed")]
    pub fn with_tenant_replication(
        mut self,
        client: crate::replicate::TenantReplicationClient,
        targets: std::collections::BTreeMap<
            crate::control::types::NodeId,
            crate::control::types::NodeMetadata,
        >,
        timeout: Duration,
    ) -> Self {
        self.tenant_replication = Some(Arc::new(TenantReplicationRuntime {
            client,
            targets,
            timeout,
        }));
        self
    }

    #[cfg(feature = "distributed")]
    pub fn with_control_node(
        mut self,
        control_node: Arc<crate::control::service::ControlNode>,
    ) -> Result<Self> {
        let node_id = control_node.metadata().node_id;
        let state = control_node.local_state().map_err(Error::Store)?;
        self = self.with_applied_control_state(node_id, state);
        self.control_node = Some(control_node);
        self.linearizable_request = false;
        Ok(self)
    }

    #[cfg(feature = "distributed")]
    pub async fn attach_existing_control_node(
        self,
        control_node: Arc<crate::control::service::ControlNode>,
    ) -> Result<Self> {
        let state = control_node
            .linearizable_read()
            .await
            .map_err(Error::PlacementUnavailable)?;
        let server = self.with_control_node(control_node)?;
        server.update_applied_control_state(state)?;
        server.control_ready.store(true, AtomicOrdering::Release);
        Ok(server)
    }

    #[cfg(feature = "distributed")]
    pub fn control_ready(&self) -> bool {
        self.control_ready.load(AtomicOrdering::Acquire)
    }

    #[cfg(feature = "distributed")]
    pub fn control_node_id(&self) -> Option<crate::control::types::NodeId> {
        self.control_node
            .as_ref()
            .map(|node| node.metadata().node_id)
    }

    #[cfg(feature = "distributed")]
    pub async fn attach_distributed_control(
        self,
        config: crate::control::service::DistributedProductionConfig,
    ) -> Result<Self> {
        crate::control::service::validate_production_ready(&config.node, &config.bootstrap)
            .map_err(Error::PlacementUnavailable)?;
        let bootstrap = config.bootstrap.clone();
        let platform_bootstrap = config.platform_bootstrap.clone();
        let control_node = Arc::new(
            crate::control::service::ControlNode::start(config.node)
                .await
                .map_err(Error::PlacementUnavailable)?,
        );
        match bootstrap {
            crate::control::service::BootstrapMode::Single => {
                control_node
                    .bootstrap_single()
                    .await
                    .map_err(Error::PlacementUnavailable)?;
                match platform_bootstrap {
                    #[cfg(unix)]
                    Some(crate::control::service::PlatformBootstrapSource::File(path)) => {
                        control_node
                            .bootstrap_platform_credential_from_file(&path)
                            .await
                            .map_err(Error::PlacementUnavailable)?;
                    }
                    Some(crate::control::service::PlatformBootstrapSource::Stdin) => {
                        let mut secret = tokio::task::spawn_blocking(|| {
                            let mut secret = String::new();
                            std::io::Read::read_to_string(&mut std::io::stdin(), &mut secret)
                                .map(|_| secret)
                                .map_err(|error| error.to_string())
                        })
                        .await
                        .map_err(|error| Error::PlacementUnavailable(error.to_string()))?
                        .map_err(Error::PlacementUnavailable)?;
                        while secret.ends_with(['\r', '\n']) {
                            secret.pop();
                        }
                        let result = control_node
                            .bootstrap_platform_credential(&secret)
                            .await
                            .map_err(Error::PlacementUnavailable);
                        secret.clear();
                        result?;
                    }
                    #[cfg(not(unix))]
                    Some(crate::control::service::PlatformBootstrapSource::File(_)) => {
                        return Err(Error::PlacementUnavailable(
                            "mode-0600 bootstrap files require Unix; use stdin".into(),
                        ));
                    }
                    None => {
                        return Err(Error::PlacementUnavailable(
                            "single-node control bootstrap requires a one-time platform credential source"
                                .into(),
                        ));
                    }
                }
            }
            crate::control::service::BootstrapMode::Join { seed } => control_node
                .join_cluster(seed)
                .await
                .map_err(Error::PlacementUnavailable)?,
        }
        let state = control_node
            .linearizable_read()
            .await
            .map_err(Error::PlacementUnavailable)?;
        let server = self.with_control_node(control_node)?;
        server.update_applied_control_state(state)?;
        server.control_ready.store(true, AtomicOrdering::Release);
        Ok(server)
    }

    #[cfg(feature = "distributed")]
    pub async fn attach_distributed_control_from_env(self) -> Result<Self> {
        let config = crate::control::service::production_config_from_env()
            .map_err(Error::PlacementUnavailable)?;
        self.attach_distributed_control(config).await
    }

    #[cfg(feature = "distributed")]
    async fn authorize_distributed_request(&self) -> Result<Self> {
        let Some(control_node) = &self.control_node else {
            return Ok(self.clone());
        };
        let state = match control_node.linearizable_read().await {
            Ok(state) => state,
            Err(error) => {
                self.control_ready.store(false, AtomicOrdering::Release);
                return Err(Error::PlacementUnavailable(format!(
                    "control quorum unavailable: {error}"
                )));
            }
        };
        self.update_applied_control_state(state)?;
        self.control_ready.store(true, AtomicOrdering::Release);
        let mut authorized = self.clone();
        authorized.linearizable_request = true;
        Ok(authorized)
    }

    async fn authorize_request_context(
        &self,
        bearer: Option<&str>,
        tenant_hint: Option<&str>,
    ) -> Result<Option<AuthContext>> {
        #[cfg(feature = "distributed")]
        if let Some(control_node) = &self.control_node {
            let Some(secret) = bearer else {
                return Ok(None);
            };
            let now_unix_ms = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX);
            let authorized = control_node
                .authorize_linearizable(secret, now_unix_ms)
                .await
                .map_err(Error::PlacementUnavailable)?;
            return Ok(authorized.map(|key| AuthContext {
                tenant_id: key.tenant_id,
                role: match key.role {
                    crate::control::types::ControlRole::Read => Role::Read,
                    crate::control::types::ControlRole::Write => Role::Write,
                    crate::control::types::ControlRole::Admin => Role::Admin,
                    crate::control::types::ControlRole::Platform => Role::Platform,
                },
            }));
        }
        Ok(self.auth.authorize(bearer, tenant_hint))
    }

    #[cfg(feature = "distributed")]
    pub fn update_applied_control_state(
        &self,
        state: crate::control::types::ControlState,
    ) -> Result<()> {
        let control = self
            .applied_control
            .as_ref()
            .ok_or_else(|| Error::Store("distributed control state is not attached".into()))?;
        *control
            .state
            .write()
            .map_err(|_| Error::Store("control state lock poisoned".into()))? = state;
        *control
            .observed_at
            .write()
            .map_err(|_| Error::Store("control watermark lock poisoned".into()))? =
            SystemTime::now();
        Ok(())
    }

    #[cfg(feature = "distributed")]
    pub fn primary_route(
        &self,
        tenant_id: &str,
    ) -> Result<(crate::control::types::NodeId, String)> {
        let control = self
            .applied_control
            .as_ref()
            .ok_or_else(|| Error::Store("distributed control state is not attached".into()))?;
        let state = control
            .state
            .read()
            .map_err(|_| Error::Store("control state lock poisoned".into()))?;
        let placement = state
            .placements
            .get(tenant_id)
            .ok_or_else(|| Error::Store("tenant placement unavailable".into()))?;
        let primary = placement
            .primary
            .ok_or_else(|| Error::Store("tenant primary unavailable".into()))?;
        let api_addr = state
            .nodes
            .get(&primary)
            .map(|node| node.api_addr.clone())
            .unwrap_or_default();
        Ok((primary, api_addr))
    }

    #[cfg(feature = "distributed")]
    fn distributed_write_identity(
        &self,
        tenant_id: &str,
    ) -> Result<Option<crate::wal::WalIdentity>> {
        let Some(control) = &self.applied_control else {
            return Ok(None);
        };
        if self.control_node.is_some() && !self.linearizable_request {
            return Err(Error::PlacementUnavailable(
                "write requires a fresh linearizable control read".into(),
            ));
        }
        let state = control
            .state
            .read()
            .map_err(|_| Error::Store("control state lock poisoned".into()))?;
        let placement = state
            .placements
            .get(tenant_id)
            .ok_or_else(|| Error::PlacementUnavailable("tenant has no applied placement".into()))?;
        let authorization = placement.authorize_write(&crate::placement::WriteFence {
            tenant_uuid: placement.tenant_uuid,
            node_id: control.node_id,
            generation: placement.generation,
        });
        if let Err(error) = authorization {
            return match error {
                crate::placement::PlacementError::NotPrimary {
                    primary,
                    generation,
                } => {
                    let api_addr = primary
                        .and_then(|node| state.nodes.get(&node))
                        .map(|node| node.api_addr.clone())
                        .filter(|address| address.starts_with("https://"));
                    Err(Error::NotPrimary {
                        primary,
                        generation,
                        api_addr,
                    })
                }
                other => Err(Error::PlacementUnavailable(other.to_string())),
            };
        }
        Ok(Some(crate::wal::WalIdentity {
            tenant_uuid: placement.tenant_uuid,
            writer_epoch: placement.generation,
            fence_generation: placement.generation,
            durability: placement.durability,
        }))
    }

    #[cfg(feature = "distributed")]
    fn distributed_durability_context(
        &self,
        tenant_id: &str,
    ) -> Result<
        Option<(
            crate::wal::WalIdentity,
            crate::placement::Placement,
            crate::control::types::NodeId,
        )>,
    > {
        let Some(control) = &self.applied_control else {
            return Ok(None);
        };
        let state = control
            .state
            .read()
            .map_err(|_| Error::Store("control state lock poisoned".into()))?;
        let placement =
            state.placements.get(tenant_id).cloned().ok_or_else(|| {
                Error::PlacementUnavailable("tenant has no applied placement".into())
            })?;
        let identity = crate::wal::WalIdentity {
            tenant_uuid: placement.tenant_uuid,
            writer_epoch: placement.generation,
            fence_generation: placement.generation,
            durability: placement.durability,
        };
        Ok(Some((identity, placement, control.node_id)))
    }

    #[cfg(feature = "distributed")]
    #[allow(clippy::too_many_arguments)]
    fn replicate_canonical_range(
        &self,
        tenant_id: &str,
        path: &std::path::Path,
        after_seq: u64,
        through_seq: u64,
        identity: crate::wal::WalIdentity,
        placement: &crate::placement::Placement,
        node_id: crate::control::types::NodeId,
    ) -> Result<()> {
        let frames = crate::wal::replication_frames(path, after_seq, through_seq, &identity)?;
        for frame in frames {
            let mutation_hash = frame.sha256;
            let mut acknowledgements = vec![crate::placement::ReplicaDurableAck::exact(
                node_id,
                frame.seq,
                mutation_hash,
            )];
            if placement.durability != crate::placement::DurabilityPolicy::Local {
                let runtime = self.tenant_replication.clone().ok_or_else(|| {
                    Error::DurabilityUnavailable {
                        policy: format!("{:?}", placement.durability),
                        watermark: frame.seq,
                        required: policy_required(placement.durability, placement.members.len()),
                        received: 1,
                    }
                })?;
                let targets: Vec<_> = placement
                    .members
                    .iter()
                    .filter(|member| **member != node_id)
                    .filter_map(|member| {
                        runtime
                            .targets
                            .get(member)
                            .cloned()
                            .map(|target| (*member, target))
                    })
                    .collect();
                let client = runtime.client.clone();
                let timeout = runtime.timeout;
                let replicated_frame = frame.clone();
                let remote = std::thread::spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    Ok::<_, String>(runtime.block_on(async move {
                        let mut durable = Vec::new();
                        for (remote_node, target) in targets {
                            let request = client.apply_wal(&target, vec![replicated_frame.clone()]);
                            if let Ok(Ok(ack)) = tokio::time::timeout(timeout, request).await {
                                if ack.durable_watermark == replicated_frame.seq
                                    && ack.tenant_uuid == replicated_frame.tenant_uuid
                                    && ack.fence_generation == replicated_frame.fence_generation
                                    && ack.operation_id.as_deref()
                                        == Some(replicated_frame.operation_id.as_str())
                                    && ack.mutation_sha256 == Some(replicated_frame.sha256)
                                {
                                    durable.push(remote_node);
                                }
                            }
                        }
                        durable
                    }))
                })
                .join()
                .map_err(|_| Error::Store("tenant replication worker panicked".into()))?
                .map_err(Error::Store)?;
                acknowledgements.extend(remote.into_iter().map(|remote_node| {
                    crate::placement::ReplicaDurableAck::exact(
                        remote_node,
                        frame.seq,
                        mutation_hash,
                    )
                }));
            }
            if crate::placement::evaluate_durable_write(
                placement.durability,
                &placement.members,
                node_id,
                frame.seq,
                mutation_hash,
                &acknowledgements,
            )
            .is_err()
            {
                return Err(Error::DurabilityUnavailable {
                    policy: format!("{:?}", placement.durability),
                    watermark: frame.seq,
                    required: policy_required(placement.durability, placement.members.len()),
                    received: acknowledgements.len(),
                });
            }
            if let Some(control_node) = self.control_node.clone() {
                let reports: Vec<_> = acknowledgements
                    .iter()
                    .map(|ack| (ack.node_id, ack.watermark))
                    .collect();
                let tenant_id = tenant_id.to_string();
                let generation = placement.generation;
                let operation_id = frame.operation_id.clone();
                let state = std::thread::spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    runtime.block_on(async move {
                        for (reported_node, durable_watermark) in reports {
                            let response = control_node
                                .propose(
                                    crate::control::types::ControlCommand::ReportDurableWatermark {
                                        tenant_id: tenant_id.clone(),
                                        request_id: format!(
                                            "durable-{operation_id}-{reported_node}"
                                        ),
                                        node_id: reported_node,
                                        generation,
                                        durable_watermark,
                                    },
                                )
                                .await?;
                            if let crate::control::types::ControlResponse::Rejected { reason } =
                                response
                            {
                                return Err(reason);
                            }
                        }
                        control_node.linearizable_read().await
                    })
                })
                .join()
                .map_err(|_| Error::Store("control watermark reporter panicked".into()))?
                .map_err(Error::PlacementUnavailable)?;
                self.update_applied_control_state(state)?;
            }
        }
        Ok(())
    }

    fn tenant_path(&self, tenant_id: &str, pool: &BrainPool) -> Result<PathBuf> {
        if tenant_id == pool.default_tenant {
            return Ok(self.default_path.clone());
        }
        TenantConfig::try_default_for(tenant_id, &pool.tenant_root)
            .map(|cfg| cfg.brain_path)
            .map_err(Error::Store)
    }

    fn refresh_if_stale(&self, tenant_id: &str) -> Result<()> {
        // Serve holds an exclusive store lock; in-memory state is authoritative.
        // Re-opening the same path while the previous brain Arc still holds
        // brain.lock self-deadlocks the pool (and wedges /ready + all APIs).
        let _ = tenant_id;
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
        let brain_path = self.tenant_path(tenant_id, &pool)?;
        // Reuse an already-open brain for the same path (auth tenant id may differ
        // from the slot key used at serve startup). Never open the same path twice.
        if let Some(existing) = pool
            .tenants
            .values()
            .find(|slot| slot.path == brain_path)
            .map(|slot| (slot.brain.clone(), slot.path.clone(), slot.loaded_mtime))
        {
            let (brain, path, loaded_mtime) = existing;
            pool.tenants.insert(
                tenant_id.to_string(),
                TenantSlot {
                    brain: brain.clone(),
                    path,
                    loaded_mtime,
                    last_access: Instant::now(),
                },
            );
            return Ok(brain);
        }
        self.evict_if_needed(&mut pool);
        if tenant_id == pool.default_tenant {
            // Primary serve path is already ensured by the caller.
        } else {
            TenantConfig::try_default_for(tenant_id, &pool.tenant_root)
                .map_err(Error::Store)?
                .ensure_dirs()
                .map_err(Error::Io)?;
        }
        let brain = FluctlightBrain::open(&brain_path)?;
        let loaded_mtime = store::snapshot_mtime(&brain_path).unwrap_or(SystemTime::UNIX_EPOCH);
        let arc = Arc::new(RwLock::new(brain));
        pool.tenants.insert(
            tenant_id.to_string(),
            TenantSlot {
                brain: arc.clone(),
                path: brain_path,
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

    #[cfg(feature = "distributed")]
    pub fn with_brain_read_consistent<F, T>(
        &self,
        tenant_id: &str,
        consistency: crate::placement::ReadConsistency,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&FluctlightBrain) -> Result<T>,
    {
        let control = self
            .applied_control
            .as_ref()
            .ok_or_else(|| Error::Store("distributed control state is not attached".into()))?;
        let state = control
            .state
            .read()
            .map_err(|_| Error::Store("control state lock poisoned".into()))?;
        let placement = state
            .placements
            .get(tenant_id)
            .ok_or_else(|| Error::Store("tenant placement unavailable".into()))?;
        let local =
            crate::placement::PlacementReconciler::new(control.node_id).reconcile(Some(placement));
        if matches!(
            local.state,
            crate::placement::PlacementState::Absent
                | crate::placement::PlacementState::Draining
                | crate::placement::PlacementState::Staging
        ) {
            return Err(Error::ReadConsistencyUnavailable(
                "local replica is absent, staging, or draining".into(),
            ));
        }
        let observed_at = *control
            .observed_at
            .read()
            .map_err(|_| Error::Store("control watermark lock poisoned".into()))?;
        let follower = placement
            .durable_watermarks
            .get(&control.node_id)
            .copied()
            .map(|durable| crate::placement::FollowerWatermark {
                durable,
                observed_at,
            });
        let primary = local.state == crate::placement::PlacementState::Primary;
        if !consistency.allows(primary, follower, SystemTime::now()) {
            return Err(Error::ReadConsistencyUnavailable(
                "requested watermark, age, or primary ownership is not satisfied".into(),
            ));
        }
        drop(state);
        self.with_brain_read(tenant_id, f)
    }

    pub fn with_brain_write<F, T>(&self, tenant_id: &str, f: F) -> Result<T>
    where
        F: FnOnce(&mut FluctlightBrain) -> Result<T>,
    {
        #[cfg(feature = "distributed")]
        let write_identity = self.distributed_write_identity(tenant_id)?;
        #[cfg(feature = "distributed")]
        let durability_context = self.distributed_durability_context(tenant_id)?;
        let brain = self.get_brain(tenant_id)?;
        let mut guard = brain
            .write()
            .map_err(|_| Error::Store("brain lock poisoned".into()))?;
        #[cfg(feature = "distributed")]
        if write_identity.is_some() {
            guard.set_wal_identity(write_identity);
        }
        #[cfg(feature = "distributed")]
        let before_wal = guard.wal_seq;
        let out = f(&mut guard)?;
        #[cfg(feature = "distributed")]
        let after_wal = guard.wal_seq;
        #[cfg(feature = "distributed")]
        let brain_path = guard.store_path().map(PathBuf::from);
        self.metrics.set_synapses(guard.graph.synapse_count());
        drop(guard);
        #[cfg(feature = "distributed")]
        if let Some((identity, placement, node_id)) = durability_context {
            let path = brain_path
                .ok_or_else(|| Error::Store("distributed brain has no WAL path".into()))?;
            if after_wal <= before_wal {
                return Err(Error::DurabilityUnavailable {
                    policy: format!("{:?}", placement.durability),
                    watermark: before_wal,
                    required: policy_required(placement.durability, placement.members.len()),
                    received: 0,
                });
            }
            self.replicate_canonical_range(
                tenant_id, &path, before_wal, after_wal, identity, &placement, node_id,
            )?;
        }
        self.touch_mtime(tenant_id);
        Ok(out)
    }

    pub fn flush_all_checkpoints(&self) -> Result<()> {
        let brains: Vec<_> = self
            .pool
            .read()
            .map_err(|_| Error::Store("pool lock poisoned".into()))?
            .tenants
            .values()
            .map(|slot| slot.brain.clone())
            .collect();
        for brain in brains {
            if let Ok(guard) = brain.read() {
                let _ = guard.checkpoint();
            }
        }
        Ok(())
    }

    pub fn validate_serve_config(&self, addr: &str) -> Result<()> {
        #[cfg(feature = "distributed")]
        if std::env::var("FLUCTLIGHT_DISTRIBUTED")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        {
            crate::control::service::ensure_production_ready().map_err(Error::Store)?;
        }
        if self.mode == ServerMode::Invalid {
            return Err(Error::Store(
                "FLUCTLIGHT_SERVER_MODE must be 'production' or 'development'".into(),
            ));
        }
        if self.mode == ServerMode::Production && !self.auth.require_auth {
            return Err(Error::Store(
                "production mode requires authentication; set FLUCTLIGHT_REQUIRE_AUTH=true and configure the active authentication source".into(),
            ));
        }
        #[cfg(feature = "distributed")]
        if self.control_node.is_some() {
            return Ok(());
        }
        enforce_bind_auth(addr, &self.auth)
    }

    pub fn serve(&self, addr: &str) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(Error::Io)?;
        let server = self.clone();
        runtime.block_on(async move {
            #[cfg(feature = "distributed")]
            let server = if std::env::var("FLUCTLIGHT_DISTRIBUTED")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                && server.control_node.is_none()
            {
                server.attach_distributed_control_from_env().await?
            } else {
                server
            };
            server.serve_async(addr).await
        })
    }

    pub async fn serve_async(&self, addr: &str) -> Result<()> {
        self.validate_serve_config(addr)?;
        #[cfg(feature = "distributed")]
        if std::env::var("FLUCTLIGHT_DISTRIBUTED")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            && (self.control_node.is_none() || !self.control_ready())
        {
            return Err(Error::PlacementUnavailable(
                "distributed control node is not ready".into(),
            ));
        }
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(Error::Io)?;
        tracing::info!(address = %addr, "fluctlight serve listening");
        eprintln!("fluctlight serve listening on http://{addr}");

        let middleware = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(handle_tower_error))
            .load_shed()
            .concurrency_limit(max_connections())
            .timeout(request_timeout())
            .layer(SetRequestIdLayer::new(
                header::HeaderName::from_static("x-request-id"),
                MakeRequestUuid,
            ))
            .layer(
                TraceLayer::new_for_http().make_span_with(|request: &Request<Body>| {
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("missing");
                    tracing::info_span!(
                        "http_request",
                        request_id,
                        method = %request.method(),
                        uri = %request.uri()
                    )
                }),
            )
            .layer(PropagateRequestIdLayer::x_request_id());
        let app = Router::new()
            .fallback(any(handle_request))
            .with_state(self.clone())
            .layer(middleware);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(Error::Io)?;
        tracing::info!("fluctlight serve drained; flushing checkpoints");
        self.flush_all_checkpoints()
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
    read_consistency: Option<String>,
    #[serde(default)]
    minimum_watermark: Option<u64>,
    #[serde(default)]
    maximum_staleness_ms: Option<u64>,
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

async fn handle_tower_error(error: BoxError) -> impl IntoResponse {
    if error.is::<tower::load_shed::error::Overloaded>() {
        return api_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({"error": "server busy"}).to_string(),
            "application/json",
        );
    }
    if error.is::<tower::timeout::error::Elapsed>() {
        return api_response(
            StatusCode::GATEWAY_TIMEOUT,
            serde_json::json!({"error": "request timeout"}).to_string(),
            "application/json",
        );
    }
    tracing::error!(error = %error, "unhandled serving middleware error");
    api_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        serde_json::json!({"error": "internal server error"}).to_string(),
        "application/json",
    )
}

#[cfg(unix)]
async fn shutdown_signal() {
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    loop {
        if SHUTDOWN_REQUESTED.load(AtomicOrdering::Acquire) {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            _ = tokio::signal::ctrl_c() => {
                request_shutdown();
            }
            _ = async {
                match terminate.as_mut() {
                    Some(signal) => { signal.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                request_shutdown();
            }
        }
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    loop {
        if SHUTDOWN_REQUESTED.load(AtomicOrdering::Acquire) {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            _ = tokio::signal::ctrl_c() => {
                request_shutdown();
            }
        }
    }
}

async fn handle_request(
    State(server): State<BrainServer>,
    request: Request<Body>,
) -> Response<Body> {
    if request.headers().len() > MAX_HEADERS {
        return json_response(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            serde_json::json!({"error": "too many headers"}),
        );
    }
    if request.headers().contains_key(header::CONTENT_LENGTH)
        && request.headers().contains_key(header::TRANSFER_ENCODING)
    {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "ambiguous request framing"}),
        );
    }
    if request
        .headers()
        .get_all(header::TRANSFER_ENCODING)
        .iter()
        .count()
        > 1
    {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "duplicate transfer-encoding"}),
        );
    }
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    #[cfg(feature = "distributed")]
    if method == "GET"
        && matches!(path.as_str(), "/ready" | "/api/v1/ready")
        && server.control_node.is_some()
        && server.authorize_distributed_request().await.is_err()
    {
        return json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({"ready": false}),
        );
    }
    let auth = bearer_token(request.headers()).map(str::to_owned);
    let idempotency = request
        .headers()
        .get("x-idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = match to_bytes(request.into_body(), MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            let status = if error.to_string().contains("length limit") {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            };
            return json_response(
                status,
                serde_json::json!({"error": if status == StatusCode::PAYLOAD_TOO_LARGE {
                    "payload too large"
                } else {
                    "malformed request body"
                }}),
            );
        }
    };
    let body = match String::from_utf8(body.to_vec()) {
        Ok(body) => body,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "request body must be valid UTF-8"}),
            )
        }
    };
    #[cfg(feature = "distributed")]
    let relaxed_follower_read = method == "POST"
        && serde_json::from_str::<ApiRequest>(&body)
            .ok()
            .and_then(|request| request.read_consistency)
            .is_some_and(|mode| matches!(mode.as_str(), "bounded_stale" | "eventual"));
    #[cfg(feature = "distributed")]
    let server = if method == "POST" && !relaxed_follower_read {
        match server.authorize_distributed_request().await {
            Ok(server) => server,
            Err(Error::PlacementUnavailable(reason)) => {
                return json_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    serde_json::json!({
                        "error": "placement_unavailable",
                        "reason": reason,
                    }),
                )
            }
            Err(error) => {
                return json_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    serde_json::json!({"error": error.to_string()}),
                )
            }
        }
    } else {
        server
    };
    let auth_context = match server
        .authorize_request_context(auth.as_deref(), None)
        .await
    {
        Ok(context) => context,
        Err(Error::PlacementUnavailable(reason)) => {
            return json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                serde_json::json!({
                    "error": "placement_unavailable",
                    "reason": reason,
                }),
            )
        }
        Err(error) => {
            return json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                serde_json::json!({"error": error.to_string()}),
            )
        }
    };
    if method != "POST" {
        return process_request(
            &server,
            method.as_str(),
            &path,
            &body,
            auth_context,
            idempotency.as_deref(),
        );
    }
    let permit = match server.dispatch_gate.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                serde_json::json!({"error": "server busy"}),
            )
        }
    };
    let dispatch_server = server.clone();
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        process_request(
            &dispatch_server,
            method.as_str(),
            &path,
            &body,
            auth_context,
            idempotency.as_deref(),
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(error = %error, "business dispatch task failed");
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "business dispatch failed"}),
            )
        }
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
        .filter(|token| !token.is_empty())
}

fn process_request(
    server: &BrainServer,
    method: &str,
    path: &str,
    body: &str,
    auth_context: Option<AuthContext>,
    idempotency: Option<&str>,
) -> Response<Body> {
    if method == "GET"
        && matches!(
            path,
            "/health" | "/api/health" | "/api/v1/health" | "/live" | "/api/v1/live"
        )
    {
        return json_response(
            StatusCode::OK,
            serde_json::json!({"ok": true, "status": "live"}),
        );
    }
    if method == "GET" && matches!(path, "/ready" | "/api/v1/ready") {
        let ready = !SHUTDOWN_REQUESTED.load(AtomicOrdering::Acquire) && server.pool.read().is_ok();
        #[cfg(feature = "distributed")]
        let ready = ready && (server.control_node.is_none() || server.control_ready());
        let status = if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        return json_response(status, serde_json::json!({"ready": ready}));
    }
    if method == "GET" && path == "/metrics" {
        let Some(context) = auth_context else {
            return json_response(
                StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "unauthorized"}),
            );
        };
        if require_role(&context, Role::Platform).is_err() {
            return json_response(
                StatusCode::FORBIDDEN,
                serde_json::json!({"error": "forbidden"}),
            );
        }
        return api_response(
            StatusCode::OK,
            server.metrics().render_prometheus(),
            "text/plain; version=0.0.4",
        );
    }
    if method == "GET" && matches!(path, "/" | "/brain" | "/viewer" | "/brain/" | "/viewer/") {
        return api_response(
            StatusCode::OK,
            VIEWER_HTML.to_string(),
            "text/html; charset=utf-8",
        );
    }
    if method != "POST" {
        return json_response(
            StatusCode::METHOD_NOT_ALLOWED,
            serde_json::json!({"error": "method not allowed"}),
        );
    }

    let (tenant_from_path, subpath) = split_tenant_path(path);
    let _tenant_hint = tenant_from_path.clone().or_else(|| {
        if body.trim().is_empty() {
            None
        } else {
            serde_json::from_str::<ApiRequest>(body)
                .ok()
                .and_then(|b| b.tenant_id)
        }
    });
    let auth_ctx = match auth_context {
        Some(c) => c,
        None => {
            return json_response(
                StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "unauthorized"}),
            )
        }
    };

    if let Some(key) = idempotency {
        let scoped = format!("{}:{}", auth_ctx.tenant_id, key);
        let Ok(mut seen) = server.idempotency.write() else {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "idempotency lock poisoned"}),
            );
        };
        if seen.len() >= MAX_IDEMPOTENCY_KEYS {
            seen.clear();
        }
        if !seen.insert(scoped) {
            return json_response(
                StatusCode::CONFLICT,
                serde_json::json!({"error": "duplicate idempotency key"}),
            );
        }
    }

    let api_body: ApiRequest = if body.trim().is_empty() {
        ApiRequest::default()
    } else {
        match serde_json::from_str(body) {
            Ok(body) => body,
            Err(error) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"error": error.to_string()}),
                )
            }
        }
    };

    let tenant_id = tenant_from_path
        .or(api_body.tenant_id.clone())
        .unwrap_or(auth_ctx.tenant_id.clone());

    if let Err(e) = enforce_tenant_access(&auth_ctx, &tenant_id) {
        return json_response(
            StatusCode::FORBIDDEN,
            serde_json::json!({"error": e.to_string()}),
        );
    }

    if !rate_limit_allow(&tenant_id) {
        let mut response = json_response(
            StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({"error": "rate limit exceeded", "retry_after_secs": 1}),
        );
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        return response;
    }

    let path = subpath.as_str();
    match dispatch(server, &auth_ctx, &tenant_id, path, api_body) {
        Ok(value) => json_response(StatusCode::OK, value),
        Err(Error::Store(message)) if message == "unauthorized" => json_response(
            StatusCode::FORBIDDEN,
            serde_json::json!({"error": "forbidden"}),
        ),
        Err(Error::Store(message)) if message == "not found" => json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({"error": "not found", "path": path}),
        ),
        Err(Error::Store(message)) if message == "read-only replica" => json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({"error": "read-only replica"}),
        ),
        Err(Error::Store(message)) if message == "fovea ingestion disabled" => {
            json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": message}))
        }
        Err(Error::NotPrimary {
            primary,
            generation,
            api_addr,
        }) => {
            let mut response = json_response(
                if api_addr.is_some() {
                    StatusCode::TEMPORARY_REDIRECT
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                },
                serde_json::json!({
                    "error": "not_primary",
                    "primary_node_id": primary,
                    "placement_generation": generation,
                    "primary_api": api_addr,
                }),
            );
            if let Some(address) = api_addr {
                if let Ok(location) = HeaderValue::from_str(&format!("{address}{path}")) {
                    response.headers_mut().insert(header::LOCATION, location);
                }
            }
            response
        }
        Err(Error::PlacementUnavailable(reason)) => json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({"error": "placement_unavailable", "reason": reason}),
        ),
        Err(Error::ReadConsistencyUnavailable(reason)) => json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({"error": "read_consistency_unavailable", "reason": reason}),
        ),
        Err(Error::DurabilityUnavailable {
            policy,
            watermark,
            required,
            received,
        }) => json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": "durability_unavailable",
                "policy": policy,
                "watermark": watermark,
                "required": required,
                "received": received,
            }),
        ),
        Err(Error::DistributedMutationDisabled { operation }) => json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": "distributed_mutation_disabled",
                "operation": operation,
            }),
        ),
        Err(error) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": error.to_string()}),
        ),
    }
}

fn json_response(status: StatusCode, value: Value) -> Response<Body> {
    api_response(status, value.to_string(), "application/json")
}

fn api_response(status: StatusCode, body: String, content_type: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .expect("valid API response")
}

fn requested_read_consistency(request: &ApiRequest) -> Result<crate::placement::ReadConsistency> {
    match request.read_consistency.as_deref().unwrap_or("primary") {
        "primary" | "linearizable" => Ok(crate::placement::ReadConsistency::Primary),
        "bounded_stale" => Ok(crate::placement::ReadConsistency::BoundedStale {
            minimum_watermark: request.minimum_watermark.unwrap_or_default(),
            maximum_age: Duration::from_millis(request.maximum_staleness_ms.unwrap_or_default()),
        }),
        "eventual" => Ok(crate::placement::ReadConsistency::Eventual),
        other => Err(Error::ReadConsistencyUnavailable(format!(
            "unknown read consistency mode {other:?}"
        ))),
    }
}

fn with_brain_read_mode<F, T>(
    server: &BrainServer,
    tenant_id: &str,
    _consistency: crate::placement::ReadConsistency,
    read: F,
) -> Result<T>
where
    F: FnOnce(&FluctlightBrain) -> Result<T>,
{
    #[cfg(feature = "distributed")]
    if server.applied_control.is_some() {
        return server.with_brain_read_consistent(tenant_id, _consistency, read);
    }
    server.with_brain_read(tenant_id, read)
}

fn with_api_brain_read<F, T>(
    server: &BrainServer,
    tenant_id: &str,
    request: &ApiRequest,
    read: F,
) -> Result<T>
where
    F: FnOnce(&FluctlightBrain) -> Result<T>,
{
    with_brain_read_mode(
        server,
        tenant_id,
        requested_read_consistency(request)?,
        read,
    )
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
            with_api_brain_read(server, tenant_id, &api_body, |b| {
                Ok(serde_json::to_value(b.status()).unwrap())
            })
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
            let cfg = tenant_config_for(server, tenant_id)?;
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
            let cfg = tenant_config_for(server, tenant_id)?;
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
            let consistency = requested_read_consistency(&api_body)?;
            let cue = api_body.cue.unwrap_or_default();
            let agent_id = api_body.agent_id.clone();
            let top_k = api_body
                .limit
                .unwrap_or(crate::api_slim::DEFAULT_API_RECALL_LIMIT);
            let mut result: ActivationResult =
                with_brain_read_mode(server, tenant_id, consistency, |b| {
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
            let agent = api_body.agent_id.clone().unwrap_or_else(|| "api".into());
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
        // ── CHORUS lane ──────────────────────────────────────────────────────────────
        // The MaxSim⊕BM25 late-interaction stack was reachable only through the native/SDK
        // API, which is unusable while `serve` holds the exclusive brain lock — so any
        // client talking HTTP could not use it at all. These endpoints expose it.
        "/api/v1/chorus/imprint" | "/chorus/imprint" => {
            require_writable(server)?;
            require_role(auth, Role::Write)?;
            if !crate::chorus_runtime::chorus_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_CHORUS=1 required for CHORUS imprint".into(),
                ));
            }
            let content = api_body
                .content
                .as_deref()
                .ok_or_else(|| Error::Store("missing content".into()))?;
            let memory_id = api_body
                .doc_id
                .as_deref()
                .or(api_body.key.as_deref())
                .ok_or_else(|| Error::Store("missing doc_id (memory_id)".into()))?;
            let input = crate::chorus::ChorusImprintInput {
                memory_id: memory_id.to_string(),
                content: content.to_string(),
                context: api_body.context.clone().unwrap_or_default(),
                semantic_vector: api_body.semantic_vector.clone(),
                token_vectors: None,
                salience: api_body.salience.unwrap_or(0.7),
                sheath: Default::default(),
            };
            server.with_brain_write(tenant_id, |b| {
                let ok = b.chorus_imprint(&input);
                Ok(serde_json::json!({
                    "ok": ok,
                    "memory_id": memory_id,
                    "chorus_len": b.chorus_len(),
                }))
            })
        }
        "/api/v1/chorus/recall" | "/chorus/recall" => {
            require_role(auth, Role::Read)?;
            if !crate::chorus_runtime::chorus_enabled() {
                return Err(Error::Store(
                    "FLUCTLIGHT_CHORUS=1 required for CHORUS recall".into(),
                ));
            }
            let cue = api_body
                .cue
                .as_deref()
                .or(api_body.content.as_deref())
                .ok_or_else(|| Error::Store("missing cue".into()))?;
            let k = api_body.limit.unwrap_or(8).min(64);
            let cue_vector = api_body.semantic_vector.as_deref();
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "hits": b.chorus_recall(cue, k, cue_vector),
                    "chorus_len": b.chorus_len(),
                }))
            })
        }
        "/api/v1/chorus/stats" | "/chorus/stats" => {
            require_role(auth, Role::Read)?;
            server.with_brain_read(tenant_id, |b| {
                Ok(serde_json::json!({
                    "enabled": crate::chorus_runtime::chorus_enabled(),
                    "chorus_len": b.chorus_len(),
                }))
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
            if !server.fovea_ingestion {
                return Err(Error::Store("fovea ingestion disabled".into()));
            }
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
                let goal_texts: Vec<&str> =
                    b.prefrontal.goals.iter().map(|g| g.text.as_str()).collect();
                Ok(serde_json::json!({
                    "ok": true,
                    "goals": goal_texts,
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
                let inhibit_patterns: Vec<&str> = b
                    .prefrontal
                    .inhibit_patterns
                    .iter()
                    .map(|i| i.pattern.as_str())
                    .collect();
                Ok(serde_json::json!({
                    "ok": true,
                    "inhibit_actions": inhibit_patterns,
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
            require_role(auth, Role::Platform)?;
            #[cfg(feature = "distributed")]
            if let Some(control) = &server.applied_control {
                let state = control
                    .state
                    .read()
                    .map_err(|_| Error::Store("control state lock poisoned".into()))?;
                return Ok(serde_json::json!({
                    "tenants": state.tenants.keys().collect::<Vec<_>>()
                }));
            }
            let store =
                crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())?;
            let tenants = store.list_tenants()?;
            Ok(serde_json::json!({"tenants": tenants}))
        }
        "/api/v1/admin/tenant/provision" | "/admin/tenant/provision" => {
            require_role(auth, Role::Platform)?;
            let tid = api_body
                .tenant_id
                .clone()
                .ok_or_else(|| Error::Store("missing tenant_id".into()))?;
            let cfg = TenantConfig::try_default_for(&tid, &default_tenant_root())
                .map_err(Error::Store)?;
            cfg.ensure_dirs().map_err(Error::Io)?;
            let _ = FluctlightBrain::open(&cfg.brain_path)?;
            #[cfg(feature = "distributed")]
            if let Some(control_node) = &server.control_node {
                let control_node = Arc::clone(control_node);
                let tenant_id = tid.clone();
                let issued = tokio::runtime::Handle::current().block_on(async move {
                    let state = control_node.linearizable_read().await?;
                    if !state.tenants.contains_key(&tenant_id) {
                        match control_node
                            .propose(crate::control::types::ControlCommand::CreateTenant {
                                tenant_id: tenant_id.clone(),
                                request_id: format!("provision-tenant-{tenant_id}"),
                                config: crate::control::types::TenantControlConfig::default(),
                            })
                            .await?
                        {
                            crate::control::types::ControlResponse::Applied { .. }
                            | crate::control::types::ControlResponse::AlreadyApplied { .. } => {}
                            crate::control::types::ControlResponse::Rejected { reason } => {
                                return Err(reason);
                            }
                        }
                    }
                    control_node
                        .issue_credential(
                            &tenant_id,
                            crate::control::types::ControlRole::Write,
                            None,
                        )
                        .await
                });
                let issued = issued.map_err(Error::Store)?;
                return Ok(serde_json::json!({
                    "kid": issued.metadata.key_id,
                    "tenant_id": issued.metadata.tenant_id,
                    "key": issued.secret,
                    "role": "write",
                    "created_at": issued.metadata.created_at_unix_ms,
                    "expires_at": issued.metadata.expires_at_unix_ms,
                    "revoked": false,
                }));
            }
            let store =
                crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())?;
            let key = store.issue_key(&tid, Role::Write)?;
            Ok(serde_json::to_value(key).unwrap())
        }
        "/api/v1/admin/tenant/revoke" | "/admin/tenant/revoke" => {
            require_role(auth, Role::Platform)?;
            let kid = api_body
                .kid
                .clone()
                .ok_or_else(|| Error::Store("missing kid".into()))?;
            #[cfg(feature = "distributed")]
            if let Some(control_node) = &server.control_node {
                let now_unix_ms = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                let control_node = Arc::clone(control_node);
                tokio::runtime::Handle::current()
                    .block_on(control_node.revoke_credential(&kid, now_unix_ms))
                    .map_err(Error::Store)?;
                return Ok(serde_json::json!({"revoked": true}));
            }
            let store =
                crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())?;
            let removed = store.revoke_key(&kid)?;
            Ok(serde_json::json!({"revoked": removed}))
        }
        "/api/v1/admin/metrics/tenants" | "/admin/metrics/tenants" => {
            require_role(auth, Role::Platform)?;
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
            #[cfg(feature = "distributed")]
            if server.applied_control.is_some() {
                let (primary_node_id, primary_api) = server.primary_route(&tid)?;
                let generation = server
                    .applied_control
                    .as_ref()
                    .and_then(|control| control.state.read().ok())
                    .and_then(|state| state.placements.get(&tid).map(|p| p.generation))
                    .ok_or_else(|| {
                        Error::PlacementUnavailable("tenant placement unavailable".into())
                    })?;
                return Ok(serde_json::json!({
                    "tenant_id": tid,
                    "primary_node_id": primary_node_id,
                    "primary_api": primary_api,
                    "placement_generation": generation,
                }));
            }
            Ok(serde_json::json!({
                "tenant_id": tid,
                "mode": "local",
                "primary": true,
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

fn tenant_config_for(server: &BrainServer, tenant_id: &str) -> Result<TenantConfig> {
    if tenant_id == "default" {
        Ok(TenantConfig::with_brain_path(
            tenant_id,
            &default_tenant_root(),
            server.default_path.clone(),
        ))
    } else {
        TenantConfig::try_default_for(tenant_id, &default_tenant_root()).map_err(Error::Store)
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
    // CAB: Platform may name any BrainId on control-plane routes; brain writes still
    // require encode/govern via require_role (Platform does not allow Write/Admin).
    if auth.role == Role::Platform {
        return Ok(());
    }
    // Admin/Write/Read: always bound to key tenant (no ambient cross-tenant Admin).
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

#[cfg(all(test, feature = "distributed"))]
mod placement_tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::control::types::ControlState;
    use crate::placement::{DurabilityPolicy, Placement};
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
        KeyUsagePurpose,
    };

    fn control_identity(node_id: u64) -> crate::control::network::TlsIdentity {
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
        ];
        let ca_key = KeyPair::generate().unwrap();
        let ca = ca_params.self_signed(&ca_key).unwrap();
        let mut node_params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        node_params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let node_key = KeyPair::generate().unwrap();
        let node = node_params.signed_by(&node_key, &ca, &ca_key).unwrap();
        crate::control::network::TlsIdentity {
            node_id,
            certificate_chain_der: vec![node.der().to_vec()],
            private_key_der: node_key.serialize_der(),
            ca_certificate_der: ca.der().to_vec(),
            server_name: "localhost".into(),
        }
    }

    #[cfg(unix)]
    fn bootstrap_file(dir: &std::path::Path, name: &str, secret: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join(name);
        std::fs::write(&path, secret).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    #[test]
    fn distributed_write_checks_applied_primary_before_entering_brain_closure() {
        let dir = tempfile::tempdir().unwrap();
        let server = BrainServer::open(dir.path().join("brain.flct")).unwrap();
        let tenant_uuid = uuid::Uuid::from_u128(123);
        let mut state = ControlState::default();
        state.placements.insert(
            "default".into(),
            Placement {
                tenant_uuid,
                generation: 4,
                primary: Some(2),
                members: BTreeSet::from([1, 2]),
                draining: BTreeSet::new(),
                durable_watermarks: BTreeMap::from([(1, 0), (2, 0)]),
                committed_watermark: 0,
                durability: DurabilityPolicy::Quorum,
            },
        );
        let server = server.with_applied_control_state(1, state);
        let entered = AtomicBool::new(false);
        let error = server
            .with_brain_write("default", |_| {
                entered.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap_err();
        assert!(!entered.load(Ordering::SeqCst));
        assert!(error.to_string().contains("not primary"), "{error}");
        assert_eq!(server.primary_route("default").unwrap().0, 2);
        server
            .with_brain_read_consistent(
                "default",
                crate::placement::ReadConsistency::BoundedStale {
                    minimum_watermark: 0,
                    maximum_age: Duration::from_secs(5),
                },
                |_| Ok(()),
            )
            .unwrap();
        assert!(server
            .with_brain_read_consistent(
                "default",
                crate::placement::ReadConsistency::Primary,
                |_| Ok(()),
            )
            .unwrap_err()
            .to_string()
            .contains("read consistency"));
    }

    #[test]
    fn distributed_server_accepts_a_live_control_node_source() {
        fn accepts(
            _method: fn(
                BrainServer,
                Arc<crate::control::service::ControlNode>,
            ) -> Result<BrainServer>,
        ) {
        }
        accepts(BrainServer::with_control_node);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_or_environment_key_is_rejected_when_control_node_is_attached() {
        use crate::control::service::{
            BootstrapMode, ControlNodeConfig, DistributedProductionConfig, PlatformBootstrapSource,
        };

        let env =
            crate::test_env::EnvGuard::acquire(&["FLUCTLIGHT_API_KEYS", "FLUCTLIGHT_REQUIRE_AUTH"]);
        env.set("FLUCTLIGHT_API_KEYS", "default:local-only:admin");
        env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
        let dir = tempfile::tempdir().unwrap();
        let standalone = BrainServer::open(dir.path().join("standalone")).unwrap();
        assert!(standalone
            .authorize_request_context(Some("local-only"), None)
            .await
            .unwrap()
            .is_some());

        let distributed = BrainServer::open(dir.path().join("distributed"))
            .unwrap()
            .attach_distributed_control(DistributedProductionConfig {
                node: ControlNodeConfig {
                    node_id: 17,
                    bind_addr: "127.0.0.1:0".into(),
                    data_dir: dir.path().join("control"),
                    cluster_pepper: vec![17; 32],
                    tls_identity: control_identity(17),
                    cluster_name: "distributed-auth-source-test".into(),
                },
                bootstrap: BootstrapMode::Single,
                platform_bootstrap: Some(PlatformBootstrapSource::File(bootstrap_file(
                    dir.path(),
                    "auth-bootstrap",
                    "control-platform-key",
                ))),
            })
            .await
            .unwrap();

        assert!(distributed
            .authorize_request_context(Some("local-only"), None)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn automatic_distributed_startup_fails_closed_on_invalid_config() {
        use crate::control::network::TlsIdentity;
        use crate::control::service::{
            BootstrapMode, ControlNodeConfig, DistributedProductionConfig,
        };

        let dir = tempfile::tempdir().unwrap();
        let server = BrainServer::open(dir.path().join("brain.flct")).unwrap();
        assert!(!server.control_ready());
        let config = DistributedProductionConfig {
            node: ControlNodeConfig {
                node_id: 1,
                bind_addr: "127.0.0.1:0".into(),
                data_dir: dir.path().join("control"),
                cluster_pepper: Vec::new(),
                tls_identity: TlsIdentity {
                    node_id: 1,
                    certificate_chain_der: Vec::new(),
                    private_key_der: Vec::new(),
                    ca_certificate_der: Vec::new(),
                    server_name: String::new(),
                },
                cluster_name: "phase-3".into(),
            },
            bootstrap: BootstrapMode::Single,
            platform_bootstrap: None,
        };

        let error = match server.attach_distributed_control(config).await {
            Ok(_) => panic!("invalid distributed config must not become ready"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("pepper"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn valid_config_automatically_starts_control_and_marks_ready() {
        use crate::control::service::{
            BootstrapMode, ControlNodeConfig, DistributedProductionConfig, PlatformBootstrapSource,
        };

        let dir = tempfile::tempdir().unwrap();
        let server = BrainServer::open(dir.path().join("brain.flct")).unwrap();
        let server = server
            .attach_distributed_control(DistributedProductionConfig {
                node: ControlNodeConfig {
                    node_id: 9,
                    bind_addr: "127.0.0.1:0".into(),
                    data_dir: dir.path().join("control"),
                    cluster_pepper: vec![9; 32],
                    tls_identity: control_identity(9),
                    cluster_name: "phase-3-startup".into(),
                },
                bootstrap: BootstrapMode::Single,
                platform_bootstrap: Some(PlatformBootstrapSource::File(bootstrap_file(
                    dir.path(),
                    "valid-bootstrap",
                    "valid-platform-key",
                ))),
            })
            .await
            .unwrap();

        assert!(server.control_ready());
        assert_eq!(server.control_node_id(), Some(9));
    }
}

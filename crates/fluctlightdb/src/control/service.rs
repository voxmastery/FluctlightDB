use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use openraft::{Config, Raft};
use tokio::sync::Mutex;

use super::network::{
    certificate_fingerprint, ControlRpcRequest, ControlRpcResponse, MtlsRaftNetworkFactory,
    MtlsRpcClient, MtlsRpcServer, PeerIdentityRegistry, RaftRpcRequest, RaftRpcResponse,
    RpcHandler, TlsIdentity,
};
use super::state_machine::{AuthorizedKey, ControlStateMachine, IssuedKey, KeyIssuer};
use super::storage::SqliteControlStore;
use super::types::{
    ControlCommand, ControlResponse, ControlRole, ControlState, ControlTypeConfig, NodeId,
    NodeMetadata,
};

#[derive(Debug, Clone)]
pub struct ControlNodeConfig {
    pub node_id: NodeId,
    pub bind_addr: String,
    pub data_dir: PathBuf,
    pub cluster_pepper: Vec<u8>,
    pub tls_identity: TlsIdentity,
    pub cluster_name: String,
}

impl ControlNodeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.node_id == 0 {
            return Err("distributed node id must be non-zero".to_string());
        }
        if self.node_id != self.tls_identity.node_id {
            return Err("TLS identity node id does not match configured node id".to_string());
        }
        if self.bind_addr.is_empty() {
            return Err("distributed Raft bind address is required".to_string());
        }
        if self.cluster_name.is_empty() {
            return Err("distributed cluster name is required".to_string());
        }
        if self.cluster_pepper.len() != 32 {
            return Err("cluster pepper must be exactly 32 bytes".to_string());
        }
        self.tls_identity.validate()
    }
}

struct ControlNodeInner {
    node_id: NodeId,
    raft: Raft<ControlTypeConfig>,
    store: SqliteControlStore,
    client: MtlsRpcClient,
    peers: PeerIdentityRegistry,
    metadata: RwLock<NodeMetadata>,
    membership_change: Mutex<()>,
}

pub struct ControlNode {
    inner: Arc<ControlNodeInner>,
    server: Mutex<Option<MtlsRpcServer>>,
}

impl ControlNode {
    pub async fn start(config: ControlNodeConfig) -> Result<Self, String> {
        config.validate()?;
        std::fs::create_dir_all(&config.data_dir).map_err(|error| error.to_string())?;
        let store = SqliteControlStore::open(
            config.data_dir.join("control.sqlite"),
            config.data_dir.join("snapshots"),
            &config.cluster_pepper,
        )?;
        let (log_store, state_machine) = store.split()?;
        let network = MtlsRaftNetworkFactory::new(config.tls_identity.clone())?;
        let raft_config = Arc::new(
            Config {
                cluster_name: config.cluster_name,
                ..Config::default()
            }
            .validate()
            .map_err(|error| error.to_string())?,
        );
        let raft = Raft::new(
            config.node_id,
            raft_config,
            network,
            log_store,
            state_machine,
        )
        .await
        .map_err(|error| error.to_string())?;
        let peers = PeerIdentityRegistry::new();
        let client = MtlsRpcClient::new(config.tls_identity.clone())?;
        let inner = Arc::new(ControlNodeInner {
            node_id: config.node_id,
            raft,
            store,
            client,
            peers: peers.clone(),
            metadata: RwLock::new(NodeMetadata::default()),
            membership_change: Mutex::new(()),
        });
        let server = MtlsRpcServer::start(
            &config.bind_addr,
            config.tls_identity.clone(),
            peers,
            Arc::new(ControlRpcHandler {
                inner: Arc::clone(&inner),
            }),
        )
        .await?;
        let metadata = NodeMetadata {
            node_id: config.node_id,
            raft_addr: server.local_addr().to_string(),
            api_addr: String::new(),
            certificate_sha256: certificate_fingerprint(
                &config.tls_identity.certificate_chain_der[0],
            ),
        };
        inner.peers.register(metadata.clone());
        *inner
            .metadata
            .write()
            .map_err(|_| "node metadata lock poisoned".to_string())? = metadata;
        Ok(Self {
            inner,
            server: Mutex::new(Some(server)),
        })
    }

    pub fn metadata(&self) -> NodeMetadata {
        self.inner
            .metadata
            .read()
            .map(|metadata| metadata.clone())
            .unwrap_or_default()
    }

    pub fn register_peers(&self, peers: impl IntoIterator<Item = NodeMetadata>) {
        let mut peers: Vec<_> = peers.into_iter().collect();
        if !peers.iter().any(|node| node.node_id == self.inner.node_id) {
            peers.push(self.metadata());
        }
        self.inner.peers.replace(peers);
    }

    pub async fn bootstrap_single(&self) -> Result<(), String> {
        let metadata = self.metadata();
        self.inner
            .raft
            .initialize(BTreeMap::from([(self.inner.node_id, metadata.clone())]))
            .await
            .map_err(|error| error.to_string())?;
        self.inner
            .raft
            .wait(Some(Duration::from_secs(5)))
            .current_leader(self.inner.node_id, "wait for bootstrap leader")
            .await
            .map_err(|error| error.to_string())?;
        self.propose(ControlCommand::RegisterNode {
            request_id: format!("bootstrap-register-node-{}", self.inner.node_id),
            node: metadata,
        })
        .await?;
        Ok(())
    }

    pub async fn join_cluster(&self, seed: NodeMetadata) -> Result<(), String> {
        if seed.node_id == 0 || seed.raft_addr.is_empty() || seed.certificate_sha256 == [0; 32] {
            return Err("join seed metadata is incomplete".to_string());
        }
        self.inner.peers.register(seed.clone());
        self.inner.peers.register(self.metadata());
        match send_control(
            &self.inner,
            &seed,
            ControlRpcRequest::AddLearner(self.metadata()),
        )
        .await?
        {
            ControlRpcResponse::Operation(result) => result,
            _ => Err("unexpected cluster-join response".to_string()),
        }
    }

    pub async fn propose(&self, command: ControlCommand) -> Result<ControlResponse, String> {
        propose_inner(&self.inner, command).await
    }

    pub async fn linearizable_read(&self) -> Result<ControlState, String> {
        linearizable_read_inner(&self.inner).await
    }

    pub async fn authorize_linearizable(
        &self,
        secret: &str,
        now_unix_ms: u64,
    ) -> Result<Option<AuthorizedKey>, String> {
        let state = linearizable_read_inner(&self.inner).await?;
        Ok(
            ControlStateMachine::from_state(self.inner.store.pepper(), state)?
                .authorize(secret, now_unix_ms),
        )
    }

    pub async fn bootstrap_platform_credential(&self, secret: &str) -> Result<u64, String> {
        if secret.is_empty() {
            return Err("platform bootstrap secret must not be empty".to_string());
        }
        let state = self.linearizable_read().await?;
        if state.credential_bootstrap_completed {
            return Err("platform credential bootstrap is already completed".to_string());
        }
        let now_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let metadata = KeyIssuer::new(self.inner.store.pepper())?.metadata_for_secret(
            "platform-bootstrap",
            "platform",
            ControlRole::Platform,
            now_unix_ms,
            None,
            secret,
        )?;
        match self
            .propose(ControlCommand::BootstrapPlatformKey {
                request_id: format!("platform-bootstrap-{}", uuid::Uuid::new_v4().simple()),
                metadata,
            })
            .await?
        {
            ControlResponse::Applied { revision } => Ok(revision),
            ControlResponse::AlreadyApplied { .. } => {
                Err("platform credential bootstrap was already attempted".to_string())
            }
            ControlResponse::Rejected { reason } => Err(reason),
        }
    }

    pub async fn issue_credential(
        &self,
        tenant_id: &str,
        role: ControlRole,
        expires_at_unix_ms: Option<u64>,
    ) -> Result<IssuedKey, String> {
        let now_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let issued = KeyIssuer::new(self.inner.store.pepper())?.issue(
            uuid::Uuid::new_v4().simple().to_string(),
            tenant_id,
            role,
            now_unix_ms,
            expires_at_unix_ms,
        )?;
        match self
            .propose(ControlCommand::IssueKey {
                request_id: format!("issue-key-{}", issued.metadata.key_id),
                metadata: issued.metadata.clone(),
            })
            .await?
        {
            ControlResponse::Applied { .. } => Ok(issued),
            ControlResponse::AlreadyApplied { .. } => {
                Err("credential issue request was already applied".to_string())
            }
            ControlResponse::Rejected { reason } => Err(reason),
        }
    }

    pub async fn revoke_credential(
        &self,
        key_id: &str,
        revoked_at_unix_ms: u64,
    ) -> Result<u64, String> {
        match self
            .propose(ControlCommand::RevokeKey {
                key_id: key_id.to_string(),
                request_id: format!("revoke-key-{key_id}-{revoked_at_unix_ms}"),
                revoked_at_unix_ms,
            })
            .await?
        {
            ControlResponse::Applied { revision } => Ok(revision),
            ControlResponse::AlreadyApplied { revision } => Ok(revision),
            ControlResponse::Rejected { reason } => Err(reason),
        }
    }

    #[cfg(unix)]
    pub async fn bootstrap_platform_credential_from_file(
        &self,
        path: &Path,
    ) -> Result<u64, String> {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| format!("could not open bootstrap secret file: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("could not inspect bootstrap secret file: {error}"))?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o777 != 0o600 {
            return Err("bootstrap secret file must be a regular mode-0600 file".to_string());
        }
        let mut secret = String::new();
        file.read_to_string(&mut secret)
            .map_err(|error| format!("could not read bootstrap secret file: {error}"))?;
        drop(file);
        while secret.ends_with(['\r', '\n']) {
            secret.pop();
        }
        let result = self.bootstrap_platform_credential(&secret).await;
        secret.clear();
        std::fs::remove_file(path)
            .map_err(|error| format!("could not remove bootstrap secret file: {error}"))?;
        result
    }

    pub fn local_state(&self) -> Result<ControlState, String> {
        self.inner.store.state()
    }

    pub fn current_leader(&self) -> Option<NodeId> {
        self.inner.raft.metrics().borrow().current_leader
    }

    pub async fn add_learner(&self, node: NodeMetadata) -> Result<(), String> {
        add_learner_inner(&self.inner, node).await
    }

    pub async fn change_membership(
        &self,
        voters: BTreeSet<NodeId>,
        expected_membership_epoch: u64,
    ) -> Result<(), String> {
        change_membership_inner(&self.inner, voters, expected_membership_epoch).await
    }

    pub async fn wait_for_revision(&self, revision: u64, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .local_state()
                .is_ok_and(|state| state.revision >= revision)
            {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub async fn stop_transport(&self) -> Result<(), String> {
        if let Some(server) = self.server.lock().await.take() {
            server.shutdown().await?;
        }
        Ok(())
    }

    /// Test/operations fault injection: stop both Raft processing and authenticated transport.
    /// The data store remains available so a colocated API can prove it fails closed.
    pub async fn isolate(&self) -> Result<(), String> {
        self.inner
            .raft
            .shutdown()
            .await
            .map_err(|error| error.to_string())?;
        self.stop_transport().await
    }

    pub async fn shutdown(self) -> Result<(), String> {
        self.inner
            .raft
            .shutdown()
            .await
            .map_err(|error| error.to_string())?;
        if let Some(server) = self.server.lock().await.take() {
            server.shutdown().await?;
        }
        Ok(())
    }
}

struct ControlRpcHandler {
    inner: Arc<ControlNodeInner>,
}

impl RpcHandler for ControlRpcHandler {
    fn handle(
        &self,
        payload: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send + '_>>
    {
        Box::pin(async move {
            let request: ControlRpcRequest = bincode::deserialize(&payload)
                .map_err(|error| format!("invalid control RPC request: {error}"))?;
            let response = match request {
                ControlRpcRequest::Raft(request) => {
                    ControlRpcResponse::Raft(handle_raft_rpc(&self.inner, request).await)
                }
                ControlRpcRequest::Propose(command) => {
                    ControlRpcResponse::Propose(propose_inner(&self.inner, command).await)
                }
                ControlRpcRequest::LinearizableRead => {
                    ControlRpcResponse::LinearizableRead(linearizable_read_inner(&self.inner).await)
                }
                ControlRpcRequest::AddLearner(node) => {
                    ControlRpcResponse::Operation(add_learner_inner(&self.inner, node).await)
                }
                ControlRpcRequest::ChangeMembership {
                    voters,
                    expected_membership_epoch,
                } => ControlRpcResponse::Operation(
                    change_membership_inner(&self.inner, voters, expected_membership_epoch).await,
                ),
            };
            bincode::serialize(&response).map_err(|error| error.to_string())
        })
    }
}

async fn handle_raft_rpc(inner: &ControlNodeInner, request: RaftRpcRequest) -> RaftRpcResponse {
    match request {
        RaftRpcRequest::AppendEntries(request) => {
            RaftRpcResponse::AppendEntries(inner.raft.append_entries(request).await)
        }
        RaftRpcRequest::Vote(request) => RaftRpcResponse::Vote(inner.raft.vote(request).await),
        RaftRpcRequest::InstallSnapshot(request) => {
            RaftRpcResponse::InstallSnapshot(inner.raft.install_snapshot(request).await)
        }
    }
}

async fn propose_inner(
    inner: &ControlNodeInner,
    command: ControlCommand,
) -> Result<ControlResponse, String> {
    match inner
        .raft
        .client_write::<tokio::sync::oneshot::error::RecvError>(command.clone())
        .await
    {
        Ok(response) => Ok(response.data),
        Err(error) => {
            let leader = error
                .forward_to_leader()
                .and_then(|forward| forward.leader_node.clone())
                .ok_or_else(|| format!("control proposal has no known leader: {error}"))?;
            match send_control(inner, &leader, ControlRpcRequest::Propose(command)).await? {
                ControlRpcResponse::Propose(result) => result,
                _ => Err("unexpected proposal forwarding response".to_string()),
            }
        }
    }
}

async fn linearizable_read_inner(inner: &ControlNodeInner) -> Result<ControlState, String> {
    match inner.raft.ensure_linearizable().await {
        Ok(_) => inner.store.state(),
        Err(error) => {
            let leader = error
                .forward_to_leader()
                .and_then(|forward| forward.leader_node.clone())
                .ok_or_else(|| format!("linearizable read has no known leader: {error}"))?;
            match send_control(inner, &leader, ControlRpcRequest::LinearizableRead).await? {
                ControlRpcResponse::LinearizableRead(result) => result,
                _ => Err("unexpected linearizable-read forwarding response".to_string()),
            }
        }
    }
}

async fn add_learner_inner(inner: &ControlNodeInner, node: NodeMetadata) -> Result<(), String> {
    inner.peers.register(node.clone());
    let registration = ControlCommand::RegisterNode {
        request_id: format!("register-node-{}", node.node_id),
        node: node.clone(),
    };
    propose_inner(inner, registration).await?;
    match inner
        .raft
        .add_learner(node.node_id, node.clone(), true)
        .await
    {
        Ok(_) => Ok(()),
        Err(error) => {
            let leader = error
                .forward_to_leader()
                .and_then(|forward| forward.leader_node.clone())
                .ok_or_else(|| format!("add learner has no known leader: {error}"))?;
            match send_control(inner, &leader, ControlRpcRequest::AddLearner(node)).await? {
                ControlRpcResponse::Operation(result) => result,
                _ => Err("unexpected add-learner forwarding response".to_string()),
            }
        }
    }
}

async fn change_membership_inner(
    inner: &ControlNodeInner,
    voters: BTreeSet<NodeId>,
    expected_membership_epoch: u64,
) -> Result<(), String> {
    if voters.is_empty() {
        return Err("membership must contain at least one voter".to_string());
    }
    let _guard = inner.membership_change.lock().await;
    let state = inner.store.state()?;
    if state.membership_epoch != expected_membership_epoch {
        return Err("membership epoch changed".to_string());
    }
    if !voters.iter().all(|node| state.nodes.contains_key(node)) {
        return Err("all voters must be registered learners before promotion".to_string());
    }
    match inner.raft.change_membership(voters.clone(), true).await {
        Ok(_) => {}
        Err(error) => {
            let leader = error
                .forward_to_leader()
                .and_then(|forward| forward.leader_node.clone())
                .ok_or_else(|| format!("membership change has no known leader: {error}"))?;
            return match send_control(
                inner,
                &leader,
                ControlRpcRequest::ChangeMembership {
                    voters,
                    expected_membership_epoch,
                },
            )
            .await?
            {
                ControlRpcResponse::Operation(result) => result,
                _ => Err("unexpected membership forwarding response".to_string()),
            };
        }
    }
    let response = propose_inner(
        inner,
        ControlCommand::SetVoters {
            request_id: format!(
                "set-voters-{}-{}",
                expected_membership_epoch,
                voters
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("-")
            ),
            expected_membership_epoch,
            voters,
        },
    )
    .await?;
    match response {
        ControlResponse::Applied { .. } | ControlResponse::AlreadyApplied { .. } => Ok(()),
        ControlResponse::Rejected { reason } => Err(reason),
    }
}

async fn send_control(
    inner: &ControlNodeInner,
    target: &NodeMetadata,
    request: ControlRpcRequest,
) -> Result<ControlRpcResponse, String> {
    let payload = bincode::serialize(&request).map_err(|error| error.to_string())?;
    let response = inner.client.request(target, payload).await?;
    bincode::deserialize(&response).map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
pub enum BootstrapMode {
    Single,
    Join { seed: NodeMetadata },
}

#[derive(Debug, Clone)]
pub struct DistributedProductionConfig {
    pub node: ControlNodeConfig,
    pub bootstrap: BootstrapMode,
    pub platform_bootstrap: Option<PlatformBootstrapSource>,
}

#[derive(Debug, Clone)]
pub enum PlatformBootstrapSource {
    File(PathBuf),
    Stdin,
}

pub fn validate_production_ready(
    config: &ControlNodeConfig,
    bootstrap: &BootstrapMode,
) -> Result<(), String> {
    config.validate()?;
    config.tls_identity.validate_crypto()?;
    if let BootstrapMode::Join { seed } = bootstrap {
        if seed.node_id == 0 || seed.raft_addr.is_empty() || seed.certificate_sha256 == [0; 32] {
            return Err(
                "join bootstrap requires seed node id, Raft address, and certificate fingerprint"
                    .to_string(),
            );
        }
    }
    Ok(())
}

pub fn production_config_from_env() -> Result<DistributedProductionConfig, String> {
    let required = |name: &str| {
        std::env::var(name).map_err(|_| format!("{name} is required for distributed production"))
    };
    let node_id = required("FLUCTLIGHT_DISTRIBUTED_NODE_ID")?
        .parse::<NodeId>()
        .map_err(|error| format!("invalid FLUCTLIGHT_DISTRIBUTED_NODE_ID: {error}"))?;
    let read_file = |name: &str| -> Result<Vec<u8>, String> {
        let path = required(name)?;
        std::fs::read(&path).map_err(|error| format!("could not read {name} at {path}: {error}"))
    };
    let tls_identity = TlsIdentity {
        node_id,
        certificate_chain_der: vec![read_file("FLUCTLIGHT_DISTRIBUTED_CERT_DER")?],
        private_key_der: read_file("FLUCTLIGHT_DISTRIBUTED_KEY_DER")?,
        ca_certificate_der: read_file("FLUCTLIGHT_DISTRIBUTED_CA_DER")?,
        server_name: required("FLUCTLIGHT_DISTRIBUTED_SERVER_NAME")?,
    };
    let cluster_pepper = decode_hex(&required("FLUCTLIGHT_CLUSTER_PEPPER_HEX")?)?;
    let node = ControlNodeConfig {
        node_id,
        bind_addr: required("FLUCTLIGHT_DISTRIBUTED_BIND")?,
        data_dir: PathBuf::from(required("FLUCTLIGHT_DISTRIBUTED_DATA_DIR")?),
        cluster_pepper,
        tls_identity,
        cluster_name: required("FLUCTLIGHT_DISTRIBUTED_CLUSTER_NAME")?,
    };
    let bootstrap = match required("FLUCTLIGHT_DISTRIBUTED_BOOTSTRAP")?.as_str() {
        "single" => BootstrapMode::Single,
        "join" => BootstrapMode::Join {
            seed: NodeMetadata {
                node_id: required("FLUCTLIGHT_DISTRIBUTED_SEED_NODE_ID")?
                    .parse()
                    .map_err(|error| {
                        format!("invalid FLUCTLIGHT_DISTRIBUTED_SEED_NODE_ID: {error}")
                    })?,
                raft_addr: required("FLUCTLIGHT_DISTRIBUTED_SEED_RAFT_ADDR")?,
                api_addr: String::new(),
                certificate_sha256: decode_fingerprint(&required(
                    "FLUCTLIGHT_DISTRIBUTED_SEED_CERT_SHA256",
                )?)?,
            },
        },
        other => {
            return Err(format!(
                "FLUCTLIGHT_DISTRIBUTED_BOOTSTRAP must be 'single' or 'join', got {other:?}"
            ))
        }
    };
    let platform_bootstrap = match &bootstrap {
        BootstrapMode::Single => {
            if let Ok(path) = std::env::var("FLUCTLIGHT_PLATFORM_BOOTSTRAP_FILE") {
                Some(PlatformBootstrapSource::File(PathBuf::from(path)))
            } else if std::env::var("FLUCTLIGHT_PLATFORM_BOOTSTRAP_STDIN")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            {
                Some(PlatformBootstrapSource::Stdin)
            } else {
                return Err(
                    "single-node bootstrap requires FLUCTLIGHT_PLATFORM_BOOTSTRAP_FILE or FLUCTLIGHT_PLATFORM_BOOTSTRAP_STDIN=true"
                        .to_string(),
                );
            }
        }
        BootstrapMode::Join { .. } => None,
    };
    validate_production_ready(&node, &bootstrap)?;
    Ok(DistributedProductionConfig {
        node,
        bootstrap,
        platform_bootstrap,
    })
}

pub fn ensure_production_ready() -> Result<(), String> {
    production_config_from_env().map(|_| ())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex value must contain an even number of characters".to_string());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            u8::from_str_radix(text, 16).map_err(|error| error.to_string())
        })
        .collect()
}

fn decode_fingerprint(value: &str) -> Result<[u8; 32], String> {
    decode_hex(value)?
        .try_into()
        .map_err(|_| "certificate fingerprint must be exactly 32 bytes".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
        KeyUsagePurpose,
    };

    #[test]
    fn production_distributed_startup_rejects_missing_pepper_and_certificates() {
        let config = ControlNodeConfig {
            node_id: 1,
            bind_addr: "127.0.0.1:9000".into(),
            data_dir: std::env::temp_dir(),
            cluster_pepper: Vec::new(),
            tls_identity: TlsIdentity {
                node_id: 1,
                certificate_chain_der: Vec::new(),
                private_key_der: Vec::new(),
                ca_certificate_der: Vec::new(),
                server_name: String::new(),
            },
            cluster_name: "test".into(),
        };
        assert!(validate_production_ready(&config, &BootstrapMode::Single)
            .unwrap_err()
            .contains("pepper"));
    }

    #[test]
    fn production_distributed_startup_is_ready_with_complete_crypto_and_bootstrap() {
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
        let config = ControlNodeConfig {
            node_id: 1,
            bind_addr: "127.0.0.1:9000".into(),
            data_dir: std::env::temp_dir(),
            cluster_pepper: vec![1; 32],
            tls_identity: TlsIdentity {
                node_id: 1,
                certificate_chain_der: vec![node.der().to_vec()],
                private_key_der: node_key.serialize_der(),
                ca_certificate_der: ca.der().to_vec(),
                server_name: "localhost".into(),
            },
            cluster_name: "test".into(),
        };

        validate_production_ready(&config, &BootstrapMode::Single).unwrap();
    }

    #[test]
    fn control_node_exposes_authenticated_join_operation() {
        let _method = ControlNode::join_cluster;
    }
}

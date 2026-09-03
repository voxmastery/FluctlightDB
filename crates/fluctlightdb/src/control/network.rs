use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use openraft::error::{InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{RaftNetwork, RaftNetworkFactory};

use super::types::{
    ControlCommand, ControlResponse, ControlState, ControlTypeConfig, NodeId, NodeMetadata,
};

const MAX_RPC_BYTES: usize = 16 * 1024 * 1024;
const TLS_TIMEOUT: Duration = Duration::from_secs(5);
const ALPN: &[u8] = b"fluctlight-raft/1";

#[derive(Clone)]
pub struct PeerIdentityBinder {
    nodes: BTreeMap<NodeId, NodeMetadata>,
}

impl PeerIdentityBinder {
    pub fn new(nodes: impl IntoIterator<Item = NodeMetadata>) -> Self {
        Self {
            nodes: nodes.into_iter().map(|node| (node.node_id, node)).collect(),
        }
    }

    pub fn verify(&self, claimed_node_id: NodeId, certificate_der: &[u8]) -> Result<(), String> {
        let registered = self
            .nodes
            .get(&claimed_node_id)
            .ok_or_else(|| "peer node id is not registered".to_string())?;
        if registered.certificate_sha256 != certificate_fingerprint(certificate_der) {
            return Err("peer certificate does not match registered node id".to_string());
        }
        Ok(())
    }
}

pub fn certificate_fingerprint(certificate_der: &[u8]) -> [u8; 32] {
    Sha256::digest(certificate_der).into()
}

#[derive(Clone, Default)]
pub struct PeerIdentityRegistry {
    nodes: Arc<RwLock<BTreeMap<NodeId, NodeMetadata>>>,
}

impl PeerIdentityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, node: NodeMetadata) {
        if let Ok(mut nodes) = self.nodes.write() {
            nodes.insert(node.node_id, node);
        }
    }

    pub fn replace(&self, nodes: impl IntoIterator<Item = NodeMetadata>) {
        if let Ok(mut registered) = self.nodes.write() {
            *registered = nodes.into_iter().map(|node| (node.node_id, node)).collect();
        }
    }

    pub fn verify(&self, node_id: NodeId, certificate_der: &[u8]) -> Result<(), String> {
        let nodes = self
            .nodes
            .read()
            .map_err(|_| "peer identity registry lock poisoned".to_string())?;
        let node = nodes
            .get(&node_id)
            .ok_or_else(|| "peer node id is not registered".to_string())?;
        if node.certificate_sha256 != certificate_fingerprint(certificate_der) {
            return Err("peer certificate does not match registered node id".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TlsIdentity {
    pub node_id: NodeId,
    pub certificate_chain_der: Vec<Vec<u8>>,
    pub private_key_der: Vec<u8>,
    pub ca_certificate_der: Vec<u8>,
    pub server_name: String,
}

impl TlsIdentity {
    pub fn validate(&self) -> Result<(), String> {
        if self.node_id == 0 {
            return Err("TLS node id must be non-zero".to_string());
        }
        if self.certificate_chain_der.is_empty() {
            return Err("TLS certificate chain is required".to_string());
        }
        if self.private_key_der.is_empty() {
            return Err("TLS private key is required".to_string());
        }
        if self.ca_certificate_der.is_empty() {
            return Err("TLS CA certificate is required".to_string());
        }
        if self.server_name.is_empty() {
            return Err("TLS server name is required".to_string());
        }
        Ok(())
    }

    pub fn validate_crypto(&self) -> Result<(), String> {
        self.client_config()?;
        self.server_config()?;
        Ok(())
    }

    fn certificate_chain(&self) -> Vec<CertificateDer<'static>> {
        self.certificate_chain_der
            .iter()
            .cloned()
            .map(CertificateDer::from)
            .collect()
    }

    fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.private_key_der.clone()))
    }

    fn roots(&self) -> Result<RootCertStore, String> {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(self.ca_certificate_der.clone()))
            .map_err(|error| format!("invalid TLS CA certificate: {error}"))?;
        Ok(roots)
    }

    fn client_config(&self) -> Result<Arc<ClientConfig>, String> {
        self.validate()?;
        let mut config = ClientConfig::builder()
            .with_root_certificates(self.roots()?)
            .with_client_auth_cert(self.certificate_chain(), self.private_key())
            .map_err(|error| format!("invalid TLS client identity: {error}"))?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        Ok(Arc::new(config))
    }

    fn server_config(&self) -> Result<Arc<ServerConfig>, String> {
        self.validate()?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(self.roots()?))
            .build()
            .map_err(|error| format!("invalid TLS client CA: {error}"))?;
        let mut config = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(self.certificate_chain(), self.private_key())
            .map_err(|error| format!("invalid TLS server identity: {error}"))?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        Ok(Arc::new(config))
    }
}

pub trait RpcHandler: Send + Sync + 'static {
    fn handle(
        &self,
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>>;
}

#[derive(Debug, Serialize, Deserialize)]
struct RpcEnvelope {
    sender_node_id: NodeId,
    payload: Vec<u8>,
}

#[derive(Clone)]
pub struct MtlsRpcClient {
    identity: TlsIdentity,
    connector: TlsConnector,
}

impl MtlsRpcClient {
    pub fn new(identity: TlsIdentity) -> Result<Self, String> {
        let connector = TlsConnector::from(identity.client_config()?);
        Ok(Self {
            identity,
            connector,
        })
    }

    pub async fn request(
        &self,
        target: &NodeMetadata,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        if payload.len() > MAX_RPC_BYTES {
            return Err("RPC payload exceeds maximum size".to_string());
        }
        let tcp = tokio::time::timeout(TLS_TIMEOUT, TcpStream::connect(&target.raft_addr))
            .await
            .map_err(|_| "RPC TCP connect timed out".to_string())?
            .map_err(|error| format!("RPC TCP connect failed: {error}"))?;
        let server_name = ServerName::try_from(self.identity.server_name.clone())
            .map_err(|error| format!("invalid TLS server name: {error}"))?;
        let mut tls = tokio::time::timeout(TLS_TIMEOUT, self.connector.connect(server_name, tcp))
            .await
            .map_err(|_| "RPC TLS handshake timed out".to_string())?
            .map_err(|error| format!("RPC TLS handshake failed: {error}"))?;
        let peer_certificate = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certificates| certificates.first())
            .ok_or_else(|| "RPC server did not present a certificate".to_string())?;
        if certificate_fingerprint(peer_certificate.as_ref()) != target.certificate_sha256 {
            return Err("RPC server certificate does not match target node id".to_string());
        }
        let envelope = bincode::serialize(&RpcEnvelope {
            sender_node_id: self.identity.node_id,
            payload,
        })
        .map_err(|error| error.to_string())?;
        write_frame(&mut tls, &envelope).await?;
        let response = read_frame(&mut tls).await?;
        bincode::deserialize::<Result<Vec<u8>, String>>(&response)
            .map_err(|error| format!("invalid RPC response: {error}"))?
    }
}

pub struct MtlsRpcServer {
    local_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<(), String>>,
}

impl MtlsRpcServer {
    pub async fn start(
        bind_addr: &str,
        identity: TlsIdentity,
        peers: PeerIdentityRegistry,
        handler: Arc<dyn RpcHandler>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|error| format!("RPC bind failed: {error}"))?;
        let local_addr = listener.local_addr().map_err(|error| error.to_string())?;
        let acceptor = TlsAcceptor::from(identity.server_config()?);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                let (tcp, _) = tokio::select! {
                    _ = &mut shutdown_rx => return Ok(()),
                    accepted = listener.accept() => {
                        accepted.map_err(|error| format!("RPC accept failed: {error}"))?
                    }
                };
                let acceptor = acceptor.clone();
                let peers = peers.clone();
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    let _ = handle_connection(tcp, acceptor, peers, handler).await;
                });
            }
        });
        Ok(Self {
            local_addr,
            shutdown: Some(shutdown_tx),
            task,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn shutdown(mut self) -> Result<(), String> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task
            .await
            .map_err(|error| format!("RPC server task failed: {error}"))?
    }
}

async fn handle_connection(
    tcp: TcpStream,
    acceptor: TlsAcceptor,
    peers: PeerIdentityRegistry,
    handler: Arc<dyn RpcHandler>,
) -> Result<(), String> {
    let mut tls = tokio::time::timeout(TLS_TIMEOUT, acceptor.accept(tcp))
        .await
        .map_err(|_| "RPC TLS handshake timed out".to_string())?
        .map_err(|error| format!("RPC TLS handshake failed: {error}"))?;
    let peer_certificate = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or_else(|| "RPC client did not present a certificate".to_string())?
        .as_ref()
        .to_vec();
    let frame = read_frame(&mut tls).await?;
    let envelope: RpcEnvelope =
        bincode::deserialize(&frame).map_err(|error| format!("invalid RPC request: {error}"))?;
    peers.verify(envelope.sender_node_id, &peer_certificate)?;
    let response = handler.handle(envelope.payload).await;
    let encoded = bincode::serialize(&response).map_err(|error| error.to_string())?;
    write_frame(&mut tls, &encoded).await
}

async fn write_frame<S>(stream: &mut S, payload: &[u8]) -> Result<(), String>
where
    S: AsyncWriteExt + Unpin,
{
    if payload.len() > MAX_RPC_BYTES {
        return Err("RPC frame exceeds maximum size".to_string());
    }
    stream
        .write_u32(payload.len() as u32)
        .await
        .map_err(|error| error.to_string())?;
    stream
        .write_all(payload)
        .await
        .map_err(|error| error.to_string())?;
    stream.flush().await.map_err(|error| error.to_string())
}

async fn read_frame<S>(stream: &mut S) -> Result<Vec<u8>, String>
where
    S: AsyncReadExt + Unpin,
{
    let length = stream.read_u32().await.map_err(|error| error.to_string())? as usize;
    if length > MAX_RPC_BYTES {
        return Err("RPC frame exceeds maximum size".to_string());
    }
    let mut payload = vec![0; length];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|error| error.to_string())?;
    Ok(payload)
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum RaftRpcRequest {
    AppendEntries(AppendEntriesRequest<ControlTypeConfig>),
    Vote(VoteRequest<NodeId>),
    InstallSnapshot(InstallSnapshotRequest<ControlTypeConfig>),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum RaftRpcResponse {
    AppendEntries(Result<AppendEntriesResponse<NodeId>, RaftError<NodeId>>),
    Vote(Result<VoteResponse<NodeId>, RaftError<NodeId>>),
    InstallSnapshot(
        Result<InstallSnapshotResponse<NodeId>, RaftError<NodeId, InstallSnapshotError>>,
    ),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ControlRpcRequest {
    Raft(RaftRpcRequest),
    Propose(ControlCommand),
    LinearizableRead,
    AddLearner(NodeMetadata),
    ChangeMembership {
        voters: std::collections::BTreeSet<NodeId>,
        expected_membership_epoch: u64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ControlRpcResponse {
    Raft(RaftRpcResponse),
    Propose(Result<ControlResponse, String>),
    LinearizableRead(Result<ControlState, String>),
    Operation(Result<(), String>),
}

#[derive(Clone)]
pub struct MtlsRaftNetworkFactory {
    client: MtlsRpcClient,
}

impl MtlsRaftNetworkFactory {
    pub fn new(identity: TlsIdentity) -> Result<Self, String> {
        Ok(Self {
            client: MtlsRpcClient::new(identity)?,
        })
    }
}

pub struct MtlsRaftNetwork {
    client: MtlsRpcClient,
    target: NodeId,
    node: NodeMetadata,
}

impl RaftNetworkFactory<ControlTypeConfig> for MtlsRaftNetworkFactory {
    type Network = MtlsRaftNetwork;

    async fn new_client(&mut self, target: NodeId, node: &NodeMetadata) -> Self::Network {
        MtlsRaftNetwork {
            client: self.client.clone(),
            target,
            node: node.clone(),
        }
    }
}

impl RaftNetwork<ControlTypeConfig> for MtlsRaftNetwork {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<ControlTypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, NodeMetadata, RaftError<NodeId>>>
    {
        let response = self.send(RaftRpcRequest::AppendEntries(rpc)).await?;
        match response {
            RaftRpcResponse::AppendEntries(Ok(response)) => Ok(response),
            RaftRpcResponse::AppendEntries(Err(error)) => {
                Err(RemoteError::new_with_node(self.target, self.node.clone(), error).into())
            }
            _ => Err(network_error("unexpected append-entries RPC response")),
        }
    }

    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<ControlTypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, NodeMetadata, RaftError<NodeId, InstallSnapshotError>>,
    > {
        let response = self.send(RaftRpcRequest::InstallSnapshot(rpc)).await?;
        match response {
            RaftRpcResponse::InstallSnapshot(Ok(response)) => Ok(response),
            RaftRpcResponse::InstallSnapshot(Err(error)) => {
                Err(RemoteError::new_with_node(self.target, self.node.clone(), error).into())
            }
            _ => Err(network_error("unexpected install-snapshot RPC response")),
        }
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, NodeMetadata, RaftError<NodeId>>> {
        let response = self.send(RaftRpcRequest::Vote(rpc)).await?;
        match response {
            RaftRpcResponse::Vote(Ok(response)) => Ok(response),
            RaftRpcResponse::Vote(Err(error)) => {
                Err(RemoteError::new_with_node(self.target, self.node.clone(), error).into())
            }
            _ => Err(network_error("unexpected vote RPC response")),
        }
    }
}

impl MtlsRaftNetwork {
    async fn send<E>(
        &self,
        request: RaftRpcRequest,
    ) -> Result<RaftRpcResponse, RPCError<NodeId, NodeMetadata, E>>
    where
        E: std::error::Error,
    {
        let payload = bincode::serialize(&ControlRpcRequest::Raft(request))
            .map_err(|error| network_error(&format!("could not encode Raft RPC: {error}")))?;
        let response = self
            .client
            .request(&self.node, payload)
            .await
            .map_err(|error| network_error(&error))?;
        match bincode::deserialize(&response)
            .map_err(|error| network_error(&format!("could not decode Raft RPC: {error}")))?
        {
            ControlRpcResponse::Raft(response) => Ok(response),
            _ => Err(network_error("unexpected control RPC response")),
        }
    }
}

fn network_error<E>(message: &str) -> RPCError<NodeId, NodeMetadata, E>
where
    E: std::error::Error,
{
    let error = std::io::Error::other(message.to_string());
    Unreachable::new(&error).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::types::NodeMetadata;
    use rcgen::{
        BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
        KeyUsagePurpose,
    };
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    #[test]
    fn peer_certificate_must_match_registered_node_id() {
        let certificate = b"node-one-certificate";
        let node = NodeMetadata {
            node_id: 1,
            raft_addr: "127.0.0.1:9101".into(),
            api_addr: "127.0.0.1:9201".into(),
            certificate_sha256: certificate_fingerprint(certificate),
        };
        let binder = PeerIdentityBinder::new([node]);

        assert!(binder.verify(1, certificate).is_ok());
        assert!(binder.verify(2, certificate).is_err());
        assert!(binder.verify(1, b"attacker-certificate").is_err());
    }

    struct Echo;

    impl RpcHandler for Echo {
        fn handle(
            &self,
            payload: Vec<u8>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>> {
            Box::pin(async move { Ok(payload) })
        }
    }

    fn test_ca() -> (Certificate, KeyPair) {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let key = KeyPair::generate().unwrap();
        (params.self_signed(&key).unwrap(), key)
    }

    fn test_identity(node_id: NodeId, ca: &Certificate, ca_key: &KeyPair) -> TlsIdentity {
        let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, ca, ca_key).unwrap();
        TlsIdentity {
            node_id,
            certificate_chain_der: vec![cert.der().to_vec()],
            private_key_der: key.serialize_der(),
            ca_certificate_der: ca.der().to_vec(),
            server_name: "localhost".into(),
        }
    }

    #[tokio::test]
    async fn mtls_round_trip_requires_registered_certificate_and_node_identity() {
        let (ca, ca_key) = test_ca();
        let server_identity = test_identity(1, &ca, &ca_key);
        let client_identity = test_identity(2, &ca, &ca_key);
        let registry = PeerIdentityRegistry::new();
        registry.register(NodeMetadata {
            node_id: 2,
            certificate_sha256: certificate_fingerprint(&client_identity.certificate_chain_der[0]),
            ..NodeMetadata::default()
        });
        let server = MtlsRpcServer::start(
            "127.0.0.1:0",
            server_identity.clone(),
            registry,
            Arc::new(Echo),
        )
        .await
        .unwrap();
        let target = NodeMetadata {
            node_id: 1,
            raft_addr: server.local_addr().to_string(),
            certificate_sha256: certificate_fingerprint(&server_identity.certificate_chain_der[0]),
            ..NodeMetadata::default()
        };
        let client = MtlsRpcClient::new(client_identity.clone()).unwrap();

        assert_eq!(
            client.request(&target, b"raft".to_vec()).await.unwrap(),
            b"raft"
        );

        let attacker = MtlsRpcClient::new(test_identity(3, &ca, &ca_key)).unwrap();
        assert!(attacker.request(&target, b"raft".to_vec()).await.is_err());

        let mut wrong_target = target;
        wrong_target.certificate_sha256 = [0; 32];
        assert!(client
            .request(&wrong_target, b"raft".to_vec())
            .await
            .is_err());
        server.shutdown().await.unwrap();
    }

    #[test]
    fn mtls_factory_implements_openraft_network_traits() {
        fn assert_factory<
            T: openraft::RaftNetworkFactory<crate::control::types::ControlTypeConfig>,
        >() {
        }
        fn assert_network<T: openraft::RaftNetwork<crate::control::types::ControlTypeConfig>>() {}

        assert_factory::<MtlsRaftNetworkFactory>();
        assert_network::<MtlsRaftNetwork>();
    }
}

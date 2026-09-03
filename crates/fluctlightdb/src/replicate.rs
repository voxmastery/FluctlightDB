//! Incremental replication state + delta sync.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};
use crate::manifest::BrainManifest;
use crate::storage;
use crate::store;
use crate::wal;

const CHECKPOINT_TRANSFER_VERSION: u32 = 1;
pub const MAX_CHECKPOINT_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointFile {
    pub name: String,
    pub length: u64,
    pub sha256: [u8; 32],
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointTransfer {
    pub protocol_version: u32,
    pub tenant_uuid: uuid::Uuid,
    pub writer_epoch: u64,
    pub fence_generation: u64,
    pub checkpoint_watermark: u64,
    pub source_generation: String,
    pub files: Vec<CheckpointFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointFileDescriptor {
    pub name: String,
    pub length: u64,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDescriptor {
    pub protocol_version: u32,
    pub transfer_id: uuid::Uuid,
    pub tenant_uuid: uuid::Uuid,
    pub writer_epoch: u64,
    pub fence_generation: u64,
    pub checkpoint_watermark: u64,
    pub source_generation: String,
    pub generation_sha256: [u8; 32],
    pub files: Vec<CheckpointFileDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointChunk {
    pub transfer_id: uuid::Uuid,
    pub file_name: String,
    pub offset: u64,
    pub sha256: [u8; 32],
    pub bytes: Vec<u8>,
}

impl CheckpointChunk {
    pub fn new(transfer_id: uuid::Uuid, file_name: String, offset: u64, bytes: Vec<u8>) -> Self {
        Self {
            transfer_id,
            file_name,
            offset,
            sha256: Sha256::digest(&bytes).into(),
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointProgress {
    pub transfer_id: uuid::Uuid,
    pub offsets: std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAck {
    pub tenant_uuid: uuid::Uuid,
    pub fence_generation: u64,
    pub durable_watermark: u64,
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub mutation_sha256: Option<[u8; 32]>,
}

impl CheckpointTransfer {
    pub fn from_active(primary: &Path, identity: wal::WalIdentity) -> Result<Self> {
        let current = fs::read_to_string(primary.join("CURRENT")).map_err(|error| {
            Error::Store(format!("checkpoint has no active generation: {error}"))
        })?;
        let source_generation = current.trim().to_string();
        validate_generation_name(&source_generation)?;
        let generation = primary.join("generations").join(&source_generation);
        let manifest_bytes = fs::read(generation.join("manifest.json"))?;
        let manifest: BrainManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| Error::Serde(error.to_string()))?;
        validate_manifest_identity(&manifest, &identity)?;

        let mut names = manifest
            .segments
            .iter()
            .map(|name| format!("{name}.seg"))
            .collect::<Vec<_>>();
        names.push("manifest.json".into());
        names.sort();
        names.dedup();
        let mut files = Vec::with_capacity(names.len());
        for name in names {
            validate_file_name(&name)?;
            let bytes = fs::read(generation.join(&name))?;
            files.push(CheckpointFile {
                name,
                length: bytes.len() as u64,
                sha256: Sha256::digest(&bytes).into(),
                bytes,
            });
        }
        Ok(Self {
            protocol_version: CHECKPOINT_TRANSFER_VERSION,
            tenant_uuid: identity.tenant_uuid,
            writer_epoch: identity.writer_epoch,
            fence_generation: identity.fence_generation,
            checkpoint_watermark: manifest.wal_checkpoint_seq,
            source_generation,
            files,
        })
    }

    fn identity(&self, durability: crate::placement::DurabilityPolicy) -> wal::WalIdentity {
        wal::WalIdentity {
            tenant_uuid: self.tenant_uuid,
            writer_epoch: self.writer_epoch,
            fence_generation: self.fence_generation,
            durability,
        }
    }

    pub fn descriptor(&self) -> CheckpointDescriptor {
        let files: Vec<_> = self
            .files
            .iter()
            .map(|file| CheckpointFileDescriptor {
                name: file.name.clone(),
                length: file.length,
                sha256: file.sha256,
            })
            .collect();
        let generation_sha256 = descriptor_digest(&files);
        CheckpointDescriptor {
            protocol_version: self.protocol_version,
            transfer_id: uuid::Uuid::from_bytes(generation_sha256[..16].try_into().unwrap()),
            tenant_uuid: self.tenant_uuid,
            writer_epoch: self.writer_epoch,
            fence_generation: self.fence_generation,
            checkpoint_watermark: self.checkpoint_watermark,
            source_generation: self.source_generation.clone(),
            generation_sha256,
            files,
        }
    }
}

pub struct ReplicaStore {
    root: std::path::PathBuf,
    identity: wal::WalIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AppliedOperation {
    operation_id: String,
    seq: u64,
    sha256: [u8; 32],
}

impl ReplicaStore {
    pub fn new(root: &Path, identity: wal::WalIdentity) -> Self {
        Self {
            root: root.to_path_buf(),
            identity,
        }
    }

    pub fn install_checkpoint(&self, transfer: CheckpointTransfer) -> Result<DurableAck> {
        if transfer.protocol_version != CHECKPOINT_TRANSFER_VERSION {
            return Err(Error::Store(
                "unsupported checkpoint transfer version".into(),
            ));
        }
        if transfer.identity(self.identity.durability) != self.identity {
            return Err(Error::Store(
                "stale or mixed checkpoint fence generation".into(),
            ));
        }
        validate_generation_name(&transfer.source_generation)?;
        if transfer.files.is_empty() {
            return Err(Error::Store("checkpoint transfer has no files".into()));
        }

        let mut manifest = None;
        for file in &transfer.files {
            validate_file_name(&file.name)?;
            if file.length != file.bytes.len() as u64 {
                return Err(Error::Store(format!(
                    "checkpoint file length mismatch for {}",
                    file.name
                )));
            }
            if <[u8; 32]>::from(Sha256::digest(&file.bytes)) != file.sha256 {
                return Err(Error::Store(format!(
                    "checkpoint file SHA-256 mismatch for {}",
                    file.name
                )));
            }
            if file.name == "manifest.json" {
                let parsed: BrainManifest = serde_json::from_slice(&file.bytes)
                    .map_err(|error| Error::Serde(error.to_string()))?;
                validate_manifest_identity(&parsed, &self.identity)?;
                if parsed.wal_checkpoint_seq != transfer.checkpoint_watermark {
                    return Err(Error::Store(
                        "checkpoint manifest watermark does not match transfer".into(),
                    ));
                }
                manifest = Some(parsed);
            }
        }
        let manifest =
            manifest.ok_or_else(|| Error::Store("checkpoint transfer lacks manifest".into()))?;
        for segment in &manifest.segments {
            let expected = format!("{segment}.seg");
            if !transfer.files.iter().any(|file| file.name == expected) {
                return Err(Error::Store(format!(
                    "checkpoint transfer lacks manifest file {expected}"
                )));
            }
        }

        crate::segment::create_private_dir_all(&self.root)?;
        let staging_root = self.root.join("staging");
        crate::segment::create_private_dir_all(&staging_root)?;
        let transfer_tag = format!(
            "{}-{}-{}",
            transfer.fence_generation,
            transfer.checkpoint_watermark,
            hex_prefix(&manifest_digest(&transfer))
        );
        let staging = staging_root.join(&transfer_tag);
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        crate::segment::create_private_dir_all(&staging)?;
        for file in &transfer.files {
            let path = staging.join(&file.name);
            let mut output = crate::segment::create_private_file(&path)?;
            output.write_all(&file.bytes)?;
            output.sync_all()?;
        }
        crate::segment::sync_parent_dir(&staging.join("manifest.json"))?;
        crate::manifest::load_v4_dir(&staging)?;

        let generations = self.root.join("generations");
        crate::segment::create_private_dir_all(&generations)?;
        let generation_name = format!("gen-{:020}", transfer.checkpoint_watermark);
        let generation = generations.join(&generation_name);
        if generation.exists() {
            fs::remove_dir_all(&generation)?;
        }
        fs::rename(&staging, &generation)?;
        crate::segment::sync_parent_dir(&generation)?;

        let current_tmp = self
            .root
            .join(format!(".CURRENT.tmp-{}", std::process::id()));
        let mut current = crate::segment::create_private_file(&current_tmp)?;
        writeln!(current, "{generation_name}")?;
        current.sync_all()?;
        drop(current);
        fs::rename(&current_tmp, self.root.join("CURRENT"))?;
        crate::segment::sync_parent_dir(&self.root.join("CURRENT"))?;
        let _ = fs::remove_dir(&staging_root);

        Ok(DurableAck {
            tenant_uuid: self.identity.tenant_uuid,
            fence_generation: self.identity.fence_generation,
            durable_watermark: transfer.checkpoint_watermark,
            operation_id: None,
            mutation_sha256: None,
        })
    }

    pub fn begin_checkpoint(&self, descriptor: CheckpointDescriptor) -> Result<CheckpointProgress> {
        if descriptor.protocol_version != CHECKPOINT_TRANSFER_VERSION
            || descriptor.tenant_uuid != self.identity.tenant_uuid
            || descriptor.writer_epoch != self.identity.writer_epoch
            || descriptor.fence_generation != self.identity.fence_generation
        {
            return Err(Error::Store(
                "stale or mixed checkpoint descriptor identity".into(),
            ));
        }
        validate_generation_name(&descriptor.source_generation)?;
        if descriptor.files.is_empty()
            || descriptor.generation_sha256 != descriptor_digest(&descriptor.files)
        {
            return Err(Error::Store(
                "checkpoint whole-generation SHA-256 mismatch".into(),
            ));
        }
        let mut unique = std::collections::BTreeSet::new();
        for file in &descriptor.files {
            validate_file_name(&file.name)?;
            if !unique.insert(file.name.clone()) {
                return Err(Error::Store("duplicate checkpoint file descriptor".into()));
            }
        }
        let staging = self.chunk_staging(descriptor.transfer_id);
        crate::segment::create_private_dir_all(&staging)?;
        let session = staging.join("session.bin");
        if session.exists() {
            let existing: CheckpointDescriptor = bincode::deserialize(&fs::read(&session)?)
                .map_err(|error| Error::Serde(error.to_string()))?;
            if existing != descriptor {
                fs::remove_dir_all(&staging)?;
                return Err(Error::Store("checkpoint resume descriptor changed".into()));
            }
        } else {
            let encoded =
                bincode::serialize(&descriptor).map_err(|error| Error::Serde(error.to_string()))?;
            let mut output = crate::segment::create_private_file(&session)?;
            output.write_all(&encoded)?;
            output.sync_all()?;
            crate::segment::sync_parent_dir(&session)?;
        }
        self.chunk_progress(&descriptor)
    }

    pub fn write_checkpoint_chunk(&self, chunk: CheckpointChunk) -> Result<CheckpointProgress> {
        if chunk.bytes.is_empty() || chunk.bytes.len() > MAX_CHECKPOINT_CHUNK_BYTES {
            return Err(Error::Store("checkpoint chunk exceeds bounded size".into()));
        }
        if <[u8; 32]>::from(Sha256::digest(&chunk.bytes)) != chunk.sha256 {
            return Err(Error::Store("checkpoint chunk SHA-256 mismatch".into()));
        }
        validate_file_name(&chunk.file_name)?;
        let descriptor = self.load_chunk_descriptor(chunk.transfer_id)?;
        let file = descriptor
            .files
            .iter()
            .find(|file| file.name == chunk.file_name)
            .ok_or_else(|| Error::Store("checkpoint chunk file is not in descriptor".into()))?;
        let path = self
            .chunk_staging(chunk.transfer_id)
            .join(format!("{}.part", chunk.file_name));
        let current = fs::metadata(&path)
            .map(|meta| meta.len())
            .unwrap_or_default();
        if chunk.offset != current {
            return Err(Error::Store(format!(
                "checkpoint chunk offset mismatch: expected {current}, found {}",
                chunk.offset
            )));
        }
        if current.saturating_add(chunk.bytes.len() as u64) > file.length {
            return Err(Error::Store(
                "checkpoint chunk exceeds declared file length".into(),
            ));
        }
        let mut output = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        output.write_all(&chunk.bytes)?;
        output.sync_all()?;
        self.chunk_progress(&descriptor)
    }

    pub fn finish_checkpoint(&self, transfer_id: uuid::Uuid) -> Result<DurableAck> {
        let descriptor = self.load_chunk_descriptor(transfer_id)?;
        let staging = self.chunk_staging(transfer_id);
        let result = (|| {
            let mut files = Vec::with_capacity(descriptor.files.len());
            for expected in &descriptor.files {
                let path = staging.join(format!("{}.part", expected.name));
                let bytes = fs::read(&path)?;
                if bytes.len() as u64 != expected.length {
                    return Err(Error::Store(format!(
                        "checkpoint file length mismatch for {}",
                        expected.name
                    )));
                }
                if <[u8; 32]>::from(Sha256::digest(&bytes)) != expected.sha256 {
                    return Err(Error::Store(format!(
                        "checkpoint whole-file SHA-256 mismatch for {}",
                        expected.name
                    )));
                }
                files.push(CheckpointFile {
                    name: expected.name.clone(),
                    length: expected.length,
                    sha256: expected.sha256,
                    bytes,
                });
            }
            if descriptor.generation_sha256 != descriptor_digest(&descriptor.files) {
                return Err(Error::Store(
                    "checkpoint whole-generation SHA-256 mismatch".into(),
                ));
            }
            self.install_checkpoint(CheckpointTransfer {
                protocol_version: descriptor.protocol_version,
                tenant_uuid: descriptor.tenant_uuid,
                writer_epoch: descriptor.writer_epoch,
                fence_generation: descriptor.fence_generation,
                checkpoint_watermark: descriptor.checkpoint_watermark,
                source_generation: descriptor.source_generation,
                files,
            })
        })();
        let _ = fs::remove_dir_all(&staging);
        let staging_root = self.root.join("staging");
        let _ = fs::remove_dir(&staging_root);
        result
    }

    pub fn apply_wal_frames(&self, frames: Vec<wal::WalReplicationFrame>) -> Result<DurableAck> {
        if frames.is_empty() {
            return Err(Error::Store("WAL replication frame batch is empty".into()));
        }
        let durable_path = self.root.join("DURABLE");
        let durable = if durable_path.exists() {
            fs::read_to_string(&durable_path)?
                .trim()
                .parse::<u64>()
                .map_err(|error| Error::Store(format!("invalid durable watermark: {error}")))?
        } else {
            let active = active_manifest(&self.root)?;
            validate_manifest_identity(&active, &self.identity)?;
            active.wal_checkpoint_seq
        };
        if frames.iter().all(|frame| frame.seq <= durable) {
            let applied = self.load_applied_operations()?;
            for frame in &frames {
                if !applied.iter().any(|operation| {
                    operation.operation_id == frame.operation_id
                        && operation.seq == frame.seq
                        && operation.sha256 == frame.sha256
                }) {
                    return Err(Error::Store(
                        "duplicate WAL sequence has a different operation identity".into(),
                    ));
                }
            }
            let last = frames.last().unwrap();
            return Ok(DurableAck {
                tenant_uuid: self.identity.tenant_uuid,
                fence_generation: self.identity.fence_generation,
                durable_watermark: last.seq,
                operation_id: Some(last.operation_id.clone()),
                mutation_sha256: Some(last.sha256),
            });
        }
        if frames.iter().any(|frame| frame.seq <= durable) {
            return Err(Error::Store(
                "WAL retry batch mixes applied and new operations".into(),
            ));
        }
        let next = durable
            .checked_add(1)
            .ok_or_else(|| Error::Store("durable watermark exhausted".into()))?;
        let stored = wal::append_replication_frames(&self.root, next, &frames, &self.identity)?;

        let ledger_path = self.root.join("APPLIED_OPERATIONS");
        let mut ledger = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ledger_path)?;
        for frame in &frames {
            let operation = AppliedOperation {
                operation_id: frame.operation_id.clone(),
                seq: frame.seq,
                sha256: frame.sha256,
            };
            let encoded =
                serde_json::to_vec(&operation).map_err(|error| Error::Serde(error.to_string()))?;
            ledger.write_all(&encoded)?;
            ledger.write_all(b"\n")?;
        }
        ledger.sync_all()?;

        let temporary = self
            .root
            .join(format!(".DURABLE.tmp-{}", std::process::id()));
        let mut output = crate::segment::create_private_file(&temporary)?;
        writeln!(output, "{stored}")?;
        output.sync_all()?;
        drop(output);
        fs::rename(&temporary, &durable_path)?;
        crate::segment::sync_parent_dir(&durable_path)?;
        Ok(DurableAck {
            tenant_uuid: self.identity.tenant_uuid,
            fence_generation: self.identity.fence_generation,
            durable_watermark: stored,
            operation_id: frames.last().map(|frame| frame.operation_id.clone()),
            mutation_sha256: frames.last().map(|frame| frame.sha256),
        })
    }

    pub fn durable_watermark(&self) -> Result<u64> {
        let durable = self.root.join("DURABLE");
        if durable.exists() {
            return fs::read_to_string(durable)?
                .trim()
                .parse()
                .map_err(|error| Error::Store(format!("invalid durable watermark: {error}")));
        }
        let manifest = active_manifest(&self.root)?;
        validate_manifest_identity(&manifest, &self.identity)?;
        Ok(manifest.wal_checkpoint_seq)
    }

    fn chunk_staging(&self, transfer_id: uuid::Uuid) -> std::path::PathBuf {
        self.root.join("staging").join(transfer_id.to_string())
    }

    fn load_applied_operations(&self) -> Result<Vec<AppliedOperation>> {
        let path = self.root.join("APPLIED_OPERATIONS");
        if !path.exists() {
            return Ok(Vec::new());
        }
        fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|error| Error::Serde(error.to_string())))
            .collect()
    }

    fn load_chunk_descriptor(&self, transfer_id: uuid::Uuid) -> Result<CheckpointDescriptor> {
        let path = self.chunk_staging(transfer_id).join("session.bin");
        let descriptor: CheckpointDescriptor = bincode::deserialize(&fs::read(path)?)
            .map_err(|error| Error::Serde(error.to_string()))?;
        if descriptor.transfer_id != transfer_id {
            return Err(Error::Store("checkpoint transfer id mismatch".into()));
        }
        Ok(descriptor)
    }

    fn chunk_progress(&self, descriptor: &CheckpointDescriptor) -> Result<CheckpointProgress> {
        let staging = self.chunk_staging(descriptor.transfer_id);
        let mut offsets = std::collections::BTreeMap::new();
        for file in &descriptor.files {
            let length = fs::metadata(staging.join(format!("{}.part", file.name)))
                .map(|meta| meta.len())
                .unwrap_or_default();
            if length > file.length {
                return Err(Error::Store(
                    "staged checkpoint file exceeds descriptor".into(),
                ));
            }
            offsets.insert(file.name.clone(), length);
        }
        Ok(CheckpointProgress {
            transfer_id: descriptor.transfer_id,
            offsets,
        })
    }
}

fn active_manifest(root: &Path) -> Result<BrainManifest> {
    let current = fs::read_to_string(root.join("CURRENT"))?;
    let generation = current.trim();
    validate_generation_name(generation)?;
    let raw = fs::read(
        root.join("generations")
            .join(generation)
            .join("manifest.json"),
    )?;
    serde_json::from_slice(&raw).map_err(|error| Error::Serde(error.to_string()))
}

fn validate_manifest_identity(manifest: &BrainManifest, identity: &wal::WalIdentity) -> Result<()> {
    if manifest.tenant_uuid != Some(identity.tenant_uuid)
        || manifest.writer_epoch != identity.writer_epoch
        || manifest.fence_generation != identity.fence_generation
        || manifest.durability != Some(identity.durability)
    {
        return Err(Error::Store(
            "stale or mixed checkpoint manifest identity".into(),
        ));
    }
    Ok(())
}

fn validate_generation_name(name: &str) -> Result<()> {
    let valid = name.strip_prefix("gen-").is_some_and(|digits| {
        digits.len() == 20 && digits.bytes().all(|byte| byte.is_ascii_digit())
    });
    if !valid {
        return Err(Error::Store("invalid checkpoint generation name".into()));
    }
    Ok(())
}

fn validate_file_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(Error::Store("invalid checkpoint file name".into()));
    }
    Ok(())
}

fn manifest_digest(transfer: &CheckpointTransfer) -> [u8; 32] {
    let mut digest = Sha256::new();
    for file in &transfer.files {
        digest.update(file.name.as_bytes());
        digest.update(file.length.to_le_bytes());
        digest.update(file.sha256);
    }
    digest.finalize().into()
}

fn descriptor_digest(files: &[CheckpointFileDescriptor]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for file in files {
        digest.update(file.name.as_bytes());
        digest.update(file.length.to_le_bytes());
        digest.update(file.sha256);
    }
    digest.finalize().into()
}

fn hex_prefix(bytes: &[u8; 32]) -> String {
    bytes[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(feature = "distributed")]
#[derive(Debug, Serialize, Deserialize)]
enum ReplicationRpcRequest {
    BeginCheckpoint(CheckpointDescriptor),
    CheckpointChunk(CheckpointChunk),
    FinishCheckpoint(uuid::Uuid),
    ApplyWal(Vec<wal::WalReplicationFrame>),
}

#[cfg(feature = "distributed")]
#[derive(Debug, Serialize, Deserialize)]
enum ReplicationRpcResponse {
    Durable(std::result::Result<DurableAck, String>),
    Progress(std::result::Result<CheckpointProgress, String>),
}

#[cfg(feature = "distributed")]
pub struct ReplicationService {
    root: std::path::PathBuf,
    identity: wal::WalIdentity,
}

#[cfg(feature = "distributed")]
impl ReplicationService {
    pub fn new(root: &Path, identity: wal::WalIdentity) -> Self {
        Self {
            root: root.to_path_buf(),
            identity,
        }
    }
}

#[cfg(feature = "distributed")]
impl crate::control::network::RpcHandler for ReplicationService {
    fn handle(
        &self,
        payload: Vec<u8>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<Vec<u8>, String>> + Send + '_>,
    > {
        Box::pin(async move {
            let request: ReplicationRpcRequest = bincode::deserialize(&payload)
                .map_err(|error| format!("invalid tenant replication request: {error}"))?;
            let store = ReplicaStore::new(&self.root, self.identity);
            let response = match request {
                ReplicationRpcRequest::BeginCheckpoint(descriptor) => {
                    ReplicationRpcResponse::Progress(
                        store
                            .begin_checkpoint(descriptor)
                            .map_err(|error| error.to_string()),
                    )
                }
                ReplicationRpcRequest::CheckpointChunk(chunk) => ReplicationRpcResponse::Progress(
                    store
                        .write_checkpoint_chunk(chunk)
                        .map_err(|error| error.to_string()),
                ),
                ReplicationRpcRequest::FinishCheckpoint(transfer_id) => {
                    ReplicationRpcResponse::Durable(
                        store
                            .finish_checkpoint(transfer_id)
                            .map_err(|error| error.to_string()),
                    )
                }
                ReplicationRpcRequest::ApplyWal(frames) => ReplicationRpcResponse::Durable(
                    store
                        .apply_wal_frames(frames)
                        .map_err(|error| error.to_string()),
                ),
            };
            bincode::serialize(&response).map_err(|error| error.to_string())
        })
    }
}

#[cfg(feature = "distributed")]
#[derive(Clone)]
pub struct TenantReplicationClient {
    rpc: crate::control::network::MtlsRpcClient,
}

#[cfg(feature = "distributed")]
impl TenantReplicationClient {
    pub fn new(
        identity: crate::control::network::TlsIdentity,
    ) -> std::result::Result<Self, String> {
        Ok(Self {
            rpc: crate::control::network::MtlsRpcClient::new(identity)?,
        })
    }

    pub async fn install_checkpoint(
        &self,
        target: &crate::control::types::NodeMetadata,
        checkpoint: CheckpointTransfer,
    ) -> std::result::Result<DurableAck, String> {
        let descriptor = checkpoint.descriptor();
        let mut progress = self
            .request_progress(
                target,
                ReplicationRpcRequest::BeginCheckpoint(descriptor.clone()),
            )
            .await?;
        for file in checkpoint.files {
            let mut offset = progress
                .offsets
                .get(&file.name)
                .copied()
                .unwrap_or_default() as usize;
            while offset < file.bytes.len() {
                let next = (offset + MAX_CHECKPOINT_CHUNK_BYTES).min(file.bytes.len());
                let chunk = CheckpointChunk::new(
                    descriptor.transfer_id,
                    file.name.clone(),
                    offset as u64,
                    file.bytes[offset..next].to_vec(),
                );
                let mut last_error = None;
                for _ in 0..3 {
                    match self
                        .request_progress(
                            target,
                            ReplicationRpcRequest::CheckpointChunk(chunk.clone()),
                        )
                        .await
                    {
                        Ok(next_progress) => {
                            progress = next_progress;
                            last_error = None;
                            break;
                        }
                        Err(error) => {
                            last_error = Some(error);
                            if let Ok(resumed) = self
                                .request_progress(
                                    target,
                                    ReplicationRpcRequest::BeginCheckpoint(descriptor.clone()),
                                )
                                .await
                            {
                                progress = resumed;
                                if progress.offsets.get(&file.name).copied() == Some(next as u64) {
                                    last_error = None;
                                    break;
                                }
                            }
                        }
                    }
                }
                if let Some(error) = last_error {
                    return Err(format!("checkpoint chunk retry exhausted: {error}"));
                }
                offset = progress
                    .offsets
                    .get(&file.name)
                    .copied()
                    .unwrap_or_default() as usize;
            }
        }
        self.request_durable(
            target,
            ReplicationRpcRequest::FinishCheckpoint(descriptor.transfer_id),
        )
        .await
    }

    pub async fn apply_wal(
        &self,
        target: &crate::control::types::NodeMetadata,
        frames: Vec<wal::WalReplicationFrame>,
    ) -> std::result::Result<DurableAck, String> {
        self.request_durable(target, ReplicationRpcRequest::ApplyWal(frames))
            .await
    }

    async fn request_durable(
        &self,
        target: &crate::control::types::NodeMetadata,
        request: ReplicationRpcRequest,
    ) -> std::result::Result<DurableAck, String> {
        let payload = bincode::serialize(&request).map_err(|error| error.to_string())?;
        let response = self.rpc.request(target, payload).await?;
        match bincode::deserialize(&response)
            .map_err(|error| format!("invalid tenant replication response: {error}"))?
        {
            ReplicationRpcResponse::Durable(result) => result,
            ReplicationRpcResponse::Progress(_) => {
                Err("unexpected checkpoint progress response".into())
            }
        }
    }

    async fn request_progress(
        &self,
        target: &crate::control::types::NodeMetadata,
        request: ReplicationRpcRequest,
    ) -> std::result::Result<CheckpointProgress, String> {
        let payload = bincode::serialize(&request).map_err(|error| error.to_string())?;
        let response = self.rpc.request(target, payload).await?;
        match bincode::deserialize(&response)
            .map_err(|error| format!("invalid tenant replication response: {error}"))?
        {
            ReplicationRpcResponse::Progress(result) => result,
            ReplicationRpcResponse::Durable(_) => {
                Err("unexpected durable checkpoint response".into())
            }
        }
    }
}

pub fn open_replica_brain(replica_dir: &Path) -> Result<FluctlightBrain> {
    let direct = replica_dir.to_path_buf();
    let nested = replica_dir.join("brain");
    let brain_path = if storage::is_v4_path(&direct) {
        direct
    } else if storage::is_v4_path(&nested) {
        nested
    } else {
        replica_dir.join("brain.flct")
    };
    store::load_readonly(&brain_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::save_v4_dir;
    use crate::placement::DurabilityPolicy;
    use crate::wal::WalIdentity;
    use crate::{Episode, FluctlightBrain};
    use tempfile::tempdir;

    #[test]
    fn verified_checkpoint_install_is_idempotent_for_same_generation() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(3);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        brain
            .experience(Episode {
                content: "incremental".into(),
                context: "t".into(),
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
        let transfer = CheckpointTransfer::from_active(&primary, identity).unwrap();
        let store = ReplicaStore::new(&replica, identity);
        let first = store.install_checkpoint(transfer.clone()).unwrap();
        let second = store.install_checkpoint(transfer).unwrap();
        assert_eq!(first.durable_watermark, second.durable_watermark);
        let loaded = open_replica_brain(&replica).unwrap();
        assert_eq!(loaded.hippocampus.engrams.len(), 1);
    }

    fn fenced_identity(generation: u64) -> WalIdentity {
        WalIdentity {
            tenant_uuid: uuid::Uuid::from_u128(44),
            writer_epoch: generation,
            fence_generation: generation,
            durability: DurabilityPolicy::Quorum,
        }
    }

    #[test]
    fn verified_checkpoint_is_staged_then_atomically_activated() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(7);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        brain
            .experience(Episode::new("replicated checkpoint", "phase4", 0.8))
            .unwrap();
        save_v4_dir(&brain, &primary).unwrap();

        let checkpoint = CheckpointTransfer::from_active(&primary, identity).unwrap();
        assert!(checkpoint
            .files
            .iter()
            .all(|file| file.length == file.bytes.len() as u64));
        let ack = ReplicaStore::new(&replica, identity)
            .install_checkpoint(checkpoint)
            .unwrap();

        assert_eq!(ack.durable_watermark, brain.wal_seq);
        assert!(!replica.join("staging").exists());
        let loaded = crate::manifest::load_v4_dir(&replica).unwrap();
        assert_eq!(loaded.wal_identity(), Some(identity));
        assert_eq!(loaded.hippocampus.engrams.len(), 1);
    }

    #[test]
    fn corrupt_or_torn_checkpoint_never_replaces_active_generation() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(8);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &primary).unwrap();
        let clean = CheckpointTransfer::from_active(&primary, identity).unwrap();
        ReplicaStore::new(&replica, identity)
            .install_checkpoint(clean.clone())
            .unwrap();
        let active_before = fs::read_to_string(replica.join("CURRENT")).unwrap();

        let mut corrupt = clean;
        let file = corrupt
            .files
            .iter_mut()
            .find(|file| file.name != "manifest.json")
            .unwrap();
        file.bytes.pop();
        let error = ReplicaStore::new(&replica, identity)
            .install_checkpoint(corrupt)
            .unwrap_err();

        assert!(error.to_string().contains("length"), "{error}");
        assert_eq!(
            fs::read_to_string(replica.join("CURRENT")).unwrap(),
            active_before
        );
    }

    #[test]
    fn checkpoint_rejects_stale_or_mixed_fence_identity() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let identity = fenced_identity(9);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &primary).unwrap();
        let transfer = CheckpointTransfer::from_active(&primary, identity).unwrap();

        let error = ReplicaStore::new(dir.path().join("replica").as_path(), fenced_identity(10))
            .install_checkpoint(transfer)
            .unwrap_err();
        assert!(error.to_string().contains("stale or mixed"), "{error}");
    }

    #[test]
    fn contiguous_wal_frames_are_fsynced_before_durable_ack() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(11);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &primary).unwrap();
        ReplicaStore::new(&replica, identity)
            .install_checkpoint(CheckpointTransfer::from_active(&primary, identity).unwrap())
            .unwrap();
        brain.attach_store_path(primary.clone());
        brain
            .experience(Episode::new("wal catch-up mutation", "phase4", 0.9))
            .unwrap();

        let frames = wal::replication_frames(&primary, 0, brain.wal_seq, &identity).unwrap();
        let ack = ReplicaStore::new(&replica, identity)
            .apply_wal_frames(frames)
            .unwrap();

        assert_eq!(ack.durable_watermark, brain.wal_seq);
        assert_eq!(fs::read_to_string(replica.join("DURABLE")).unwrap(), "1\n");
        let loaded = store::load_readonly(&replica).unwrap();
        assert!(loaded
            .activate("wal catch-up mutation")
            .recalls
            .iter()
            .any(|recall| recall.episode.content == "wal catch-up mutation"));
    }

    #[test]
    fn wal_stream_rejects_gaps_corruption_and_stale_primary() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(12);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &primary).unwrap();
        ReplicaStore::new(&replica, identity)
            .install_checkpoint(CheckpointTransfer::from_active(&primary, identity).unwrap())
            .unwrap();
        brain.attach_store_path(primary.clone());
        brain.tick().unwrap();
        brain.tick().unwrap();
        let frames = wal::replication_frames(&primary, 0, 2, &identity).unwrap();

        let gap = ReplicaStore::new(&replica, identity)
            .apply_wal_frames(vec![frames[1].clone()])
            .unwrap_err();
        assert!(gap.to_string().contains("contiguous"), "{gap}");

        let mut corrupt = frames.clone();
        corrupt[0].payload.push(b'x');
        let corrupt_error = ReplicaStore::new(&replica, identity)
            .apply_wal_frames(corrupt)
            .unwrap_err();
        assert!(
            corrupt_error.to_string().contains("SHA-256"),
            "{corrupt_error}"
        );

        let stale = wal::WalReplicationFrame {
            fence_generation: 11,
            ..frames[0].clone()
        };
        let stale_error = ReplicaStore::new(&replica, identity)
            .apply_wal_frames(vec![stale])
            .unwrap_err();
        assert!(
            stale_error.to_string().contains("stale or mixed"),
            "{stale_error}"
        );
        assert!(!replica.join("DURABLE").exists());
    }

    #[test]
    fn checkpoint_chunks_are_bounded_resumable_and_retry_corruption() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(13);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &primary).unwrap();
        let transfer = CheckpointTransfer::from_active(&primary, identity).unwrap();
        let descriptor = transfer.descriptor();
        let store = ReplicaStore::new(&replica, identity);
        let mut progress = store.begin_checkpoint(descriptor.clone()).unwrap();
        assert_eq!(progress.offsets.values().copied().sum::<u64>(), 0);

        let file = &transfer.files[0];
        let end = file.bytes.len().min(MAX_CHECKPOINT_CHUNK_BYTES);
        let valid = CheckpointChunk::new(
            descriptor.transfer_id,
            file.name.clone(),
            0,
            file.bytes[..end].to_vec(),
        );
        let mut corrupt = valid.clone();
        corrupt.bytes[0] ^= 0x80;
        let error = store.write_checkpoint_chunk(corrupt).unwrap_err();
        assert!(error.to_string().contains("chunk SHA-256"), "{error}");
        progress = store.write_checkpoint_chunk(valid).unwrap();
        assert_eq!(progress.offsets[&file.name], end as u64);

        let resumed = store.begin_checkpoint(descriptor.clone()).unwrap();
        assert_eq!(resumed.offsets[&file.name], end as u64);
        for source in &transfer.files {
            let mut offset = resumed.offsets.get(&source.name).copied().unwrap_or(0) as usize;
            while offset < source.bytes.len() {
                let next = (offset + MAX_CHECKPOINT_CHUNK_BYTES).min(source.bytes.len());
                let chunk = CheckpointChunk::new(
                    descriptor.transfer_id,
                    source.name.clone(),
                    offset as u64,
                    source.bytes[offset..next].to_vec(),
                );
                assert!(chunk.bytes.len() <= MAX_CHECKPOINT_CHUNK_BYTES);
                store.write_checkpoint_chunk(chunk).unwrap();
                offset = next;
            }
        }
        let ack = store.finish_checkpoint(descriptor.transfer_id).unwrap();
        assert_eq!(ack.durable_watermark, 0);
        assert!(!replica.join("staging").exists());
    }

    #[test]
    fn duplicate_operation_is_idempotently_acknowledged_without_second_wal_append() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let identity = fenced_identity(14);
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &primary).unwrap();
        let store = ReplicaStore::new(&replica, identity);
        store
            .install_checkpoint(CheckpointTransfer::from_active(&primary, identity).unwrap())
            .unwrap();
        brain.attach_store_path(primary.clone());
        brain.tick().unwrap();
        let frames = wal::replication_frames(&primary, 0, 1, &identity).unwrap();
        let first_operation = frames[0].operation_id.clone();
        let first_hash = frames[0].sha256;
        let first = store.apply_wal_frames(frames.clone()).unwrap();
        let wal_path = wal::wal_path(&replica);
        let length = fs::metadata(&wal_path).unwrap().len();

        let duplicate = store.apply_wal_frames(frames).unwrap();
        assert_eq!(duplicate, first);
        assert_eq!(
            duplicate.operation_id.as_deref(),
            Some(first_operation.as_str())
        );
        assert_eq!(duplicate.mutation_sha256, Some(first_hash));
        assert_eq!(fs::metadata(wal_path).unwrap().len(), length);
    }
}

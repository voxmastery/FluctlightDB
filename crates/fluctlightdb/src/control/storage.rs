use std::fmt::Debug;
use std::io::{Cursor, Write};
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use openraft::storage::{LogFlushed, RaftLogStorage, RaftStateMachine};
use openraft::{
    AnyError, Entry, EntryPayload, LogId, LogState, RaftLogReader, RaftSnapshotBuilder, Snapshot,
    SnapshotMeta, StorageError, StorageIOError, StoredMembership, Vote,
};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::state_machine::{AuthorizedKey, ControlStateMachine, KeyIssuer};
use super::types::{
    ControlCommand, ControlResponse, ControlRole, ControlState, ControlTypeConfig, NodeId,
    NodeMetadata,
};

type ControlEntry = Entry<ControlTypeConfig>;
type ControlStorageError = StorageError<NodeId>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthStoreMigrationReport {
    pub imported: usize,
    pub requires_reissue: Vec<String>,
}

#[derive(Clone)]
pub struct SqliteControlStore {
    path: PathBuf,
    snapshot_dir: PathBuf,
    pepper: [u8; 32],
    machine: Arc<RwLock<ControlStateMachine>>,
}

impl SqliteControlStore {
    pub fn open(
        path: impl AsRef<Path>,
        snapshot_dir: impl AsRef<Path>,
        pepper: &[u8],
    ) -> Result<Self, String> {
        let pepper: [u8; 32] = pepper
            .try_into()
            .map_err(|_| "cluster pepper must be exactly 32 bytes".to_string())?;
        let path = path.as_ref().to_path_buf();
        let snapshot_dir = snapshot_dir.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::create_dir_all(&snapshot_dir).map_err(|error| error.to_string())?;
        let connection = open_connection(&path)?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS raft_meta (
                    name TEXT PRIMARY KEY,
                    value BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS raft_log (
                    log_index INTEGER PRIMARY KEY,
                    entry BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS control_state (
                    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                    state BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS snapshots (
                    snapshot_id TEXT PRIMARY KEY,
                    meta BLOB NOT NULL,
                    path TEXT NOT NULL UNIQUE,
                    created_revision INTEGER NOT NULL
                );",
            )
            .map_err(|error| error.to_string())?;
        let state = connection
            .query_row(
                "SELECT state FROM control_state WHERE singleton = 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .ok()
            .map(|bytes| serde_json::from_slice::<ControlState>(&bytes))
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        let machine = ControlStateMachine::from_state(&pepper, state)?;
        Ok(Self {
            path,
            snapshot_dir,
            pepper,
            machine: Arc::new(RwLock::new(machine)),
        })
    }

    pub fn authorize(&self, secret: &str, now_unix_ms: u64) -> Option<AuthorizedKey> {
        self.machine.read().ok()?.authorize(secret, now_unix_ms)
    }

    #[cfg(test)]
    fn apply_control(&self, command: ControlCommand) -> Result<ControlResponse, String> {
        let mut guard = self
            .machine
            .write()
            .map_err(|_| "control state lock poisoned".to_string())?;
        let mut next = guard.clone();
        let response = next.apply(command)?;
        persist_state(&self.path, next.state())?;
        *guard = next;
        Ok(response)
    }

    pub fn state(&self) -> Result<ControlState, String> {
        self.machine
            .read()
            .map(|machine| machine.state().clone())
            .map_err(|_| "control state lock poisoned".to_string())
    }

    pub fn snapshot_dir(&self) -> &Path {
        &self.snapshot_dir
    }

    pub fn pepper(&self) -> &[u8; 32] {
        &self.pepper
    }

    pub fn split(&self) -> Result<(SqliteLogStore, SqliteStateMachine), String> {
        let connection = open_connection(&self.path)?;
        let last_applied = read_meta::<LogId<NodeId>>(&connection, "last_applied")?;
        let membership =
            read_meta::<StoredMembership<NodeId, NodeMetadata>>(&connection, "last_membership")?
                .unwrap_or_default();
        Ok((
            SqliteLogStore {
                path: self.path.clone(),
            },
            SqliteStateMachine {
                store: self.clone(),
                last_applied,
                membership,
            },
        ))
    }

    pub fn migrate_auth_store(
        &self,
        legacy_path: impl AsRef<Path>,
    ) -> Result<AuthStoreMigrationReport, String> {
        let legacy_path = legacy_path.as_ref().to_path_buf();
        let source = Connection::open(&legacy_path).map_err(|error| error.to_string())?;
        let mut statement = source
            .prepare(
                "SELECT kid, tenant_id, key_secret, role, created_at, expires_at, revoked
                 FROM api_keys ORDER BY kid",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, bool>(6)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        drop(source);
        let issuer = KeyIssuer::new(&self.pepper)?;
        let mut report = AuthStoreMigrationReport::default();
        let mut plaintext_to_scrub = Vec::new();
        let mut machine = self
            .machine
            .read()
            .map_err(|_| "control state lock poisoned".to_string())?
            .clone();
        if machine.state().revision != 0 {
            return Err("AuthStore migration is only allowed before Raft bootstrap".to_string());
        }

        for row in rows {
            let (key_id, tenant_id, secret, role, created_at, expires_at, revoked) = row;
            if !secret.starts_with("fld_") {
                report.requires_reissue.push(key_id);
                continue;
            }
            let role = match role.as_str() {
                "read" => ControlRole::Read,
                "write" => ControlRole::Write,
                "admin" => ControlRole::Admin,
                "platform" => ControlRole::Platform,
                _ => {
                    report.requires_reissue.push(key_id);
                    continue;
                }
            };
            let created_at_unix_ms = u64::try_from(created_at)
                .unwrap_or_default()
                .saturating_mul(1_000);
            let mut metadata = issuer.metadata_for_secret(
                &key_id,
                tenant_id,
                role,
                created_at_unix_ms,
                expires_at
                    .and_then(|value| u64::try_from(value).ok())
                    .map(|value| value.saturating_mul(1_000)),
                &secret,
            )?;
            if revoked {
                metadata.revoked_at_unix_ms = Some(created_at_unix_ms);
            }
            let response = machine.apply(ControlCommand::IssueKey {
                request_id: format!("auth-store-migration:{key_id}"),
                metadata,
            })?;
            if matches!(response, ControlResponse::Applied { .. }) {
                report.imported += 1;
                plaintext_to_scrub.push((key_id, secret));
            }
        }
        persist_state(&self.path, machine.state())?;
        let mut source = Connection::open(&legacy_path).map_err(|error| error.to_string())?;
        let transaction = source
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        for (key_id, plaintext) in plaintext_to_scrub {
            transaction
                .execute(
                    "UPDATE api_keys SET key_secret = ?1
                     WHERE kid = ?2 AND key_secret = ?3",
                    params![crate::auth::hash_api_key(&plaintext), key_id, plaintext],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        source
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(|error| error.to_string())?;
        *self
            .machine
            .write()
            .map_err(|_| "control state lock poisoned".to_string())? = machine;
        Ok(report)
    }
}

#[derive(Clone)]
pub struct SqliteLogStore {
    path: PathBuf,
}

#[derive(Clone)]
pub struct SqliteStateMachine {
    store: SqliteControlStore,
    last_applied: Option<LogId<NodeId>>,
    membership: StoredMembership<NodeId, NodeMetadata>,
}

#[derive(Clone)]
pub struct SqliteSnapshotBuilder {
    store: SqliteControlStore,
    last_applied: Option<LogId<NodeId>>,
    membership: StoredMembership<NodeId, NodeMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotEnvelope {
    last_applied: Option<LogId<NodeId>>,
    membership: StoredMembership<NodeId, NodeMetadata>,
    state: ControlState,
}

impl RaftLogReader<ControlTypeConfig> for SqliteLogStore {
    async fn try_get_log_entries<RB>(
        &mut self,
        range: RB,
    ) -> Result<Vec<ControlEntry>, ControlStorageError>
    where
        RB: RangeBounds<u64> + Clone + Debug + Send,
    {
        let connection = open_connection(&self.path).map_err(storage_read)?;
        let mut statement = connection
            .prepare("SELECT log_index, entry FROM raft_log ORDER BY log_index")
            .map_err(storage_read)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(storage_read)?;
        let mut entries = Vec::new();
        for row in rows {
            let (index, bytes) = row.map_err(storage_read)?;
            if range.contains(&index) {
                entries.push(bincode::deserialize(&bytes).map_err(storage_read)?);
            }
        }
        Ok(entries)
    }
}

impl RaftLogStorage<ControlTypeConfig> for SqliteLogStore {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<ControlTypeConfig>, ControlStorageError> {
        let connection = open_connection(&self.path).map_err(storage_read)?;
        let last_purged_log_id = read_meta(&connection, "last_purged").map_err(storage_read)?;
        let last_present = connection
            .query_row(
                "SELECT entry FROM raft_log ORDER BY log_index DESC LIMIT 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .ok()
            .map(|bytes| bincode::deserialize::<ControlEntry>(&bytes))
            .transpose()
            .map_err(storage_read)?
            .map(|entry| entry.log_id);
        Ok(LogState {
            last_log_id: last_present.or(last_purged_log_id),
            last_purged_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), ControlStorageError> {
        write_meta(&self.path, "vote", vote).map_err(storage_write)
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, ControlStorageError> {
        let connection = open_connection(&self.path).map_err(storage_read)?;
        read_meta(&connection, "vote").map_err(storage_read)
    }

    async fn save_committed(
        &mut self,
        committed: Option<LogId<NodeId>>,
    ) -> Result<(), ControlStorageError> {
        write_meta(&self.path, "committed", &committed).map_err(storage_write)
    }

    async fn read_committed(&mut self) -> Result<Option<LogId<NodeId>>, ControlStorageError> {
        let connection = open_connection(&self.path).map_err(storage_read)?;
        Ok(read_meta::<Option<LogId<NodeId>>>(&connection, "committed")
            .map_err(storage_read)?
            .flatten())
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<ControlTypeConfig>,
    ) -> Result<(), ControlStorageError>
    where
        I: IntoIterator<Item = ControlEntry> + Send,
        I::IntoIter: Send,
    {
        let result = (|| {
            let mut connection = open_connection(&self.path)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            for entry in entries {
                let bytes = bincode::serialize(&entry).map_err(|error| error.to_string())?;
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO raft_log(log_index, entry) VALUES(?1, ?2)",
                        params![entry.log_id.index, bytes],
                    )
                    .map_err(|error| error.to_string())?;
            }
            transaction.commit().map_err(|error| error.to_string())
        })();
        match result {
            Ok(()) => {
                callback.log_io_completed(Ok(()));
                Ok(())
            }
            Err(error) => {
                callback.log_io_completed(Err(std::io::Error::other(error.clone())));
                Err(storage_write(error))
            }
        }
    }

    async fn truncate(&mut self, log_id: LogId<NodeId>) -> Result<(), ControlStorageError> {
        let connection = open_connection(&self.path).map_err(storage_write)?;
        connection
            .execute(
                "DELETE FROM raft_log WHERE log_index >= ?1",
                params![log_id.index],
            )
            .map_err(storage_write)?;
        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<NodeId>) -> Result<(), ControlStorageError> {
        let mut connection = open_connection(&self.path).map_err(storage_write)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_write)?;
        transaction
            .execute(
                "DELETE FROM raft_log WHERE log_index <= ?1",
                params![log_id.index],
            )
            .map_err(storage_write)?;
        set_meta(&transaction, "last_purged", &log_id).map_err(storage_write)?;
        transaction.commit().map_err(storage_write)
    }
}

impl RaftStateMachine<ControlTypeConfig> for SqliteStateMachine {
    type SnapshotBuilder = SqliteSnapshotBuilder;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<NodeId>>,
            StoredMembership<NodeId, NodeMetadata>,
        ),
        ControlStorageError,
    > {
        Ok((self.last_applied, self.membership.clone()))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<ControlResponse>, ControlStorageError>
    where
        I: IntoIterator<Item = ControlEntry> + Send,
        I::IntoIter: Send,
    {
        let mut machine = self
            .store
            .machine
            .read()
            .map_err(|_| storage_write("control state lock poisoned"))?
            .clone();
        let mut last_applied = self.last_applied;
        let mut membership = self.membership.clone();
        let mut responses = Vec::new();
        for entry in entries {
            last_applied = Some(entry.log_id);
            match entry.payload {
                EntryPayload::Blank => responses.push(ControlResponse::default()),
                EntryPayload::Normal(command) => {
                    responses.push(machine.apply(command).map_err(storage_write)?);
                }
                EntryPayload::Membership(value) => {
                    membership = StoredMembership::new(last_applied, value);
                    responses.push(ControlResponse::default());
                }
            }
        }

        persist_applied(&self.store.path, machine.state(), last_applied, &membership)
            .map_err(storage_write)?;
        *self
            .store
            .machine
            .write()
            .map_err(|_| storage_write("control state lock poisoned"))? = machine;
        self.last_applied = last_applied;
        self.membership = membership;
        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        SqliteSnapshotBuilder {
            store: self.store.clone(),
            last_applied: self.last_applied,
            membership: self.membership.clone(),
        }
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, ControlStorageError> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, NodeMetadata>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), ControlStorageError> {
        let bytes = snapshot.into_inner();
        let envelope: SnapshotEnvelope = serde_json::from_slice(&bytes).map_err(storage_write)?;
        if envelope.last_applied != meta.last_log_id || envelope.membership != meta.last_membership
        {
            return Err(storage_write("snapshot metadata does not match payload"));
        }
        store_snapshot(&self.store, meta, &bytes).map_err(storage_write)?;
        persist_applied(
            &self.store.path,
            &envelope.state,
            envelope.last_applied,
            &envelope.membership,
        )
        .map_err(storage_write)?;
        *self
            .store
            .machine
            .write()
            .map_err(|_| storage_write("control state lock poisoned"))? =
            ControlStateMachine::from_state(self.store.pepper(), envelope.state)
                .map_err(storage_write)?;
        self.last_applied = envelope.last_applied;
        self.membership = envelope.membership;
        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<ControlTypeConfig>>, ControlStorageError> {
        load_current_snapshot(&self.store).map_err(storage_read)
    }
}

impl RaftSnapshotBuilder<ControlTypeConfig> for SqliteSnapshotBuilder {
    async fn build_snapshot(&mut self) -> Result<Snapshot<ControlTypeConfig>, ControlStorageError> {
        let state = self.store.state().map_err(storage_read)?;
        let envelope = SnapshotEnvelope {
            last_applied: self.last_applied,
            membership: self.membership.clone(),
            state,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(storage_write)?;
        let last = self
            .last_applied
            .map(|log| {
                format!(
                    "{}-{}-{}",
                    log.leader_id.term, log.leader_id.node_id, log.index
                )
            })
            .unwrap_or_else(|| "empty".to_string());
        let digest = Sha256::digest(&bytes);
        let snapshot_id = format!("{last}-{}", hex_bytes(&digest[..8]));
        let meta = SnapshotMeta {
            last_log_id: self.last_applied,
            last_membership: self.membership.clone(),
            snapshot_id,
        };
        store_snapshot(&self.store, &meta, &bytes).map_err(storage_write)?;
        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(bytes)),
        })
    }
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

fn persist_state(path: &Path, state: &ControlState) -> Result<(), String> {
    let mut connection = open_connection(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(state).map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO control_state(singleton, state) VALUES(1, ?1)
             ON CONFLICT(singleton) DO UPDATE SET state = excluded.state",
            params![bytes],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

fn persist_applied(
    path: &Path,
    state: &ControlState,
    last_applied: Option<LogId<NodeId>>,
    membership: &StoredMembership<NodeId, NodeMetadata>,
) -> Result<(), String> {
    let mut connection = open_connection(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let state_bytes = serde_json::to_vec(state).map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO control_state(singleton, state) VALUES(1, ?1)
             ON CONFLICT(singleton) DO UPDATE SET state = excluded.state",
            params![state_bytes],
        )
        .map_err(|error| error.to_string())?;
    set_meta(&transaction, "last_applied", &last_applied)?;
    set_meta(&transaction, "last_membership", membership)?;
    transaction.commit().map_err(|error| error.to_string())
}

fn write_meta<T: Serialize>(path: &Path, name: &str, value: &T) -> Result<(), String> {
    let mut connection = open_connection(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    set_meta(&transaction, name, value)?;
    transaction.commit().map_err(|error| error.to_string())
}

fn set_meta<T: Serialize>(connection: &Connection, name: &str, value: &T) -> Result<(), String> {
    let bytes = bincode::serialize(value).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO raft_meta(name, value) VALUES(?1, ?2)
             ON CONFLICT(name) DO UPDATE SET value = excluded.value",
            params![name, bytes],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn read_meta<T: for<'de> Deserialize<'de>>(
    connection: &Connection,
    name: &str,
) -> Result<Option<T>, String> {
    connection
        .query_row(
            "SELECT value FROM raft_meta WHERE name = ?1",
            params![name],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .ok()
        .map(|bytes| bincode::deserialize(&bytes).map_err(|error| error.to_string()))
        .transpose()
}

fn store_snapshot(
    store: &SqliteControlStore,
    meta: &SnapshotMeta<NodeId, NodeMetadata>,
    bytes: &[u8],
) -> Result<(), String> {
    let file_name = format!(
        "{}.snapshot",
        hex_bytes(&Sha256::digest(meta.snapshot_id.as_bytes()))
    );
    let path = store.snapshot_dir.join(file_name);
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            std::fs::File::open(&store.snapshot_dir)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| error.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(&path).map_err(|error| error.to_string())? != bytes {
                return Err("immutable snapshot id already has different content".to_string());
            }
        }
        Err(error) => return Err(error.to_string()),
    }

    let mut connection = open_connection(&store.path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let meta_bytes = bincode::serialize(meta).map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO snapshots(snapshot_id, meta, path, created_revision)
             VALUES(?1, ?2, ?3, ?4)",
            params![
                meta.snapshot_id,
                meta_bytes,
                path.to_string_lossy(),
                store.state()?.revision
            ],
        )
        .map_err(|error| error.to_string())?;
    set_meta(&transaction, "current_snapshot", &meta.snapshot_id)?;
    transaction.commit().map_err(|error| error.to_string())
}

fn load_current_snapshot(
    store: &SqliteControlStore,
) -> Result<Option<Snapshot<ControlTypeConfig>>, String> {
    let connection = open_connection(&store.path)?;
    let Some(snapshot_id) = read_meta::<String>(&connection, "current_snapshot")? else {
        return Ok(None);
    };
    let row = connection
        .query_row(
            "SELECT meta, path FROM snapshots WHERE snapshot_id = ?1",
            params![snapshot_id],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let meta = bincode::deserialize(&row.0).map_err(|error| error.to_string())?;
    let bytes = std::fs::read(row.1).map_err(|error| error.to_string())?;
    Ok(Some(Snapshot {
        meta,
        snapshot: Box::new(Cursor::new(bytes)),
    }))
}

fn storage_read(error: impl std::fmt::Display) -> ControlStorageError {
    StorageIOError::read(AnyError::error(error.to_string())).into()
}

fn storage_write(error: impl std::fmt::Display) -> ControlStorageError {
    StorageIOError::write(AnyError::error(error.to_string())).into()
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::state_machine::KeyIssuer;
    use crate::control::types::{ControlCommand, ControlRole, ControlTypeConfig};
    use openraft::testing::{StoreBuilder, Suite};

    #[test]
    fn sqlite_state_survives_reopen_without_persisting_plaintext_key() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("control.sqlite");
        let snapshots = dir.path().join("snapshots");
        let pepper = [9; 32];
        let issuer = KeyIssuer::new(&pepper).unwrap();
        let issued = issuer
            .issue("key-1", "tenant-a", ControlRole::Admin, 1, None)
            .unwrap();

        {
            let store = SqliteControlStore::open(&db, &snapshots, &pepper).unwrap();
            store
                .apply_control(ControlCommand::IssueKey {
                    request_id: "issue-1".into(),
                    metadata: issued.metadata,
                })
                .unwrap();
        }

        let bytes = std::fs::read(&db).unwrap();
        assert!(!bytes
            .windows(issued.secret.len())
            .any(|window| window == issued.secret.as_bytes()));

        let reopened = SqliteControlStore::open(&db, &snapshots, &pepper).unwrap();
        assert_eq!(
            reopened.authorize(&issued.secret, 2).unwrap().tenant_id,
            "tenant-a"
        );
    }

    struct Builder;

    impl StoreBuilder<ControlTypeConfig, SqliteLogStore, SqliteStateMachine, tempfile::TempDir>
        for Builder
    {
        async fn build(
            &self,
        ) -> Result<(tempfile::TempDir, SqliteLogStore, SqliteStateMachine), ControlStorageError>
        {
            let directory = tempfile::tempdir().map_err(storage_write)?;
            let store = SqliteControlStore::open(
                directory.path().join("control.sqlite"),
                directory.path().join("snapshots"),
                &[3; 32],
            )
            .map_err(storage_write)?;
            let (log, state_machine) = store.split().map_err(storage_write)?;
            Ok((directory, log, state_machine))
        }
    }

    #[test]
    fn sqlite_implementation_passes_openraft_storage_v2_conformance() {
        Suite::<
            ControlTypeConfig,
            SqliteLogStore,
            SqliteStateMachine,
            Builder,
            tempfile::TempDir,
        >::test_all(Builder)
        .unwrap();
    }

    #[test]
    fn auth_store_migration_imports_plaintext_once_and_flags_hashes_for_reissue() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("auth.db");
        let connection = Connection::open(&legacy).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE api_keys (
                    kid TEXT PRIMARY KEY,
                    tenant_id TEXT NOT NULL,
                    key_secret TEXT NOT NULL UNIQUE,
                    role TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER,
                    revoked INTEGER NOT NULL DEFAULT 0
                );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO api_keys VALUES('plain', 'tenant-a', 'fld_legacy', 'read', 1, NULL, 0)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO api_keys VALUES('hashed', 'tenant-b', ?1, 'admin', 1, NULL, 0)",
                params![crate::auth::hash_api_key("fld_unavailable")],
            )
            .unwrap();
        drop(connection);

        let store = SqliteControlStore::open(
            dir.path().join("control.sqlite"),
            dir.path().join("snapshots"),
            &[4; 32],
        )
        .unwrap();
        let report = store.migrate_auth_store(&legacy).unwrap();

        assert_eq!(report.imported, 1);
        assert_eq!(report.requires_reissue, vec!["hashed"]);
        assert_eq!(
            store.authorize("fld_legacy", 2_000).unwrap().tenant_id,
            "tenant-a"
        );
        let legacy_bytes = std::fs::read(&legacy).unwrap();
        assert!(!legacy_bytes
            .windows(b"fld_legacy".len())
            .any(|window| window == b"fld_legacy"));
    }
}

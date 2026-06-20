//! SQLite-backed API key store with rotation support.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::auth::{format_key_entry, generate_api_key, role_name, Role};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKey {
    pub kid: String,
    pub tenant_id: String,
    pub key: String,
    pub role: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub revoked: bool,
}

pub struct AuthStore {
    path: PathBuf,
}

impl AuthStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(Error::Io)?;
        }
        let store = Self { path };
        store.init_schema()?;
        Ok(store)
    }

    pub fn default_path() -> PathBuf {
        crate::tenant::default_tenant_root().join("auth.db")
    }

    fn conn(&self) -> Result<Connection> {
        Connection::open(&self.path).map_err(|e| Error::Store(e.to_string()))
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_keys (
                kid TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                key_secret TEXT NOT NULL UNIQUE,
                role TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER,
                revoked INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_api_keys_secret ON api_keys(key_secret);",
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    pub fn issue_key(&self, tenant_id: &str, role: Role) -> Result<StoredKey> {
        let key = generate_api_key();
        let kid = uuid::Uuid::new_v4().simple().to_string();
        let now = chrono_now();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO api_keys (kid, tenant_id, key_secret, role, created_at, revoked)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![kid, tenant_id, key, role_name(role), now],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(StoredKey {
            kid,
            tenant_id: tenant_id.to_string(),
            key,
            role: role_name(role).to_string(),
            created_at: now,
            expires_at: None,
            revoked: false,
        })
    }

    pub fn revoke_key(&self, kid: &str) -> Result<bool> {
        let conn = self.conn()?;
        let n = conn
            .execute(
                "UPDATE api_keys SET revoked = 1 WHERE kid = ?1",
                params![kid],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(n > 0)
    }

    pub fn lookup(&self, secret: &str) -> Option<(String, Role)> {
        let conn = self.conn().ok()?;
        let mut stmt = conn
            .prepare(
                "SELECT tenant_id, role FROM api_keys
                 WHERE key_secret = ?1 AND revoked = 0",
            )
            .ok()?;
        let row = stmt.query_row(params![secret], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        });
        match row {
            Ok((tenant, role_s)) => {
                let role = match role_s.as_str() {
                    "read" => Role::Read,
                    "write" => Role::Write,
                    _ => Role::Admin,
                };
                Some((tenant, role))
            }
            Err(_) => None,
        }
    }

    pub fn list_tenants(&self) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT DISTINCT tenant_id FROM api_keys WHERE revoked = 0 ORDER BY tenant_id")
            .map_err(|e| Error::Store(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn export_env_keys(&self) -> Result<String> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT tenant_id, key_secret, role FROM api_keys WHERE revoked = 0 ORDER BY tenant_id",
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| Error::Store(e.to_string()))?;
        let mut parts = Vec::new();
        for row in rows.flatten() {
            let (tenant, key, role_s) = row;
            let role = match role_s.as_str() {
                "read" => Role::Read,
                "write" => Role::Write,
                _ => Role::Admin,
            };
            parts.push(format_key_entry(&tenant, &key, role));
        }
        Ok(parts.join(","))
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

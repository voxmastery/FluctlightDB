//! SQLite-backed API key store with rotation support (CAB: hashed secrets + expiry).

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::auth::{generate_api_key, hash_api_key, role_name, Role};
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
        self.issue_key_with_expiry(tenant_id, role, None)
    }

    pub fn issue_key_with_expiry(
        &self,
        tenant_id: &str,
        role: Role,
        expires_at: Option<i64>,
    ) -> Result<StoredKey> {
        let key = generate_api_key();
        let secret_hash = hash_api_key(&key);
        let kid = uuid::Uuid::new_v4().simple().to_string();
        let now = chrono_now();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO api_keys (kid, tenant_id, key_secret, role, created_at, expires_at, revoked)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![kid, tenant_id, secret_hash, role_name(role), now, expires_at],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(StoredKey {
            kid,
            tenant_id: tenant_id.to_string(),
            key,
            role: role_name(role).to_string(),
            created_at: now,
            expires_at,
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
        let now = chrono_now();
        let hashed = hash_api_key(secret);
        let conn = self.conn().ok()?;
        // Prefer hashed secrets; also accept legacy plaintext rows (fld_*) once.
        let mut stmt = conn
            .prepare(
                "SELECT tenant_id, role, key_secret FROM api_keys
                 WHERE revoked = 0
                   AND (key_secret = ?1 OR key_secret = ?2)
                   AND (expires_at IS NULL OR expires_at > ?3)",
            )
            .ok()?;
        let row = stmt.query_row(params![hashed, secret, now], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        });
        match row {
            Ok((tenant, role_s, stored_secret)) => {
                let role = Role::parse(&role_s)?;
                // Upgrade legacy plaintext to hash on successful lookup.
                if stored_secret == secret && secret.starts_with("fld_") {
                    let _ = conn.execute(
                        "UPDATE api_keys SET key_secret = ?1 WHERE key_secret = ?2 AND revoked = 0",
                        params![hashed, secret],
                    );
                }
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
        // Hashed secrets cannot be re-exported as usable plaintext keys.
        Err(Error::Store(
            "export_env_keys unavailable after hashed secrets; re-issue keys".into(),
        ))
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

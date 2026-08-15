//! API key authentication and RBAC for serve (CAB capability mapping).

use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Read,
    Write,
    /// Govern bound brain only (compact / death / forget / mark-core).
    Admin,
    /// Control-plane only (provision / revoke / list). Does not imply brain write.
    Platform,
}

impl Role {
    /// Capability check: Platform only satisfies Platform; others use classic ladder.
    pub fn allows(&self, required: Role) -> bool {
        match (self, required) {
            (Role::Platform, Role::Platform) => true,
            (Role::Platform, _) => false,
            (_, Role::Platform) => false,
            (Role::Admin, _) => true,
            (Role::Write, Role::Write | Role::Read) => true,
            (Role::Read, Role::Read) => true,
            _ => false,
        }
    }

    pub fn parse(name: &str) -> Option<Role> {
        match name {
            "read" => Some(Role::Read),
            "write" => Some(Role::Write),
            "admin" => Some(Role::Admin),
            "platform" => Some(Role::Platform),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub tenant_id: String,
    pub role: Role,
}

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub keys: HashMap<String, (String, Role)>,
    pub require_auth: bool,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self {
            require_auth: env::var("FLUCTLIGHT_REQUIRE_AUTH")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            keys: HashMap::new(),
        };
        if let Ok(raw) = env::var("FLUCTLIGHT_API_KEYS") {
            for part in raw.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                let pieces: Vec<&str> = part.split(':').collect();
                if pieces.len() >= 3 {
                    let tenant = pieces[0].to_string();
                    let key = pieces[1].to_string();
                    let Some(role) = Role::parse(pieces[2]) else {
                        eprintln!(
                            "fluctlight auth: ignoring API key with unknown role {:?}",
                            pieces[2]
                        );
                        continue;
                    };
                    cfg.keys.insert(key, (tenant, role));
                }
            }
        }
        if !cfg.keys.is_empty() {
            cfg.require_auth = true;
        }
        cfg
    }

    pub fn authorize(
        &self,
        bearer: Option<&str>,
        tenant_hint: Option<&str>,
    ) -> Option<AuthContext> {
        // Open mode: Admin on BrainId "default" only (CAB realm). Ignore attacker hints.
        if self.keys.is_empty() && !self.require_auth {
            let _ = tenant_hint;
            return Some(AuthContext {
                tenant_id: "default".to_string(),
                role: Role::Admin,
            });
        }
        if self.keys.is_empty() && self.require_auth {
            // Fall through to auth store lookup.
        }
        let token = bearer?;
        if let Some((tenant, role)) = self.keys.get(token) {
            return Some(AuthContext {
                tenant_id: tenant.clone(),
                role: *role,
            });
        }
        if let Ok(store) =
            crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path())
        {
            if let Some((tenant, role)) = store.lookup(token) {
                return Some(AuthContext {
                    tenant_id: tenant,
                    role,
                });
            }
        }
        None
    }

    pub fn check_role(ctx: &AuthContext, required: Role) -> bool {
        ctx.role.allows(required)
    }
}

pub fn generate_api_key() -> String {
    format!("fld_{}", uuid::Uuid::new_v4().simple())
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Read => "read",
        Role::Write => "write",
        Role::Admin => "admin",
        Role::Platform => "platform",
    }
}

pub fn format_key_entry(tenant: &str, key: &str, role: Role) -> String {
    format!("{}:{}:{}", tenant, key, role_name(role))
}

pub fn hash_api_key(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_allows_write_platform_does_not() {
        assert!(Role::Admin.allows(Role::Write));
        assert!(!Role::Read.allows(Role::Write));
        assert!(Role::Platform.allows(Role::Platform));
        assert!(!Role::Platform.allows(Role::Write));
        assert!(!Role::Admin.allows(Role::Platform));
    }

    #[test]
    fn unknown_role_parse_rejects() {
        assert!(Role::parse("superuser").is_none());
        assert_eq!(Role::parse("admin"), Some(Role::Admin));
    }

    #[test]
    fn open_mode_pins_default_ignores_hint() {
        let cfg = AuthConfig::default();
        let ctx = cfg.authorize(None, Some("../PWNED")).unwrap();
        assert_eq!(ctx.tenant_id, "default");
        assert_eq!(ctx.role, Role::Admin);
    }
}

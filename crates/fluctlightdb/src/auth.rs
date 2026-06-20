//! API key authentication and RBAC for serve.

use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Read,
    Write,
    Admin,
}

impl Role {
    pub fn allows(&self, required: Role) -> bool {
        *self >= required
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
                    let role = match pieces[2] {
                        "read" => Role::Read,
                        "write" => Role::Write,
                        "admin" => Role::Admin,
                        _ => Role::Write,
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

    pub fn authorize(&self, bearer: Option<&str>, tenant_hint: Option<&str>) -> Option<AuthContext> {
        if self.keys.is_empty() && !self.require_auth {
            return Some(AuthContext {
                tenant_id: tenant_hint.unwrap_or("default").to_string(),
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
        if let Ok(store) = crate::auth_store::AuthStore::open(crate::auth_store::AuthStore::default_path()) {
            if let Some((tenant, role)) = store.lookup(token) {
                return Some(AuthContext { tenant_id: tenant, role });
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
    }
}

pub fn format_key_entry(tenant: &str, key: &str, role: Role) -> String {
    format!("{}:{}:{}", tenant, key, role_name(role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_allows_write() {
        assert!(Role::Admin.allows(Role::Write));
        assert!(!Role::Read.allows(Role::Write));
    }
}

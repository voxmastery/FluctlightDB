//! Adversarial tests for auth, API keys, and tenant isolation boundaries.

use fluctlightdb::auth::{AuthConfig, AuthContext, Role};
use fluctlightdb::auth_store::AuthStore;
use fluctlightdb::test_env::EnvGuard;
use tempfile::tempdir;

#[test]
fn auth_rejects_missing_bearer_when_keys_configured() {
    let _env = EnvGuard::acquire(&["FLUCTLIGHT_API_KEYS"]);
    _env.set("FLUCTLIGHT_API_KEYS", "tenant_a:secret_a:write");
    let cfg = AuthConfig::from_env();
    assert!(cfg.authorize(None, None).is_none());
    assert!(cfg.authorize(Some("wrong"), None).is_none());
    assert!(cfg.authorize(Some("secret_a"), None).is_some());
}

#[test]
fn auth_key_binds_tenant_not_hint_on_mismatch() {
    let _env = EnvGuard::acquire(&["FLUCTLIGHT_API_KEYS"]);
    _env.set("FLUCTLIGHT_API_KEYS", "tenant_a:secret_a:write");
    let cfg = AuthConfig::from_env();
    let ctx = cfg
        .authorize(Some("secret_a"), Some("tenant_b"))
        .expect("secret_a must authorize under locked env");
    // Key wins — caller must enforce path/body tenant matches ctx (serve does).
    assert_eq!(ctx.tenant_id, "tenant_a");
}

#[test]
fn auth_read_role_cannot_satisfy_write() {
    let ctx = AuthContext {
        tenant_id: "t".into(),
        role: Role::Read,
    };
    assert!(!AuthConfig::check_role(&ctx, Role::Write));
    assert!(AuthConfig::check_role(&ctx, Role::Read));
}

#[test]
fn auth_store_revoked_key_not_found() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("auth.db");
    let store = AuthStore::open(&path).unwrap();
    let issued = store.issue_key("tenant_a", Role::Write).unwrap();
    assert!(store.lookup(&issued.key).is_some());
    assert!(store.revoke_key(&issued.kid).unwrap());
    assert!(store.lookup(&issued.key).is_none());
}

#[test]
fn auth_store_tenant_keys_do_not_cross_leak() {
    let dir = tempdir().unwrap();
    let store = AuthStore::open(dir.path().join("auth.db")).unwrap();
    let a = store.issue_key("tenant_a", Role::Write).unwrap();
    let b = store.issue_key("tenant_b", Role::Write).unwrap();
    let (ta, _) = store.lookup(&a.key).unwrap();
    let (tb, _) = store.lookup(&b.key).unwrap();
    assert_eq!(ta, "tenant_a");
    assert_eq!(tb, "tenant_b");
    assert_ne!(ta, tb);
}

#[test]
fn auth_store_garbage_token_never_authorizes() {
    let dir = tempdir().unwrap();
    let store = AuthStore::open(dir.path().join("auth.db")).unwrap();
    let _ = store.issue_key("tenant_a", Role::Admin).unwrap();
    for garbage in ["", "fld_not-real", "tenant_a:fake:admin", "\0"] {
        assert!(
            store.lookup(garbage).is_none(),
            "unexpected hit for {garbage:?}"
        );
    }
    assert!(store.lookup(&" ".repeat(64)).is_none());
}

#[test]
fn auth_malformed_env_entries_ignored() {
    let _env = EnvGuard::acquire(&["FLUCTLIGHT_API_KEYS"]);
    _env.set(
        "FLUCTLIGHT_API_KEYS",
        "badentry,tenant_a:goodkey:write,also-bad",
    );
    let cfg = AuthConfig::from_env();
    assert!(cfg.authorize(Some("goodkey"), None).is_some());
    assert!(cfg.authorize(Some("badentry"), None).is_none());
}

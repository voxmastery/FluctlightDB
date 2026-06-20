# Tenant provisioning

Layout:

```
~/.fluctlight/tenants/{tenant_id}/brain.flct
~/.fluctlight/tenants/{tenant_id}/brain.flct.wal*
```

HTTP routes:

```
POST /api/v1/tenants/{tenant_id}/experience
POST /api/v1/tenants/{tenant_id}/activate
POST /api/v1/tenants/{tenant_id}/status
```

Map your application's user or session id → `tenant_id` in your agent config, and pass `agent_id` on experience/activate for scoped recall.

Auth: `FLUCTLIGHT_API_KEYS=tenant1:key1:write,tenant2:key2:read`

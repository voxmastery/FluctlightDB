# Backup and restore

## Daily backup

From your FluctlightDB clone:

```bash
./scripts/fluctlight-backup.sh
```

Or with explicit paths:

```bash
FLUCTLIGHT_BRAIN_PATH=~/.fluctlight/tenants/default/brain ./scripts/fluctlight-backup.sh
```

Enable timer:

```bash
sudo cp systemd/fluctlight-backup.* /etc/systemd/system/
sudo systemctl enable --now fluctlight-backup.timer
```

See [systemd/README.md](../systemd/README.md) for production layout (`/etc/fluctlight/environment`).

## Restore

```bash
sudo systemctl stop fluctlight-serve
./scripts/fluctlight-restore.sh ~/.fluctlight/backups/TIMESTAMP
sudo systemctl start fluctlight-serve
```

Verify with `fluctlight status --path ~/.fluctlight/tenants/default/brain`.

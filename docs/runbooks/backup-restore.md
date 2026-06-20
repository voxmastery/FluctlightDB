# Backup and restore

## Daily backup

```bash
/home/ambugo/fluctlightdb/scripts/fluctlight-backup.sh
```

Enable timer:

```bash
sudo cp /home/ambugo/fluctlightdb/systemd/fluctlight-backup.* /etc/systemd/system/
sudo systemctl enable --now fluctlight-backup.timer
```

## Restore

```bash
sudo systemctl stop fluctlight-serve
/home/ambugo/fluctlightdb/scripts/fluctlight-restore.sh ~/.fluctlight/backups/TIMESTAMP
sudo systemctl start fluctlight-serve
```

Verify with `fluctlight status --path ~/.fluctlight/serverbrain.flct`.

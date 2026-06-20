# Serve crash recovery

1. Check WAL segments: `ls -la ~/.fluctlight/serverbrain.flct.wal*`
2. Restart: `sudo systemctl restart fluctlight-serve`
3. On load, corrupt WAL lines are skipped; good ops replay
4. If checksum mismatch on snapshot, restore from latest backup
5. Check logs: `journalctl -u fluctlight-serve -n 100`

WAL replay merges pending ops then checkpoints.

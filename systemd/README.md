# systemd units (production examples)

These files are **templates**. They do not assume a specific Linux user or clone path.

## Install

1. Build the release binary:

   ```bash
   cargo build --release
   sudo install -d /opt/FluctlightDB
   sudo rsync -a --exclude target ./ /opt/FluctlightDB/
   cd /opt/FluctlightDB && cargo build --release
   ```

2. Create data and config directories:

   ```bash
   sudo install -d -o fluctlight -g fluctlight /var/lib/fluctlight/tenants/default
   sudo install -d -m 750 /etc/fluctlight
   sudo cp systemd/environment.example /etc/fluctlight/environment
   sudoedit /etc/fluctlight/environment
   ```

3. Optional secrets (API keys, embed URLs):

   ```bash
   sudo install -m 600 systemd/auth.env.example /etc/fluctlight/auth.env
   sudoedit /etc/fluctlight/auth.env
   ```

4. Install units and enable services:

   ```bash
   sudo cp systemd/fluctlight-*.service systemd/fluctlight-*.timer /etc/systemd/system/
   sudo cp fluctlight-serve.service /etc/systemd/system/ 2>/dev/null || true
   sudo systemctl daemon-reload
   sudo systemctl enable --now fluctlight-serve
   ```

5. Drop-ins (e.g. `fluctlight-serve.service.d/production.conf`) tune rate limits and WAL batching.

## Agent bridge drop-in

`agent-bridge.service.d.example/` shows how to wire an external agent process to Fluctlight serve + native SDK paths. Copy and rename for your agent unit, e.g.:

```bash
sudo mkdir -p /etc/systemd/system/my-agent.service.d
sudo cp -r systemd/agent-bridge.service.d.example/* /etc/systemd/system/my-agent.service.d/
```

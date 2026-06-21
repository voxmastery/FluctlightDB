# Docker

Run FluctlightDB like Qdrant: **pull an image**, set an API key, persist a volume. No Rust or `cargo` required.

## Quick start

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest

docker run -d --name fluctlight \
  -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret-key:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

Python client:

```bash
pip install fluctlightdb
export FLUCTLIGHT_SERVE_URL=http://127.0.0.1:8792
export FLUCTLIGHT_API_KEY=your-secret-key
python -c "from fluctlightdb import FluctlightClient; c=FluctlightClient.from_env(); print(c.health())"
```

## Docker Compose

From a clone of this repo:

```bash
docker compose up -d
```

Edit `FLUCTLIGHT_API_KEYS` in `docker-compose.yml` before production use.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `FLUCTLIGHT_HOME` | `/data` | Data root (`~/.fluctlight` lives here) |
| `FLUCTLIGHT_API_KEYS` | *(required)* | `tenant:key:role` — required for `0.0.0.0` bind |
| `FLUCTLIGHT_TENANT_ID` | `default` | Tenant to auto-create on first start |
| `FLUCTLIGHT_SERVE_ADDR` | `0.0.0.0:8792` | HTTP listen address |
| `FLUCTLIGHT_BRAIN_PATH` | `$FLUCTLIGHT_HOME/.fluctlight/tenants/$TENANT/brain` | Override brain directory |
| `FLUCTLIGHT_STORAGE` | `v4` | Storage layout |

## Build locally

```bash
docker build -t fluctlightdb:local .
docker run --rm -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:dev:write \
  -v fluctlight-data:/data \
  fluctlightdb:local
```

## Images

Published on every git tag `v*` to [GitHub Container Registry](https://github.com/voxmastery/FluctlightDB/pkgs/container/fluctlightdb):

- `ghcr.io/voxmastery/fluctlightdb:latest`
- `ghcr.io/voxmastery/fluctlightdb:0.4.2` (example version tag)

Make the package public once after first push: GitHub repo → Packages → Package settings → Change visibility.

## Release binaries (non-Docker)

Tarballs for Linux (x86_64, arm64) and macOS (arm64) are attached to [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases):

```bash
curl -LO https://github.com/voxmastery/FluctlightDB/releases/download/v0.4.2/fluctlight-0.4.2-linux-x86_64.tar.gz
tar xzf fluctlight-0.4.2-linux-x86_64.tar.gz
./fluctlight serve --addr 127.0.0.1:8792 --path /tmp/brain
```

Use `127.0.0.1` for local dev; set `FLUCTLIGHT_API_KEYS` when binding `0.0.0.0`.

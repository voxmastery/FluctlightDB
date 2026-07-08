# Publishing to PyPI (maintainers)

FluctlightDB ships two Python packages:

| PyPI name | Source | Audience |
|-----------|--------|----------|
| **`fluctlightdb`** | `sdks/python/` | All agent developers (HTTP client, stdlib only) |
| **`fluctlightdb-native`** | `crates/fluctlight-py/` | Optional in-process recall (prebuilt wheels) |

## One-time PyPI setup

1. Create accounts on [pypi.org](https://pypi.org) and [test.pypi.org](https://test.pypi.org) (optional).
2. Register both project names: `fluctlightdb` and `fluctlightdb-native`.
3. Enable **trusted publishing** (recommended) or add a **`PYPI_API_TOKEN`** repository secret:
   - Trusted: PyPI → Your project → Publishing → Add GitHub Actions publisher
   - Token: PyPI → Account → API tokens → scope to `fluctlightdb` + `fluctlightdb-native`
   - GitHub → Settings → Secrets → Actions → `PYPI_API_TOKEN`
4. Push a tag and publish a GitHub Release (see below). No separate `pypi` environment is required.

Alternative: store `PYPI_API_TOKEN` as a repository secret and remove `id-token: write` if not using trusted publishing.

**Note:** GitHub Release workflows cannot trigger other workflows via `release: published` when the release is created by Actions (same `GITHUB_TOKEN`). This repo chains **Publish to PyPI** via `workflow_run` after the **Release** workflow succeeds, or you can run it manually.

## Release process

1. Bump versions (keep in sync):
   - `sdks/python/pyproject.toml` → `version`
   - `crates/fluctlight-py/pyproject.toml` → `version`

   Native wheels use **stable ABI (abi3, cp39 tag)** — one manylinux wheel for Python **3.9–3.13**. Publish **sdist** as fallback (`maturin build --sdist`).

2. Verify locally:

   ```bash
   bash scripts/verify-pypi-wheel.sh
   ```

3. Commit, tag, and push:

   ```bash
   git tag v0.5.2
   git push origin main --tags
   ```

4. Create a **GitHub Release** from the tag. The `Publish to PyPI` workflow runs via `workflow_run` after **Release**, or trigger **Publish to PyPI** manually.

CI job **`pypi-wheel-smoke`** mirrors release: build wheels, install on Python 3.9–3.13, `import fluctlightdb_native` — no source tree.

## Local test build (before release)

```bash
cd sdks/python
python -m pip install build
python -m build
python -m pip install dist/fluctlightdb-*.whl
python -c "from fluctlightdb import FluctlightClient; print(FluctlightClient)"

# Native abi3 wheel (matches CI / PyPI)
cd ../../crates/fluctlight-py
maturin build --release --sdist
bash ../../scripts/verify-pypi-wheel.sh
```

Test upload to TestPyPI:

```bash
python -m pip install twine
twine upload --repository testpypi sdks/python/dist/*
```

## User-facing install (after publish)

```bash
pip install fluctlightdb
pip install "fluctlightdb[native]"   # optional speed
```

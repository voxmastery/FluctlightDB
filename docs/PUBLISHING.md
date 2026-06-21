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

1. Bump versions:
   - `sdks/python/pyproject.toml` → `version`
   - `crates/fluctlight-py/pyproject.toml` → `version` (keep in sync)
2. Commit, tag, and push:

   ```bash
   git tag v0.4.0
   git push origin main --tags
   ```

3. Create a **GitHub Release** from the tag (title `v0.4.0`).  
   The `Publish to PyPI` workflow runs on `release: published`.

Or trigger manually: Actions → **Publish to PyPI** → Run workflow.

## Local test build (before release)

```bash
cd sdks/python
python -m pip install build
python -m build
python -m pip install dist/fluctlightdb-*.whl
python -c "from fluctlightdb import FluctlightClient; print(FluctlightClient)"

# Native (requires Rust + maturin)
cd ../../crates/fluctlight-py
maturin build --release
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

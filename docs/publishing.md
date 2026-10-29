# Publishing ServerBench on GitHub

Repository: **https://github.com/elroy910/ServerBenchmark**

## First publication

Create an empty **public** repository named `ServerBenchmark` under `elroy910`. Public visibility allows ordinary `wget` downloads without authentication. The repository has already been supplied for this project; use it rather than creating a duplicate.

From PowerShell in the project directory, initialize Git if it has not already been initialized:

```powershell
git init -b main
git remote add origin https://github.com/elroy910/ServerBenchmark.git
git add .
git status --short
git commit -m "Prepare ServerBench for GitHub releases"
git push -u origin main
```

If `origin` already exists, inspect `git remote -v`; update it with `git remote set-url origin https://github.com/elroy910/ServerBenchmark.git` only if it points elsewhere. GitHub authentication is handled by Git/Git Credential Manager. GitHub account passwords are not accepted as Git HTTPS passwords; use the normal browser sign-in or an appropriate token when prompted.

`.gitignore` excludes downloaded toolchains, build artifacts, local results, root `vps-*.json` copies and `.env` files. Review staged files before publishing; benchmark results are not project source.

## Publish version 0.1.2

Commit all intended release changes before creating the tag:

```powershell
git tag -a v0.1.2 -m "ServerBench 0.1.2"
git push origin v0.1.2
```

The tag must match `[workspace.package].version` in `Cargo.toml`. Tag pushes trigger the Release workflow, which:

1. Checks the tag against the application version.
2. Runs formatting, Clippy, tests and a real quick benchmark.
3. Builds GNU and static musl binaries for x86_64 and ARM64.
4. Downloads all build artifacts and verifies every asset checksum.
5. Creates a draft release, uploads the complete asset set and publishes it as Latest.

Open [Actions](https://github.com/elroy910/ServerBenchmark/actions) to follow the run. The `wget` URLs work **after the release has finished publishing**, not immediately after the source push. Uploading only a GitHub source archive does not supply compiled executables.

The workflow uses GitHub's automatically provided `GITHUB_TOKEN`, with `contents: write` only on the publishing job. No personal access token secret or server credentials are needed. If repository/organization policy blocks Actions or token write permission, enable the permitted workflow/token settings under repository **Settings → Actions**.

The GitHub connector is not required. You can publish from ordinary Git and let Actions perform all release API calls.

## Download URLs

Latest x86_64 executable:

```text
https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-x86_64
```

Latest ARM64 executable:

```text
https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-arm64
```

Append `.sha256` for the matching checksum file. The release also contains `SHA256SUMS`, four target-specific `.tar.gz` archives and their checksums. Archives include the executable, README, license and supporting documentation.

For repeatable comparisons, pin a release instead of following Latest:

```text
https://github.com/elroy910/ServerBenchmark/releases/download/v0.1.1/serverbench-linux-x86_64
```

These follow GitHub's [documented release asset URL format](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases).

## Later releases

Update the application version in `Cargo.toml`, refresh the workspace package versions in `Cargo.lock` with `cargo check --workspace`, update `CHANGELOG.md`, commit, and push a matching new `vX.Y.Z` tag. Preserve the benchmark version/schema for presentation-only changes; follow [versioning](versioning.md) before altering measured workloads or scores.

Published release assets are never replaced by the script. A failed upload can be retried while the release remains a draft. A completed release requires a new version/tag for different contents. GitHub's [release creation guidance](https://cli.github.com/manual/gh_release_create) describes the draft/upload/publish sequence used here.

## Local release checks

```bash
bash scripts/check-release-version.sh v0.1.2
bash -n scripts/package.sh scripts/prepare-release.sh scripts/publish-release.sh
bash scripts/package.sh x86_64-unknown-linux-musl
bash scripts/package.sh aarch64-unknown-linux-musl
```

`prepare-release.sh` requires all four archives and both static executables. It refuses incomplete or corrupted asset sets. `publish-release.sh` is intended for GitHub Actions, where `gh`, `GH_TOKEN` and `GH_REPO` are available.

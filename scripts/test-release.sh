#!/usr/bin/env bash
# Development-only tests for complete, missing and corrupted release assets.
set -euo pipefail
cd "$(dirname "$0")/.."
workspace="$PWD"
version="$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("Cargo.toml").read_text())["workspace"]["package"]["version"])')"
bash scripts/check-release-version.sh "v$version"
if bash scripts/check-release-version.sh not-a-version >/dev/null 2>&1; then
  echo 'Invalid release tag was accepted.' >&2
  exit 1
fi
scratch="$(mktemp -d)"
trap 'rm -f -- "$scratch"/*; rmdir -- "$scratch"' EXIT
cd "$scratch"
for target in x86_64-unknown-linux-gnu x86_64-unknown-linux-musl aarch64-unknown-linux-gnu aarch64-unknown-linux-musl; do
  printf '%s\n' "$target" > "serverbench-$target.tar.gz"
  sha256sum "serverbench-$target.tar.gz" > "serverbench-$target.tar.gz.sha256"
done
for asset in serverbench-linux-x86_64 serverbench-linux-arm64; do
  printf '%s\n' "$asset" > "$asset"
  sha256sum "$asset" > "$asset.sha256"
done
bash "$workspace/scripts/prepare-release.sh" "$scratch" >/dev/null
test "$(wc -l < SHA256SUMS)" -eq 6
mv serverbench-linux-arm64 serverbench-linux-arm64.saved
if bash "$workspace/scripts/prepare-release.sh" "$scratch" >/dev/null 2>&1; then
  echo 'Incomplete release was accepted.' >&2
  exit 1
fi
mv serverbench-linux-arm64.saved serverbench-linux-arm64
printf 'corrupt\n' >> serverbench-linux-arm64
if bash "$workspace/scripts/prepare-release.sh" "$scratch" >/dev/null 2>&1; then
  echo 'Corrupted release was accepted.' >&2
  exit 1
fi
printf 'Release version, complete set, missing asset and corruption checks passed.\n'

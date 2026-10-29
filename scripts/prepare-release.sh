#!/usr/bin/env bash
set -euo pipefail
directory="${1:?Usage: bash scripts/prepare-release.sh ASSET_DIRECTORY}"
cd "$directory"
assets=(serverbench-linux-x86_64 serverbench-linux-arm64)
for target in x86_64-unknown-linux-gnu x86_64-unknown-linux-musl aarch64-unknown-linux-gnu aarch64-unknown-linux-musl; do
  archive="serverbench-$target.tar.gz"
  test -s "$archive"
  test -s "$archive.sha256"
  sha256sum -c "$archive.sha256"
  assets+=("$archive")
done
for asset in serverbench-linux-x86_64 serverbench-linux-arm64; do
  test -s "$asset"
  test -s "$asset.sha256"
  sha256sum -c "$asset.sha256"
done
sha256sum "${assets[@]}" > SHA256SUMS
sha256sum -c SHA256SUMS

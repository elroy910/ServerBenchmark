#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
target="${1:-x86_64-unknown-linux-musl}"
case "$target" in
  x86_64-unknown-linux-gnu|x86_64-unknown-linux-musl|aarch64-unknown-linux-gnu|aarch64-unknown-linux-musl) ;;
  *) echo "Unsupported release target: $target" >&2; exit 2 ;;
esac
cargo build --release --locked --target "$target" -p serverbench-cli
directory="dist/serverbench-$target"
mkdir -p "$directory"
cp "target/$target/release/serverbench" "$directory/serverbench"
cp README.md LICENSE "$directory/"
cp -R docs "$directory/"
(
  cd "$directory"
  sha256sum serverbench > SHA256SUMS
)

# Short names for direct wget installation; these are the static musl binaries.
case "$target" in
  x86_64-unknown-linux-musl) asset="serverbench-linux-x86_64" ;;
  aarch64-unknown-linux-musl) asset="serverbench-linux-arm64" ;;
  *) asset="" ;;
esac
if [[ -n "$asset" ]]; then
  cp "$directory/serverbench" "dist/$asset"
  (
    cd dist
    sha256sum "$asset" > "$asset.sha256"
  )
fi
tar -czf "dist/serverbench-$target.tar.gz" -C dist "serverbench-$target"
(
  cd dist
  sha256sum "serverbench-$target.tar.gz" > "serverbench-$target.tar.gz.sha256"
)

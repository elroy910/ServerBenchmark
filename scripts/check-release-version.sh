#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
tag="${1:?Usage: bash scripts/check-release-version.sh vX.Y.Z}"
version="$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("Cargo.toml").read_text())["workspace"]["package"]["version"])')"
if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ || "$tag" != "v$version" ]]; then
  printf 'Release tag %s must match Cargo.toml version v%s.\n' "$tag" "$version" >&2
  exit 2
fi
printf 'Release version verified: %s\n' "$tag"

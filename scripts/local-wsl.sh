#!/usr/bin/env bash
# Convenience for a repository-local toolchain, not required by release users.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.tools/linux-cargo"
export RUSTUP_HOME="$PWD/.tools/linux-rustup"
export PATH="$CARGO_HOME/bin:$PATH"
if [[ "${1:-}" == "--tool" ]]; then
  shift
  exec "$@"
fi
exec cargo "$@"

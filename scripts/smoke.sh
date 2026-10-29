#!/usr/bin/env bash
# An actual quick run; taskset constrains only this child process.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked -p serverbench-cli
mkdir -p results/ci
cpu="$(awk '/Cpus_allowed_list:/ {print $2}' /proc/self/status | cut -d, -f1 | cut -d- -f1)"
target/release/serverbench info --json > results/ci/info.json
target/release/serverbench validate --json > results/ci/validation.json
taskset -c "$cpu" target/release/serverbench benchmark --quick --output "results/ci/quick-$(date +%s).json"
result="$(find results/ci -maxdepth 1 -name 'quick-*.json' -print | sort | tail -n1)"
target/release/serverbench compare "$result" "$result"

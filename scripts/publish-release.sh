#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
tag="${1:?Usage: bash scripts/publish-release.sh TAG ASSET_DIRECTORY}"
directory="${2:?Asset directory is required}"
: "${GH_REPO:?GH_REPO must be set to OWNER/REPOSITORY}"
: "${GH_TOKEN:?GH_TOKEN must be supplied by GitHub Actions}"
bash scripts/check-release-version.sh "$tag"
bash scripts/prepare-release.sh "$directory"
notes="$directory/release-notes.md"
cat > "$notes" <<EOF
Standalone Linux CPU benchmarking for x86_64 and ARM64.

Download serverbench-linux-x86_64 for Intel/AMD servers, or serverbench-linux-arm64 for ARM servers. The matching .sha256 file verifies the executable. Archives include the executable, README, license and documentation.

This release adds sequential fresh-process repeat runs, incremental result bundles, failure/cancellation handling, duplicate-aware cohort summaries and optional conditional uncertainty estimates. Use benchmark --repeat 3 to collect a series, then compare the saved series manifests. Copy each manifest together with its sibling .runs directory.

CPU benchmark schema 1 remains experimental. Workloads, reference and scoring are unchanged from the initial CPU contract. See the README and docs/repeated-runs.md for wget installation, repeats, labels, live stats, surveys and comparisons. Detailed live statistics still follow the existing output policy; compact-by-default progress remains planned separately.
EOF
if gh release view "$tag" --repo "$GH_REPO" --json isDraft --jq '.isDraft' > "$directory/release-state.txt" 2>/dev/null; then
  if [[ "$(cat "$directory/release-state.txt")" != "true" ]]; then
    echo "Release $tag is already published; published assets are never overwritten." >&2
    exit 1
  fi
else
  gh release create "$tag" --repo "$GH_REPO" --verify-tag --draft --title "ServerBench $tag" --notes-file "$notes"
fi
assets=("$directory"/serverbench-*.tar.gz "$directory"/serverbench-*.sha256 "$directory/serverbench-linux-x86_64" "$directory/serverbench-linux-arm64" "$directory/SHA256SUMS")
gh release upload "$tag" "${assets[@]}" --repo "$GH_REPO" --clobber
gh release edit "$tag" --repo "$GH_REPO" --draft=false --latest --notes-file "$notes"

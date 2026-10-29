# Contributing

Start with [Project status](STATUS.md). When finishing a feature, merging it or publishing a release, update that page's review date, milestone state and validation evidence. Track **implemented**, **in main** and **released** separately; link the actual commit/release when available. Keep usage changes in the README/guides and release history in the changelog.

Build with the pinned Rust toolchain and commit `Cargo.lock`:

```bash
cargo build --release --locked -p serverbench-cli
bash scripts/check.sh
```

Keep benchmark work outside the CLI. Preserve raw measurements, anonymous defaults and offline operation. Do not change the workload, reference, timing contract or scoring under the existing CPU schema. Read [versioning](docs/versioning.md) before benchmark changes.

Use fixtures for proc/sys/cgroup parsing and known-answer tests for workloads. Describe what changed and which checks you ran. Do not commit machine results, credentials, build directories or downloaded toolchains.

For release maintenance, see [Publishing](docs/publishing.md).

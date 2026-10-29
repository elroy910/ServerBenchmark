# Contributing

Build with the pinned Rust toolchain and commit `Cargo.lock`:

```bash
cargo build --release --locked -p serverbench-cli
bash scripts/check.sh
```

Keep benchmark work outside the CLI. Preserve raw measurements, anonymous defaults and offline operation. Do not change the workload, reference, timing contract or scoring under the existing CPU schema. Read [versioning](docs/versioning.md) before benchmark changes.

Use fixtures for proc/sys/cgroup parsing and known-answer tests for workloads. Describe what changed and which checks you ran. Do not commit machine results, credentials, build directories or downloaded toolchains.

For release maintenance, see [Publishing](docs/publishing.md).

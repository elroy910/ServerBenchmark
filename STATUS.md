# Project status — start here

**Last reviewed: 29 October 2026.** This is the project handover page: what is implemented, what reached `main`, what users can download, and what comes next. It is a dated snapshot, not a live GitHub feed.

## Current position

- **Published release:** [v0.1.2](https://github.com/elroy910/ServerBenchmark/releases/tag/v0.1.2), released 29 October 2026 with all 13 assets; [Latest release](https://github.com/elroy910/ServerBenchmark/releases/latest).
- **Released source:** [`ba58e37`](https://github.com/elroy910/ServerBenchmark/commit/ba58e3753553f57b94e360c204a3f6f4ff800aa8) on `main`.
- **Completed milestones:** PR 0 and PR 1. These are roadmap milestone names; GitHub pull-request numbers may differ.
- **Next numbered milestone:** PR 2 — allocation/topology discovery and CPU selection.
- **Separate pending request:** compact progress by default, detailed live statistics with `--verbose`.
- **Benchmark contract:** CPU v1 remains `cpu-v1-experimental-1`, benchmark version 0.1.0, schema 1. Application releases and benchmark versions are separate.

## Delivery tracker

“Implemented” means working code and relevant checks exist. “In main” means merged/pushed. “Released” means a published downloadable release contains the feature. A plan or a successful local build alone does not establish all three.

| Milestone | Scope | Implemented | In main | First released |
|---|---|---|---|---|
| PR 0 | Document dispatch, preview contracts, fingerprints and v1 regression protection | Yes | Yes | v0.1.2 |
| PR 1 | Fresh-process repeats, result bundles, cohorts and optional conditional intervals | Yes | Yes | v0.1.2 |
| CLI follow-up | Compact progress; `--verbose` selects detailed live stats | No — planned | No | — |
| PR 2 | Allocation/topology discovery, physical-first CPU selection, explicit limit availability | No — planned | No | — |
| PR 3 | Separate v2 engine and authoritative worker/window accounting | No — planned | No | — |
| PR 4 | Scaling study | No — planned | No | — |
| PR 5 | Scoped contention telemetry and evidence | No — planned | No | — |
| PR 6 | Degradation analysis, comparison model and offline HTML reports | No — planned | No | — |
| PR 7 | FP64 and fixed working-set diagnostics | No — planned | No | — |
| PR 8 | Fixed-batch latency diagnostic | No — planned | No | — |
| PR 9 | External calibration, reference and promotion | No — planned | No | — |

Implementation tasks, dependencies and acceptance criteria live in the [ordered backlog](docs/plans/implementation-backlog.md). Keep this table short; retain the detailed design there.

## What works today

- Single CPU v1 benchmarks, live statistics, JSON export, labels, compatible comparisons and scheduled surveys.
- `benchmark --repeat N --repeat-pause D`, with original child results and incremental series manifests. `--repeat 1` preserves the single-result behavior.
- Failure/cancellation handling, duplicate detection, descriptive run statistics and allocation/protocol cohorts.
- `compare ... --uncertainty independent-runs` for explicitly conditional inference with sufficient distinct runs. Three runs do not provide a finite two-sided 95% median interval.
- `--no-progress` already hides repeated live-statistic lines while retaining phase messages and final results. Currently `--verbose` adds hashes; compact-by-default output is still planned.

See [installation and basic use](README.md), the [repeat guide](docs/repeated-runs.md) and [implemented storage contracts](docs/result-contracts.md).

## Verification and limits

For v0.1.2: 72 Rust tests, formatting, Clippy, JSON Schema checks and release-script checks passed. A release-build three-run quick smoke test passed on one WSL CPU, with 10-second sustained phases and 1-second pauses. These were functional checks, not performance calibration.

GitHub [CI](https://github.com/elroy910/ServerBenchmark/actions/runs/40917480619) and the [release workflow](https://github.com/elroy910/ServerBenchmark/actions/runs/40917480929) passed. All four Linux targets built; all executable/archive checksums were verified. The public x86_64 download reported 0.1.2 and passed integrity/workload validation in WSL.

V2 benchmark documents are readable groundwork; there is no executing v2 engine or calibrated v2 score yet. Native ARM64 performance validation and controlled bare-metal/VM/shared-VPS calibration remain outstanding. The CPU baseline remains experimental; follow the [external validation checklist](docs/validation.md).

## Returning after a break

1. Read this page and its review date.
2. Check your checkout and installed executable:

   ```bash
   git status --short
   git log -5 --oneline
   ./serverbench --version
   ```

3. Compare the checkout with GitHub when needed (`git fetch origin --tags`), and check [Latest release](https://github.com/elroy910/ServerBenchmark/releases/latest). Fetching does not merge changes or update the executable.
4. Read the next milestone's section in the [backlog](docs/plans/implementation-backlog.md), then start a feature branch.

Example handover prompt: “Read STATUS.md and implement PR 2 from the backlog on a feature branch. Preserve the v1 contract. Leave publishing to me.”

## Keeping this page current

- At feature completion: update the milestone, behavior, verification evidence and review date. Record local implementation separately from merge/release status.
- At merge: record the verified `main` commit and update “In main”.
- At release: verify the published tag/assets, record the first release containing each completed milestone, and refresh the release/check links.
- Put release history in [CHANGELOG.md](CHANGELOG.md), usage in README/guides, and detailed future work in the backlog. Do not mark calibration complete from CI or fixture tests.

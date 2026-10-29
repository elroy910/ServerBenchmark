# Benchmark improvements: implementation backlog

**Sequenced delivery plan.** Based on app 0.1.1 at `2b78150c50762ed32edda40e693af8749196e539`. PRs 0 and 1 are implemented; see [implemented contracts](../result-contracts.md) and [repeat runs](../repeated-runs.md). PRs 2–9 and the compact-progress follow-up remain planned. Future CLI options below must be introduced in their own PRs.

## Delivery rules

Each PR must include its API/model changes, fixtures, meaningful tests, documentation and backward-compatibility results. Keep all data-generating execution changes under the new preview protocol. Do not retag v0.1.1, replace its assets or reinterpret old scores.

The initial rollout keeps `benchmark` on the unchanged v1 profile. The v2 profile is opt-in and uncalibrated. Defaults for execution order, durations, placement and diagnostic instrumentation are immutable within each distributed preview identity.

No PR is allowed to claim calibration from fixture tests or GitHub CI alone. Maintain a clearly separate external-lab evidence log. Normal users need no root or additional runtime tools.

## PR 0 — Contract dispatch and regression protection

**Implemented:** typed v1/preview benchmark readers, explicit unsupported states for other/future kinds, identity/validity/scope validation, uncalibrated score dispatch, fingerprints, duplicate counting, JSON Schema and synthetic regression snapshots. Timed v1 execution, reference and arithmetic remain unchanged. Full payloads for series/studies/reports arrive with their own PRs.

**Depends on:** nothing. **Goal:** make later formats possible without corrupting v1 meaning.

**Files/modules:** core `documents.rs`, `v2.rs`; report reader/compatibility; score contract dispatch; `tests/fixtures/results/`; version documentation. Preserve existing timed worker implementation.

Tasks:

- Introduce `StoredDocument`, `DocumentHeader`, `ComparisonKey`, `Capability`, `Validity` and structured reasons. Recognize missing type + schema1 as legacy.
- Preserve the old v1 model/scorer/validator, including optional sample fields and exact reference hash. Build explicit v1 and new-format dispatch, not permissive deserialize-and-guess fallback.
- Define schema2 document types, SI-unit naming, preview score-null behavior and unknown-contract behavior. Validate integer counters, finite values, monotonic ranges, result status and declared measurement scope.
- Define immutable `WorkloadIdentity`, `ExecutionProtocol`, `AllocationSignature`, `BuildPolicy` and normalization identity. Record approved generic build/ISA policy without exposing user-specific compiler paths.
- Implement exact-file and canonical-run fingerprints; exclude label/price/path from the latter. Duplicate data must not create repeated statistical evidence.
- Add legacy fixture snapshots with expected scores and compatibility decisions. Synthetic fixtures must not expose a user's server/IP/hostname.

**Done when:** existing v1 fixtures revalidate to identical arithmetic scores; future/unknown contracts produce an explicit unsupported state; preview data cannot enter the v1 scorer; mixed-contract percentages are suppressed; the duplicate-input test passes.

**Validation:** T01–T04 below. No hardware benchmark is required for the reader itself.

## CLI follow-up — Compact progress and verbose details

**Planned; no runtime behavior changed by this plan update.** Can ship independently after PR 0, before or alongside PR 1. This addresses the request for a quieter test that still shows what is happening. Reuse `--verbose`, `--quiet` and `--no-progress`; no additional `--silent` alias is needed.

**Files/modules:** CLI `main.rs`; report `progress.rs`; progress event classification/call sites in core/survey where necessary; CLI/reporter tests; README and release notes.

| Options | Planned running output | Completion |
|---|---|---|
| Default | Phase/workload/worker-mode name and elapsed/target time, approximately every five seconds; no CPU/RAM/temp/load/steal/frequency columns | Existing final report and JSON path |
| `--verbose` | Detailed live resource statistics and existing diagnostic hashes | Final report, JSON path and verbose diagnostics |
| `--no-progress` | Phase transitions only; no periodic live lines | Final report and JSON path |
| `--verbose --no-progress` | Phase transitions only; no live resource lines | Final report/path and verbose diagnostics |
| `--quiet` | No routine phase/progress output | JSON still saved; routine console report/path suppressed |

Tasks:

- Introduce one explicit output policy shared by the CLI and reporter; preserve the existing `--quiet`/`--verbose` conflict. `--no-progress` suppresses periodic updates even with `--verbose`.
- Format compact samples as, for example, `Sustained combined CPU workload [31/300s]`. Keep startup/preflight, phase changes and completion visible. Classify diagnostic messages explicitly instead of parsing human-readable message strings; suppress detailed preflight load/capacity dumps in compact mode. Keep actionable warnings and errors visible with their correct exit behavior, including under `--quiet`.
- Retain the bounded asynchronous nonblocking queue, five-second throttling and first/final sample behavior. Support plain text SSH and redirected logs without requiring a TTY or emitting ANSI control sequences.
- Change presentation only: keep CPU workloads, sampling, durations, telemetry collection, saved JSON, score formulas and benchmark identities identical across verbosity modes. Include raw measurements in JSON even when their console columns are hidden.
- Propagate the output policy to survey and future repeat children. The parent shows attempt/phase context without printing duplicate parent/child progress lines; repeat options remain owned by PR 1.
- Document the default-output change in the application release notes. Bump the application release version when shipping; retain the v1 benchmark version/schema.

**Done when:** captured-output tests cover every option combination above, first/final/throttled updates, a phase change, missing sensors, warnings/errors and redirected output. Feed identical synthetic measurement events through each output policy and verify unchanged saved measurements/scores; quiet must not suppress file creation or failure exit status. No hardware performance run is needed for a formatting-only change.

**Current workaround:** `benchmark --no-progress` already keeps phase messages and results while hiding live statistic lines. Today `--verbose` adds hashes; it does not control whether live statistics appear.

## PR 1 — Repeat series and descriptive/inferential analysis

**Implemented in app 0.1.2 source:** sequential child runner, incremental verifiable bundles, failure/cancellation states, duplicate-aware cohorts, descriptive summaries, explicit conditional intervals, CLI integration and cohort-aware survey summaries. The v1 engine and score arithmetic are unchanged. Native multi-machine calibration remains an external review/lab task; local functional smoke evidence is not calibration.

**Depends on:** PR0. **Goal:** useful repeated measurements before changing the timed benchmark.

**Files/modules:** new runner crate (`single.rs`, `repeat.rs`, child process/cancellation abstraction), new analysis crate (`repeat.rs`, `median_interval.rs`), core series models, CLI flags, report summaries, survey cohort analysis.

Tasks:

- Add proposed `benchmark --repeat N --repeat-pause D [--continue-on-error]`. Accept N=1–100; N=1 retains the original single-result behavior. Require N>=2 for a repeat-specific pause.
- For N>1, `--output series.json` names the manifest; create a sibling `series.runs/` directory with `run-0001.json` etc. Refuse existing destinations before starting. Default outputs use unique timestamped names.
- Spawn the same executable's single-run path with an argument vector and no repeat flag. Preserve every measured profile/mode option, validate the exit status and saved result, and reap children on cancellation. Never run children concurrently.
- Write an initial manifest and atomically update it after each attempt. Save success/failure/invalid status separately. Stop on first failure unless continuation is requested; an incomplete/failed series is never represented as fully successful.
- Preserve parent quiet/no-progress behavior. Child logs may be inherited, but JSON is always a file and never scraped from human console text.
- Implement median/range/SD/CV/MAD across distinct completed runs and per-workload medians. Partition changed observable allocation; preserve chronological survey attempts instead of silently mixing them.
- Implement exact ordered median intervals and conservative ratio-of-medians bounds as specified in design section4. Proposed `compare ... --uncertainty independent-runs` explicitly requests conditional inference; default comparisons remain descriptive.
- Do not add an IID claim for ordinary survey samples or a 95% claim for three repeats. Do not implement resume in this first PR; test interrupted manifests as readable partial artifacts.

**Done when:** a three-run fake-runner integration test saves three distinct child documents; failures/cancellation/duplicates are honest; numeric golden intervals pass; legacy N=1 output remains unchanged. A single real three-run hardware check is part of review, not every CI invocation.

**Validation:** T05–T08. Verify default estimated full-repeat runtime: 3×495s +2×60s =1605s (26m45s), plus initialization.

## PR 2 — Allocation/topology discovery and CPU selection

**Depends on:** PR0. **Goal:** a testable v2 execution plan, without yet changing the legacy runner.

**Files/modules:** system `allocation.rs`, `topology.rs`, `cache.rs`; core topology/allocation models; proc/sys fixtures; CLI profile selection metadata.

Tasks:

- Split observed logical CPU IDs, affinity, effective cpuset, online set, quota rational, effective CPU-equivalent capacity and worker count.
- Preserve limit scope and missing/error/unlimited distinctions. Use rational arithmetic for quota comparisons/ceil. Handle hidden ancestry conservatively.
- Make presentation distinguish a confirmed unlimited visible cgroup limit from unavailable, unreadable and malformed observations. Do not infer unlimited from legacy v1 nulls. Label topology and capacity as guest-visible in VMs, and explain that visible guest cgroup ancestry does not establish host-side VM limits. Keep legacy JSON/scoring unchanged.
- Parse core/sibling groups with reciprocal consistency validation; retain unknown/default topology and virtualization limits. Cache/capacity classes are context, never guessed from clock speed.
- Implement deterministic physical-representatives-first ordering and ascending-ID fallback. Select `min(allowed_count, max(1, ceil(capacity)))` workers for v2 only.
- Define phase-to-phase allocation change detection and the allocation signature used by repeat cohorts. Do not include changing load/frequency/temperature in that signature.
- Preserve the current v1 selection path under its old contract.

**Done when:** all specified fractional/sparse/SMT/missing-topology fixtures pass; a runtime pin probe stays in the allowed set; no host setting is written. There is still no v2 CPU Index.

**Validation:** T09–T11; controlled quota experiment reserved for the lab track.

## PR 3 — Isolated v2 engine and authoritative worker accounting

**Depends on:** PR0, PR2. **Goal:** correct throughput history despite observer delays.

**Files/modules:** CPU `v2/engine.rs`, `v2/counters.rs`, resource planning/workload adapter; core v2 phase/window/worker models; runner profile dispatch.

Tasks:

- Leave v1 timed code behavior intact. Add an opt-in v2 engine with its own protocol manifest and `scores: null`.
- Pin/verify workers, then allocate/touch their private prepared state. Complete preparation before the shared start barrier. Record actual worker start/finish offsets and status.
- Implement per-worker private full-window/tail counters, one reused post-batch clock read and padded low-frequency live counters. Resource sampling must not be the source of authoritative throughput buckets.
- Add actual elapsed totals, exact counts and phase-boundary worker CPU time. Persist authoritative throughput windows separately from irregular resource intervals.
- Bound phase duration at24h in the preview and account for bucket/buffer memory. Use fallible preparation and reject insufficient resources before timed work; never silently change the worker count or dataset size.
- Preserve completed data on cancellation/checksum failure. Any failed worker prevents that phase from becoming a complete valid scored result. Check allocation before/after each phase; changes create an explicit invalidity reason.
- Keep phase/warm-up order and durations explicit. Ordinary measurement is not batch-latency instrumentation. Record timer/collection coverage and observer CPU cost.

**Done when:** fake-clock accounting is exact; delayed/blocked observer output cannot lose worker buckets; full+tail units equal completed work; memory/cancellation tests pass. Real quiet native x86_64 and ARM64 smoke evidence is collected before wider preview use.

**Validation:** T12–T15. Compare v1/v2 only as separately identified raw protocols, not percent CPU Index changes.

## PR 4 — Scaling study

**Depends on:** PR1 analysis infrastructure, PR3. **Goal:** show delivered scaling without altering the index.

**Files/modules:** runner `scaling.rs`; analysis `scaling.rs`; core study models; CLI `scaling`; console report.

Tasks:

- Generate the exact unique worker-count set, physical-core boundary and three counterbalanced non-one-worker orders.
- Execute bracketing one-worker controls, fixed warm-up/measurement/recovery and idle baseline. Calculate the displayed runtime from the generated phase list.
- Implement speedup, per-worker and budget-relative efficiency, observed peak and baseline-drift reasons. Keep efficiencies above100% and slower high-worker points visible.
- Persist every trial/control and partial study. A one-worker-only study produces no parallel-scaling claim.

**Done when:** all point/order fixtures and C=0.5/1.5 normalization examples pass; changed allocations do not become a completed stable curve; runtime estimates match the generated schedule within setup/batch-tail overhead.

**Validation:** T16–T17; one homogeneous-core and one quota-limited external test.

## PR 5 — Scoped contention telemetry and evidence

**Depends on:** PR3. **Goal:** collect enough scope-correct data to explain observations cautiously.

**Files/modules:** metrics `pressure.rs`, `cgroup.rs`, `cpu_time.rs`; analysis `evidence.rs`; core capabilities/scopes; fixtures and live/report formatting.

Tasks:

- Collect feature-detected pressure, enforcement/usage/burst and optional local-throttle counters separately for visible scopes. Retain raw units/counters and derive interval deltas.
- Collect process CPU-time intervals and per-worker phase CPU time, with clear accounting boundaries. Keep CPU-set usage distinct from own execution.
- Make read failures, unsupported fields, resets and unmeaningful system CPU-full values explicit unknowns. Do not sum ancestors or clamp runqueue-throttle duration into a false percent.
- Implement rule-versioned observations with evidence IDs, scope and no unsupported causal claim. Normal quota enforcement is not classified as a noisy neighbour.
- Measure collection duration, observer CPU and dropped display updates; keep output off workers/sampler. Do not record cgroup paths/task IDs in exported metadata.

**Done when:** capability/units/scope tests pass, no-pressure-data does not mean no contention, and overhead evidence satisfies the preview gate or the expensive observation becomes a distinct opt-in protocol.

**Validation:** T18–T20; delegated cgroup and no-sensor/no-PSI lab examples.

## PR 6 — Offline degradation analysis, comparison model and HTML

**Depends on:** PR0, PR1; evidence overlays depend on PR5. **Goal:** make existing and new data understandable offline.

**Files/modules:** analysis `comparison.rs`, `degradation.rs`, `report_model.rs`; report `html.rs` and embedded assets; CLI `report`; fixtures/screenshots.

Tasks:

- Produce one report model for console/HTML: names, source fingerprints, provenance, compatibility, allocations, raw metrics, uncertainty, limitations and duplicate warnings.
- Add the specified offline early/late and rolling decline analyses. Never replace stored v1 scores/Consistency with these new interpretations.
- Align plots by elapsed phase time and real interval overlap; leave missing resource observations as gaps. Display standard/scaling/latency protocols distinctly.
- Generate a single offline HTML file with embedded assets, safe user-text/JSON handling, restrictive content policy and no network requests.
- Preserve extrema/gaps during display downsampling and provide accessible tables. Raw JSON is unchanged and export is explicit.
- Keep report execution read-only with respect to benchmark inputs; no implicit new benchmark, upload or browser launch.

**Done when:** original 0.1.0/0.1.1 files render correctly, incompatible results have no misleading percentages, duplicate copies are flagged, and offline screenshots/network tests pass.

**Validation:** T21–T24. Deliver reader/console improvements before waiting for every advanced diagnostic plot.

## PR 7 — FP64 and fixed working-set diagnostics

**Depends on:** PR3; reporting uses PR6. **Goal:** broaden measured behavior with correct, separately named tests.

**Files/modules:** CPU `v2/workloads/fp64.rs`, `corpus.rs`, variants/resource declarations; independent vector generator; fixtures; validation metadata.

Tasks:

- Implement the exact dyadic 64x64 FP candidate, pinned operation/buffer/verification semantics and independently reproduced input/output hashes from design section8.
- Add deterministic64KiB/1MiB/16MiB compression/decompression/software-SHA variants with stable identities and full correctness checks. Preserve the original64KiB v1 generator unchanged.
- Implement complete `ResourcePlan` bounds and bounded/fallible buffer preparation. Retain required/optional skipped reasons; never reward missing variants by changing an aggregate denominator.
- Record working-set bytes and algorithm/build policy; do not claim cache-level isolation or peak FLOPS. Inspect release code generation and verify black-box/correctness consumption.
- Run native x86_64 and ARM64 golden vectors; pin any new dependency with its MSRV/target validation. No accelerated-crypto/BLAS substitution under existing names.

**Done when:** candidate correctness and resource limits pass and the raw diagnostics are usable. All normalized v2 score fields remain null until PR9; pilot findings may require a new preview identity before any public scoring.

**Validation:** T25–T26. Native hardware execution is a review requirement for the vector implementation.

## PR 8 — Fixed-batch latency diagnostic

**Depends on:** PR3; plots use PR6. **Goal:** expose completion jitter with bounded measurement overhead.

**Files/modules:** CPU `v2/latency.rs`; core histogram/latency models; analysis histogram merge/quantile bounds; CLI `latency`.

Tasks:

- Pin a Rust1.85.1-compatible HdrHistogram dependency in the lockfile, using fixed range/precision and no auto-resize in timed recording. Disable unnecessary codec/synchronization features when possible.
- Use the fixed integer batch; bracket every batch only in this separate protocol. Initialize histograms before timing; keep per-worker ownership.
- Serialize explicit count/bound bins and overflow/underflow/max metadata. Merge distributions, not percentiles. Enforce the chosen minimum display counts and preserve bounds for overflowing quantiles.
- Record empty clock-probe costs and resolution without subtracting them from observations. Set instrumentation identity so the result cannot enter a normal CPU Index.
- Describe the measured closed-loop completion time and its limits; no web/API latency prediction or unspecified coordinated-omission correction.

**Done when:** fixed-range/overflow/merge/quantile tests pass, worker hot loops do not allocate, and instrumentation overhead is measured on both architectures.

**Validation:** T27–T28. Do not lengthen a run silently until enough samples exist; report insufficient tail sample counts.

## PR 9 — Calibration report, reference and promotion

**Depends on:** completed measured features selected for v2 and their evidence; data collection starts after PR0.

**Files/artifacts:** external lab manifests/raw results, reproducible analysis, versioned calibration report, approved reference/score-model files, score vectors, documentation and release notes.

Tasks:

- Execute the cohort/intervention matrix in design section11 with fixed release binaries and retained failed/noisy observations.
- Establish empirical repeatability, quota accounting, observer cost and component behavior, including native ARM64 and actual shared VPS data.
- Evaluate category weighting, correlation/rank sensitivity and the meaning of Reliable/Consistency. Decide a reference and score model only from this report; do not tune them to a preferred machine's result.
- Publish exact workloads, variants, durations, allocations, units, weights, reference values, known limits and golden score vectors. If design constants changed, use a new preview contract and repeat the affected experiments.
- Add contract-specific v2 score calculation/validation, schema reader and comparison groups. Keep all historical v1 arithmetic/readers.
- Promote a new default only in an explicit application release with migration notes. Do not present the existing experimental schema1 or a partial preview as an already calibrated public standard.

**Done when:** the public promotion gate is met with reviewable data, not just code/CI. If lab resources are unavailable, ship useful raw preview diagnostics while leaving promotion blocked on those experiments.

**Validation:** T29–T30, full external evidence review and the immutable-reference test suite.

## Acceptance matrix

T01–T04 are covered by PR 0's regression suite. PR 1 adds automated T05–T08 lifecycle/statistics/cohort coverage and a local release smoke check. T09–T30 remain future tests/experiments; local functional checks are not calibration evidence.

| ID | Test or experiment | Expected evidence |
|---|---|---|
| T01 | v1 snapshots | Identical recomputed historical scores and reference hash |
| T02 | Document dispatch | No wrong-contract scorer fallback; explicit unknown format |
| T03 | Canonical duplicate handling | Renamed/relabelled same run is not a second repetition |
| T04 | Build/protocol compatibility | No percentages across incompatible instrumentation/build policies |
| T05 | Repeat child lifecycle | Sequential fresh children, no recursion/orphans, exact options |
| T06 | Partial repeat failures | Immediate persisted manifests and correct exit/status/counts |
| T07 | Exact interval math | Golden small-n/coverage/tie/zero cases and stable n=100 calculation |
| T08 | Changed cohort | No pooling across changed observable allocations or mixed modes |
| T09 | Topology masks | Sparse/large IDs, restricted siblings, unknown/sentinel/inconsistent masks |
| T10 | Quota rational math | Exact ceil near integers; parent/cpuset bounds and hidden visibility |
| T11 | Runtime placement | Verified allowed affinity and unchanged caller configuration |
| T12 | Worker bucket accounting | Full windows + tail = all completed work, including zero-work windows |
| T13 | Delayed observer | Observer/output delays do not lose or fabricate authoritative throughput |
| T14 | Invalid worker/cancellation | Partial data retained and no complete valid phase/score |
| T15 | Resource budget | Bounded/fallible preparation; no implicit dataset/worker reduction |
| T16 | Scaling schedules | Exact point lists/orders/controls, runtime and baseline drift flags |
| T17 | Scaling formula | C=0.5 and1.5 cases, zero baseline and >100% values handled honestly |
| T18 | PSI/cgroup fixtures | Correct scope, source units, missing fields, resets and local counters |
| T19 | Attribution rules | No noisy-neighbour claim from benchmark pressure or expected quota enforcement |
| T20 | Observer overhead | Measured CPU/throughput perturbation, bounded memory and preserved raw data |
| T21 | Historical report | Old JSON, long/missing labels, duplicates and mixed contracts display correctly |
| T22 | Offline HTML | No external requests; readable from file URL with safe metadata rendering |
| T23 | Chart transforms | Gaps/extrema/units preserved; original JSON unchanged |
| T24 | Degradation fixtures | Isolated stall, decline, recovery, missing sensor and changed allocation cases |
| T25 | FP golden answers | Exact integer-derived output and matching native architecture digests |
| T26 | Working-set variants | Exact identities, units, roundtrips and visible resource skips |
| T27 | Histogram limits | No auto-resize/clamping/data loss; per-worker bins reconstruct aggregate bounds |
| T28 | Latency overhead/meaning | Instrumented mode isolated; insufficient tails/clock costs exposed |
| T29 | Controlled intervention | Correct quota/time response and matched host/VM evidence |
| T30 | Public score freeze | Multi-system repeats/surveys, reviewed weights/reference and immutable vectors |

## Required external resources and release boundaries

The first reader/repeat/report work can begin with existing code and synthetic fixtures. Meaningful public calibration needs controlled Intel, AMD and native ARM64 systems, a matched host/VM setup, permission to operate isolated lab competitors/delegated quotas, and an actual shared VPS over several days. No credentials or provider access are assumed in this plan.

Early application releases may deliver repeats, legacy report improvements and explicitly unscored diagnostics. The current v1 contract remains the historical default until explicit promotion. A lack of calibration resources should not be hidden by choosing arbitrary new reference numbers.

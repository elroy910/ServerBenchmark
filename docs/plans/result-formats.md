# Proposed result and service contracts

This is the full design appendix for [benchmark-improvements.md](benchmark-improvements.md), not a measured result. PR 0 implements the read-only benchmark subset; see [implemented contracts](../result-contracts.md) and its JSON Schema. Series, study, richer telemetry and report payloads below remain future work.

## Format envelope

New documents use the following common contract. The existing schema1 result remains accepted without any migration or rewrite.

| Field | Type / invariant |
|---|---|
| `schema_version` | Integer2 for new-format documents |
| `document_type` | `benchmark`, `repeat_series`, `scaling_study`, `latency_study`, `comparison_report` |
| `application_version` | Producing application version; not a score identity |
| `benchmark_schema_version` | Underlying execution contract family; series uses the child contract |
| `benchmark_contract_id` | Exact distributed workload/execution contract |
| `protocol_id` | Mode, timing/order, placement, telemetry and instrumentation variant |
| `score_model_id` | Nullable; absent calibration is not a zero score |
| `score_status` | `uncalibrated`, `available`, `not_applicable`, or `invalid` |
| `status` | `valid`, `incomplete` or `invalid` for the artifact |
| `reason_codes` | Stable machine-readable diagnostics; explanatory text is separate |
| `timestamp` | UTC creation/start metadata; never the timing source |
| `build` | Compiler/target/approved flags/dependency and binary/source manifest fingerprints; no local paths |
| `allocation` | Visible limit/topology/placement snapshot and completeness; no physical-host identity claim |
| `metadata` | Explicit label/provider/price and anonymous flag, outside measured identity |
| `capabilities` | Source/scoped availability with structured reasons |
| `payload` | Type-specific data below |
| `scores` | Contract-specific object or null; never filled by a foreign legacy scorer |

All new raw `u64` counters, durations in ns and histogram edges/counts are canonical decimal **strings** in JSON, validated back to bounded integers in Rust. Versions, bounded CPU IDs and worker counts can remain JSON numbers. This avoids JavaScript precision loss for long-lived/multicore cumulative counters. V1 numeric fields retain their original representation; conversion is only in the report view model and never overwrites source JSON.

Throughput is finite f64 with a declared unit. Fractions and percentages use different field suffixes. Optional observations are null with a reason; a measured zero stays zero. Reject negative durations, nonfinite values, inconsistent totals and fabricated complete status.

## Shared value objects

### AllocationSignature

- Observed allowed/online/cpuset IDs and opaque topology grouping, known capacity classes or explicit unknown.
- Exact positive quota/period rational or distinct unlimited/unknown state; ordered visible ancestor records with ordinal scope IDs.
- Effective CPU-equivalent ceiling, planned workers, selected logical CPU IDs and placement/allocation ownership policy.
- Completeness and changed-allocation evidence. Kernel/model metadata is useful context, not a unique physical-machine identifier.
- Exclude transient load, temperature and frequency from the repeat cohort key.

### WorkloadIdentity / ExecutionProtocol

Algorithm ID/version, dataset generator/parameters/hash, input/working-set bytes, work units per batch, correctness vector ID, verification placement, ISA/backend policy, buffer ownership, warm-up/measurement/order rules, timer/bucket policy, placement and instrumentation identity. A human display name is not a sufficient identity.

### Capability and MetricObservation

Capability states distinguish available, unsupported, not visible, permission denied, malformed and not applicable. Counter resets/incomplete intervals are sample validity reasons, not zero values. Every metric carries its scope and source identity; source paths are represented logically (`system_visible`, `leaf`, `ancestor_1`), not exported absolute filesystem paths.

### EvidenceObservation

Rule ID/version, measured observation code, affected phase/window IDs, source/scopes, thresholds and limitations. This is evidence for an observation, not an automatically proven cause. No effect on CPU scores.

## Benchmark payload

`phases[]` contains:

- Workload/protocol identities, role (`single`, `multi`, `sustained`, `warmup`) and status.
- Nominal duration, actual elapsed duration, actual worker start/finish offsets and tail duration.
- Checked completed batch/unit totals, actual-elapsed throughput and per-worker/CPU-equivalent rates.
- Per-worker ordinal ID, selected/verified CPU IDs, completion counts, CPU time and status; no serialized Linux TID.
- Authoritative fixed-grid `throughput_windows[]`: start/end ns, completed units per worker and aggregate. Explicit zeros; full windows and tail distinguished.
- Independent `resource_samples[]` with actual interval bounds, scoped original counters, normalized deltas and optional CPU/memory/temperature data. Telemetry gaps do not erase throughput windows.
- Summary statistics with their sample/window selection and analysis version. Invalid required phases prevent a complete scored benchmark.

Invariant: aggregate full-window units + aggregate tail units = summed worker completed units = workload units per batch × summed completed batches. Counts/elapsed must reconstruct published total throughput within declared floating tolerance.

## Repeat series payload

Series options and cohort signature; `attempts[]` with attempt ordinal, child relative path/hash, content fingerprint, requested options, start/end, exit/status and failure reason; fixed pause and completion status; distinct successful run count and run-level summaries.

The manifest is created before the first child and updated after every attempt. Relative paths must resolve inside the series bundle; offline imports cannot silently traverse to unrelated files. Original child JSON stays intact. A malformed/missing child is not a successful run.

Uncertainty is an optional analysis object: estimator (`median`), observation unit (`distinct completed run`), n, explicit sampling assumptions, method/version, requested coverage, binomial coverage bound, endpoints/reason and comparison ratio interval. The manifest never treats within-run windows as extra runs.

## Scaling study payload

Fixed point list and generated trial schedule; per-trial pre/post one-worker controls; point worker count/placement and phase results; control drift; speedup, worker and budget-relative efficiencies; observed peak; changed allocation/partial study status. The control records are not independent repeated study runs.

Record the normalization baseline explicitly. Preserve a raw curve even when drift or insufficient capacity prevents a confident interpretation. No CPU Index field is produced by this study.

## Latency study payload

Fixed work/batch identity, instrumentation mode, per-worker bounded histograms, bin low/high/count triplets, precision/range, total count, zero/underflow/overflow count, maximum actual duration, clock resolution/probe distribution, and phase CPU time.

The aggregate is a sum of counts with equal histogram settings. Published percentile values are quantized bounds and include any overflow limitation. Counts below the quantile display policy return null/reason. No score is produced from instrumented throughput.

## ComparisonReport service model

Source provenance and names; duplicate information; compatibility decision per comparison family; displayed scores/raw components and their units; optional run-level uncertainty; typed observations; aligned chart traces with coverage/gaps; explicit explanatory limitations. Console and HTML consume this same validated object.

Its raw counters remain exact strings. Browser chart coordinates may use finite numbers after a checked unit conversion; labels use text-safe rendering. Display downsampling operates on a copy only.

## Service boundaries

The interfaces below describe responsibilities; they are not source code to paste without implementation/testing.

| Service | Inputs → outputs | Side effects |
|---|---|---|
| `DocumentReader` | Path/bytes → validated typed document or explicit unsupported/invalid result | Reads only |
| `AllocationPlanner` | proc/sys snapshot + profile → immutable allocation/resource plan | Reads only |
| `RunExecutor` | Validated profile/phase plan + cancellation/output observer → raw result | CPU work, scoped threads, local result writes |
| `RepeatRunner` | Child invocation spec + repeat plan → incremental series | Sequential child processes and bundle writes |
| `StudyRunner` | Generated scaling/latency phase plan → incremental study | Explicit measured work and local writes |
| `Analysis` | Validated raw documents + versioned rules → summaries/evidence/comparison | Pure; no benchmark invocation |
| `ReportRenderer` | Typed report + output policy → console/HTML | Output writes only; no network/browser launch |
| `ScoreRegistry` | Matching complete valid benchmark + known score model → scores | Pure; rejects uncalibrated/foreign contracts |

All cancellation paths release threads/children, retain completed data and prevent incomplete measurements from acquiring complete-score status. Runtime dependencies remain native Rust/Linux only.

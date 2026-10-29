# Result contracts and document reader

PR 0 adds read-only benchmark contracts and regression protection. PR 1 adds [repeat series](repeated-runs.md) and conditional analysis in application 0.1.2. A single `benchmark` still executes CPU v1 and writes its original schema 1 JSON. V2 execution/calibration, scaling/latency studies and HTML export remain planned.

The [design appendix](plans/result-formats.md) describes later extensions. This document describes the implemented subset. The Rust models and reader are authoritative for semantic validation; the [preview JSON Schema](../schemas/benchmark-preview-v2.schema.json) describes its structural representation.

## Reading and dispatch

`serverbench_report::read_document(bytes)` and `load_document(path)` return one of:

- `ReadOutcome::Supported`: a validated `StoredDocument`, `ComparisonKey`, exact file hash and canonical run hash.
- `ReadOutcome::Series`: a typed repeat manifest, child verification state and verified runs/cohorts when loaded from a bundle path. Byte-only imports remain explicitly unresolved.
- `ReadOutcome::Unsupported`: the parsed `DocumentHeader` and a structured reason. Measurement and score fields from this document are never trusted or displayed.
- `Err` with code 4: malformed JSON or invalid data in a supported contract. File I/O failures retain code 1.

Core `decode_document` handles structural dispatch; the report reader additionally invokes the score registry. Call the report reader when importing files. `load_result` remains a legacy-only convenience and rejects unsupported/preview documents. Source bytes are never rewritten, and reading/comparing files starts no benchmark.

| Input | Outcome |
|---|---|
| Schema 1, no `document_type`, known v1 contract/version | Original `BenchmarkResult`, original validator and scorer |
| Schema 1, explicit `document_type: "benchmark"` | Same historical reader |
| Schema 2, `benchmark`, known preview contract/protocol/build policy | Typed, validated preview; scores must be null |
| Schema 2, known `repeat_series` contract/protocol | Typed repeat manifest; path imports verify child provenance and recompute summaries |
| Schema 2, `scaling_study`, `latency_study`, `comparison_report` | Recognized kind, `not_implemented`; payload support arrives in its planned PR |
| Unknown schema, kind, contract, protocol, score model or build policy | Explicit `unsupported_*` reason; no fallback |
| Missing schema, null discriminator, duplicate JSON keys, invalid known payload | Validation error |

The supported preview identifiers are `cpu-v2-preview-1`, benchmark schema 2 and `cpu-v2-preview/core-v1`. A schema 2 benchmark must explicitly contain `score_model_id: null`, `score_status: "uncalibrated"` and `scores: null`, including a fully completed preview. An unknown score model is unsupported; a non-null score in the known uncalibrated protocol is invalid.

Preview structs reject unknown fields. Legacy structs retain their historical field/default behavior, including missing memory and temperature observations. Future fields require an intentional reader/contract update.

## Values, identities and validity

New `u64` counters and nanosecond durations use canonical decimal strings: `"0"` through `"18446744073709551615"`. JSON numbers, signs, leading zeros, exponent notation and overflow are rejected. Bounded versions, worker ordinals and CPU IDs use JSON integers. V1 keeps all original numeric representations.

Time fields ending `_ns` use nanoseconds; allocation quota rationals ending `_us` use microseconds. Byte counts use bytes. `total_throughput` is a finite, nonnegative number in the workload's declared units per second; percentages are finite values in [0,100]. No missing observation is converted to zero.

- `WorkloadIdentity` binds algorithm/version, corpus hash where applicable, input/working-set byte counts, units per batch, unit, verification policy and ISA/backend. Identities for a workload must remain equal across phases. This first preview uses the existing five workloads and mixed cycle; FP64 and working-set variants arrive later.
- `ExecutionProtocol` binds mode, explicit nanosecond durations, timer/window policy, placement, throughput instrumentation and telemetry policy. Its fixed phase order is mixed warm-up, five single phases in `WORKLOADS` order, five multi phases in that order, then mixed sustained. Nominal durations follow the selected mode; sustained may be a whole number of seconds from 10 seconds through 24 hours.
- `AllocationSignature` records sorted allowed CPU IDs, ordered unique selected CPU IDs, worker count, visible quota state/rational, visible CPU-equivalent capacity, visibility, placement, topology confidence and buffer ownership. Limited quota uses exact integer ceiling, capped by allowed CPUs, for the worker count. Unknown limits stay explicitly unknown; visible capacity is not proof of physical host capacity. Discovery/topology implementations arrive in PR 2.
- `BuildPolicy` approves `rust-1.85.1-generic-v1`: exact Rust 1.85.1 compiler identity, release optimization 3, thin LTO, one codegen unit, unwind, generic CPU and no extra target features. Four Linux GNU/musl x86_64/ARM64 targets are supported. Lockfile, optional executable SHA-256 and source commit are declared provenance, not authenticated attestations. No compiler executable path is represented. A future optimized ISA policy needs its own approved identity.
- `NormalizationIdentity` binds the v1 contract, pinned reference hash and index 1000. Preview normalization is absent. A preview never inherits v1 normalization.

`Validity` distinguishes valid, incomplete, invalid and unavailable observations. A benchmark artifact uses the first three; unavailable is for observations/capabilities. Incomplete/invalid artifacts and phases require reason codes. Valid artifacts require every planned phase, ordered correctly, with valid workers and checksums. Partial artifacts may retain the measured phase prefix; they cannot become successful scored runs.

For every phase the reader checks positive bounded nominal duration, worker ordinals/placement, ordered start/finish offsets within elapsed time, checked batch-to-unit arithmetic, checked worker sums, finite reconstructed throughput, contiguous one-second completion windows, coverage and exact full-window plus tail totals. Window/tail units must represent whole completed batches; nonzero window work must overlap that worker's lifetime. Actual elapsed time includes completion tails. A valid phase needs full nominal window coverage, elapsed time at least nominal duration and every worker finishing at or after the nominal end. Short phases, overflow and failed required workers cannot masquerade as completed measurements.

Capabilities carry a logical source ID, declared scope and either an available value or an unavailability reason (`not_supported`, `not_visible`, `permission_denied`, `malformed`, `not_applicable`). Logical scopes are `system_visible`, `allowed_cpu_set`, `process`, `worker` with ordinal, and `cgroup` with visible ancestor ordinal (0 is the leaf). No absolute path or operating-system thread ID is part of a scope. Worker scopes must reference an existing worker. Resource intervals are ordered and non-overlapping within a scope; different scopes may cover the same interval. Resource windows are independent of authoritative throughput windows. These types do not yet collect new telemetry.

## Comparability and scoring

Legacy comparison keys preserve the existing decisions: result/benchmark schema, benchmark major, mode, durations, contract, reference/corpus hashes, compiler and optimized-build flag must agree. Different server hardware, worker counts and app patch versions remain comparable under those rules. V1 has limited historical build metadata; the reader does not invent a new ISA/build attestation for old results.

Preview keys include the exact contract, full execution protocol, workload manifests and approved build identity. Architecture can differ within a generic build policy; GNU and musl, compiler identities and dependency-lock fingerprints stay separate. Binary/source fingerprints remain provenance rather than requiring identical executables across architectures. Observable allocation is a separate signature for later run cohorts, not a requirement for comparing different servers. A compatible raw preview key still does not authorize normalized scores or percentage CPU Index comparisons.

The score registry accepts a `StoredDocument` variant. Only `LegacyBenchmarkV1` invokes the unchanged v1 validator/calculation; `PreviewBenchmarkV2` returns `Uncalibrated` and `RepeatSeries` returns `NotApplicable` after validation. Series do not have a single-run comparison key/content fingerprint and never enter the scorer as synthetic benchmarks. File comparison validates each supported input before deciding compatibility, including when inputs have different build settings. Known malformed scores are rejected rather than displayed.

Console comparison preserves v1 numeric output. Mixed, preview and unsupported inputs show identity/status and suppress score differences. Unknown score values are not rendered. Recognizing an unsupported file is a successful descriptive comparison; invalid known data still exits with validation failure.

## Fingerprints and duplicate evidence

`exact_file_sha256` hashes the original bytes, including whitespace. `canonical_run_sha256` uses the versioned algorithm below:

1. Deserialize and validate into the supported typed model. Serialize that model to a JSON value, normalizing legacy missing optional fields to null.
2. Remove root `metadata`, `application_version`, `warnings`, `scores` and `price_performance`. Thus labels, providers, prices, currency and anonymity flags cannot create another run.
3. Recursively order JSON object keys and serialize compact JSON using the pinned serde_json representation. Array order remains meaningful. Do not enable `serde_json/preserve_order` without revising/testing this algorithm.
4. SHA-256 the bytes `serverbench:canonical-run:v1\0` (the final byte is NUL), followed by that canonical JSON.

The source filesystem path is never part of either typed run identity. Timestamp, measured raw data, system/allocation observations, benchmark/protocol identities and build provenance remain in the canonical run. This is an application-specific canonicalization, not RFC 8785. Deliberately edited measurements/timestamps can evade duplicate detection; a content hash proves no physical independence or authenticity.

`duplicate_kind` identifies exact files or canonical copies. `distinct_run_count` counts unique valid runs only; preview incomplete/invalid artifacts contribute none. It does not pool different contracts or assert IID sampling. Future series/analysis code must partition cohorts and use these hashes rather than counting files. Console comparison flags duplicates; showing identical descriptive scores remains useful.

## Regression evidence

The [synthetic fixture notes](../tests/fixtures/results/README.md) explain v1 expected scores and comparison decisions. `bash scripts/check.sh` runs the document regression tests with the existing workload/reference tests. CI additionally validates the Draft 2020-12 schema, preview fixture, invalid score/type states and decimal-string bounds against integer arithmetic. To run that developer-only check locally:

```bash
python3 -m venv .tools/schema-check
.tools/schema-check/bin/python -m pip install -r scripts/schema-check-requirements.txt
.tools/schema-check/bin/python scripts/check-contract-schema.py
```

This optional development tool does not add a server/runtime dependency. No hardware benchmark is required to verify the reader. PR 0 retained app version 0.1.1; PR 1 source advances it to 0.1.2. Publishing remains a separate explicit action and existing tags/assets stay immutable.

# Changelog

## Unreleased

- Add explicit legacy/preview document dispatch, validity, scope and build/workload/protocol identities.
- Validate imported results before file comparison; suppress foreign/uncalibrated score differences.
- Detect copied/relabelled runs with file/content fingerprints and preserve v1 golden scores.
- Document the read-only schema 2 preview contract and sequenced improvement plan. V1 execution and scoring remain unchanged.

## 0.1.1

- Show server labels and source filenames in reports and comparisons.
- Display live CPU, RAM, load, steal, frequency and available CPU temperatures.
- Preserve optional memory and temperature observations in JSON.
- Keep slow terminal output off the sampler with a bounded progress queue.
- Preserve compatibility with CPU benchmark schema 1 results from app 0.1.0.
- Add GitHub release automation, standalone wget assets and checksums.

## 0.1.0

- Initial experimental Linux CPU benchmark: five deterministic workloads, single/multi-core and sustained measurements.
- System/cgroup detection, raw runtime metrics, scoring, JSON, compare and survey.

The CPU baseline is experimental and has not been frozen as a public benchmark standard.

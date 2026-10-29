# Repeat runs and conditional uncertainty

Application 0.1.2 adds PR 1. Each repetition launches the same executable as a fresh process through the unchanged single-run v1 path. Workloads, idle/warm-up phases, scoring, reference and benchmark version remain unchanged. Source support does not publish a GitHub release automatically.

## Run a series

```bash
./serverbench benchmark --full --repeat 3 --repeat-pause 60s \
  --no-progress --label "Server A" --output results/server-a-series.json
```

Counts are 1–100. Omitting `--repeat`, or specifying `--repeat 1`, keeps the original single-result format and execution path. `--repeat-pause` and `--continue-on-error` require at least two runs. A pause is a fixed positive duration, default 60 seconds; it does not adapt to temperature or observed performance.

Three full runs require approximately `3 × 495 + 2 × 60 = 1605` seconds (26 minutes 45 seconds), plus initialization and completion tails. The CLI prints the estimate using the actual requested mode/durations. Quick runs remain suitable for a functional check; use the same full profile when comparing servers seriously.

For N>1, `--output` names a manifest rather than a benchmark result:

```text
results/server-a-series.json
results/server-a-series.runs/run-0001.json
results/server-a-series.runs/run-0002.json
results/server-a-series.runs/run-0003.json
```

Existing manifests or child directories are refused before starting work. Default output names are unique. Each child receives the requested mode, sustained duration, strict setting, user metadata and quiet/verbose/no-progress options, without repeat flags. Children run sequentially, never concurrently. Child logs are inherited; parent code reads saved JSON, not console text.

`--no-progress` keeps phase messages and final reports without live statistics. `--quiet` suppresses routine output while preserving JSON. The separate planned compact-progress change has not been implemented by PR 1.

## Failure and interruption

An initial manifest is saved before launching the first child. Before every launch the attempt is marked running; after every outcome the manifest is atomically replaced with updated status, provenance and summaries. Child result files are never overwritten.

The default stops at the first failed, invalid or duplicate attempt and exits 1. With `--continue-on-error`, remaining attempts run, but any failure/invalidity/duplicate still produces an unsuccessful series and exit 1. A zero child exit status without a valid, matching saved result is invalid. Spawn failures, nonzero exits, option mismatches and missing/corrupt outputs remain recorded.

SIGINT/Ctrl+C or SIGTERM stops repetition/pause scheduling. A running child receives SIGINT in its dedicated process group, with a two-second grace period before forced termination. Children are waited for and reaped. Previously completed child files remain usable; incomplete current measurements never acquire scores. The manifest stays incomplete. If the parent is forcibly killed, the saved running attempt remains readable as pending. There is no resume mode: choose a new output for another series.

Keep the manifest and its `.runs` folder together when copying results. For example, from **Windows PowerShell**, outside your SSH session:

```powershell
scp USER@SERVER:~/serverbench/results/server-a-series.json "$HOME\Downloads\server-a-series.json"
scp -r USER@SERVER:~/serverbench/results/server-a-series.runs "$HOME\Downloads\"
```

The offline reader resolves child paths inside the manifest's directory, rejects traversal and symlink escapes, revalidates each successful child and verifies its exact-file/content/cohort hashes and requested options. Do not edit child files after recording a series. Displayed statistics are recomputed from verified children; saved manifest summaries are never trusted as input evidence.

## Descriptive comparisons

```bash
./serverbench compare results/server-a-series.json results/server-b-series.json
```

Single v1 results and repeat series can be compared. Each side must contain one compatible cohort for numerical differences. Changed allocation/protocol creates separate cohorts; no pooled percentage is produced for a multi-cohort series. Labels and run counts are shown. Completed valid children from a partial/failed series may be described, with that series status visible.

Summaries describe distinct completed **runs**: count, median, min/max, mean, population SD, CV and median absolute deviation (MAD). CV is unavailable when the mean is zero. Per-workload summaries use each run's workload median. One-second windows remain within-run measurements; they never inflate the repetition count.

Canonical content hashes deduplicate renamed/relabelled copies. Content hashes do not authenticate measurements or establish physical independence. Stable allocation cohorts include contract/build/mode/durations plus guest-visible architecture, affinity/cpuset, quota/capacity, workers, topology and cgroup visibility. Transient load, MHz, temperature and memory availability do not define the cohort. Legacy unknown limits stay unknown; PR 1 does not improve topology discovery or infer hidden host limits.

Survey now records cohort IDs alongside its chronological attempts and adds the same descriptive cohort summaries. For changed cohorts or duplicate evidence, `pooled_summary_available` is false, legacy aggregate `cpu_index` is an empty statistics object (`count: 0`) and aggregate steal is null. Individual records and cohort summaries remain intact. Old survey JSON without the new optional fields remains deserializable. No inferential confidence claim is attached to serial survey observations.

## Explicit conditional inference

```bash
./serverbench compare results/server-a-series.json results/server-b-series.json \
  --uncertainty independent-runs
```

This option explicitly assumes independent draws **within each cohort** under reasonably stable sampling conditions. A pause does not establish independence; the software cannot verify the assumption. Without this option, comparison is descriptive.

Median intervals use actual ordered observations and exact integer binomial tails for N≤100. Endpoints are not interpolated. Requested coverage is 95%; the output also reports the attained binomial coverage bound. Three or five runs cannot provide a finite two-sided 95% median interval and return `insufficient_repeats`. Six runs can use min/max with 96.875% coverage in the continuous IID model; ties can make coverage conservative.

For `median(B)/median(A)`, each side needs at least ten distinct runs. The tool combines at-least-97.5% individual median intervals as `[LB/UA, UB/LA]`; their union bound supplies at least 95% joint coverage under the stated per-cohort assumptions. Independence between A and B is not required, although shared/copied observations are explicitly flagged and suppress inference. Completed, successful, nonduplicate series are required for these claims. Incomplete/failed series do not silently become an unbiased subset.

A zero baseline median makes the point ratio undefined. A positive baseline median with zero lower interval bound retains its point estimate and reports an unbounded upper interval. Missing limits are never replaced with fabricated finite bounds. An interval spanning 1 does not resolve a direction; a resolved direction describes this measured protocol under the assumptions, not every application or future VPS availability.

## Storage and validation

The [series JSON Schema](../schemas/repeat-series-v2.schema.json) and core `series.rs` define `document_type: "repeat_series"`, schema 2, protocol `repeat-v1-sequential-1`, underlying benchmark schema 1 and `cpu-v1-experimental-1`. A series always has `score_model_id: null`, `score_status: "not_applicable"`, `scores: null`; its summaries never synthesize a scored benchmark. Requested v1 durations retain the legacy seconds representation. New pause counters are canonical decimal strings.

Path-only imports resolve and verify child files. Byte-only `read_document` can validate the manifest's structure but returns `children_verified: false` and no verified summaries until a bundle path is available. Future contracts are explicitly unsupported.

`bash scripts/check.sh` covers v1 regressions, fresh-process argument construction, lifecycle/cancellation, incremental manifests, malformed/duplicate outputs, bundle provenance/path containment, cohorts, descriptive statistics and golden interval math. `python scripts/check-contract-schema.py` (with the documented developer dependency) checks both stored format schemas. See [contract documentation](result-contracts.md).

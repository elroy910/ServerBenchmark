# ServerBench

**Compare the CPU capacity your Linux servers deliver—and how consistently they deliver it.**

[![CI](https://github.com/elroy910/ServerBenchmark/actions/workflows/ci.yml/badge.svg)](https://github.com/elroy910/ServerBenchmark/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/elroy910/ServerBenchmark)](https://github.com/elroy910/ServerBenchmark/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

ServerBench is a standalone Rust CPU benchmark for dedicated servers, VMs and shared VPSes. It measures single-core, multi-core and sustained performance, alongside variation and CPU steal. Everything needed for a benchmark is embedded in one executable.

**Linux x86_64 & ARM64 · No root · No runtime dependencies · Offline benchmarks · Local JSON results**

> **Experimental CPU standard.** Application **0.1.1** uses CPU schema **1**, contract `cpu-v1-experimental-1`. The reference is fixed but not yet empirically calibrated or frozen as a public benchmark standard. Scores describe these workloads, not every application.

## Install on Ubuntu or another Linux server

Run these commands **on the Linux server**, for example inside its SSH session. You do not need to copy this repository or install Rust, Python, Docker or benchmark packages.

### 1. Check the architecture

```bash
uname -m
```

| Output | Download |
|---|---|
| `x86_64` | [Intel/AMD 64-bit executable](https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-x86_64) |
| `aarch64` / `arm64` | [ARM64 executable](https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-arm64) |

### 2. Download and verify

For **Intel/AMD x86_64**:

```bash
mkdir -p ~/serverbench
cd ~/serverbench
wget -O serverbench-linux-x86_64 https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-x86_64
wget -O serverbench-linux-x86_64.sha256 https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-x86_64.sha256
sha256sum -c serverbench-linux-x86_64.sha256
```

Continue when the checksum reports `OK`:

```bash
mv serverbench-linux-x86_64 serverbench
chmod +x serverbench
./serverbench --version
./serverbench info
./serverbench validate
```

<details>
<summary><strong>ARM64 installation commands</strong></summary>

```bash
mkdir -p ~/serverbench
cd ~/serverbench
wget -O serverbench-linux-arm64 https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-arm64
wget -O serverbench-linux-arm64.sha256 https://github.com/elroy910/ServerBenchmark/releases/latest/download/serverbench-linux-arm64.sha256
sha256sum -c serverbench-linux-arm64.sha256
```

After the checksum reports `OK`:

```bash
mv serverbench-linux-arm64 serverbench
chmod +x serverbench
./serverbench validate
```

</details>

Downloads become available after the first GitHub Release finishes publishing. A 404 before then means there is no published binary yet. See [publishing the first release](docs/publishing.md).

For repeatable comparisons, use the same tagged release on every server: replace `/releases/latest/download/` with `/releases/download/v0.1.1/`. Release archives and checksums are also available on the [Releases page](https://github.com/elroy910/ServerBenchmark/releases).

### 3. Benchmark the server

First, optionally check that everything runs in quick mode:

```bash
./serverbench benchmark --quick --label "Proxmox Ubuntu VM" --output results/vm-quick.json
```

Use **full mode** to compare servers:

```bash
./serverbench benchmark --full --label "Proxmox Ubuntu VM" --output results/vm-full.json
```

On another server, use its own label and filename:

```bash
./serverbench benchmark --full --label "Dedicated Ubuntu" --output results/dedicated-full.json
```

The active phases use the available CPU capacity. Choose a suitable time on a busy server and keep SSH open until the run finishes. Results stay local and existing JSON files are never overwritten; use new filenames for repeat runs.

## Compare results

Collect both JSON files onto any Linux machine with ServerBench, or use [WSL on Windows](docs/windows.md):

```bash
./serverbench compare vm-full.json dedicated-full.json
```

Illustrative output:

```text
Server A: Proxmox Ubuntu VM
  File: vm-full.json
Server B: Dedicated Ubuntu
  File: dedicated-full.json

Metric                       Server A       Server B      Difference
CPU Index                      3414.9         4941.8          +44.7%
Single                         1778.9         2016.8          +13.4%
Multi                          4036.7         6557.9          +62.5%
Per-vCPU                       1009.2         1093.0           +8.3%
Reliable                       6510.9        11020.2          +69.3%
Consistency                      96.4           92.2     -4.2 points
```

Labels are read from JSON; missing labels fall back to filenames. Hostnames are not collected automatically. Compare suppresses differences when schemas, benchmark major versions, modes, durations, dataset/reference contracts or compiler/build configuration differ. Old compatible JSON results remain readable after application updates.

## What the numbers mean

| Metric | Meaning |
|---|---|
| **Single** | Geometric mean of normalized single-worker workload medians |
| **Multi** | Same workloads using the effective available worker count |
| **Per-vCPU** | Multi score divided by the benchmark worker count |
| **Average** | Average sustained mixed-workload throughput, normalized |
| **Reliable** | P10 sustained throughput, normalized: a view of lower-end delivered capacity |
| **Consistency** | 0–100 summary of variation, low throughput, degradation, steal and outliers |
| **CPU Index** | Weighted geometric mean: Single 35%, Multi 40%, Reliable 25% |

Five deterministic workloads cover four categories: integer/branching, DEFLATE compression and decompression, software SHA-256, and JSON/string/sort/filter/lookup processing. Each batch checks its result. Dataset generation and preparation happen before timing; workers perform no filesystem/network I/O.

The sustained workload repeats a fixed mix of all five batches. Its unit is mixed cycles/s. Raw throughput, distributions, complete time series and metadata are preserved alongside scores. See [methodology](docs/methodology.md) for units, percentiles and exact formulas.

### Why shared VPSes need sustained measurements

Advertised vCPUs do not guarantee exclusive physical CPU time. Scheduling, oversubscription, noisy neighbours and provider limits can change delivered throughput. A short burst may look better than a longer run.

ServerBench measures `/proc/stat` steal deltas together with throughput variation. Missing or invalid counters become `null`; measured zero remains zero. Zero steal alone does not rule out contention. A lower Reliable score or changing throughput deserves attention even when steal is zero.

## Modes and live stats

| Mode | Idle | Warm-up | Each single/multi workload | Sustained | Approximate total |
|---|---:|---:|---:|---:|---:|
| `--quick` | 5 s | 5 s | 4 s | 30 s | 80 seconds |
| `--full` / default | 30 s | 15 s | 15 s | 5 min | 8 min 15 s |
| `--extended` | 30 s | 15 s | 15 s | 26 min 45 s | 30 minutes |

Preparation and finishing the last batch add small overhead. `--sustained 10m` overrides the sustained phase. Quick and full results are not equivalent.

Live updates show phase elapsed/target time, CPU usage, RAM, CPU temperature, load, steal and frequency, approximately every five seconds:

```text
Sustained combined CPU workload [31/300s] CPU 98.5% | RAM 3.2/15.6 GiB (21%) | CPU temp 61.0 C | Load 4.10/2.30/1.80 | Steal 0.0% | Freq 3200.0 MHz
```

CPU usage covers allowed logical CPUs. RAM is the Linux system's `MemTotal - MemAvailable`, not just this process; in a VM it describes guest-visible memory. Cgroup memory usage/limits appear separately when available. Load is the 1/5/15-minute average. Temperature uses recognized CPU sensors and shows `unknown` if unavailable, as on many VMs. These observations are informational, never scoring inputs.

Progress goes through a bounded nonblocking queue; slow SSH output does not block sampling. Use `--no-progress` to hide live lines, or `--quiet` to hide normal console output. JSON is still saved.

For phase messages and the final result without repeated CPU/RAM/temperature lines:

```bash
./serverbench benchmark --quick --no-progress --label "Proxmox Ubuntu VM" --output results/vm-quick.json
```

Use a new output filename for each run. `--quiet` also hides the phase messages and final console summary. Currently `--verbose` adds executable/reference hashes; detailed live stats are enabled by default. A [planned compact progress mode](docs/plans/implementation-backlog.md#cli-follow-up--compact-progress-and-verbose-details) will make detailed live stats opt-in with `--verbose`.

### Unknown fields inside a Proxmox/QEMU VM

Missing optional observations do not invalidate a CPU benchmark. ServerBench reports the interfaces exposed to the guest; it does not query the Proxmox host.

| Output | Meaning in the current version |
|---|---|
| Governor/current MHz/nominal MHz unknown | The corresponding guest CPUFreq sysfs files are absent, unreadable or not parseable. These fields are not inferred from the host CPU model. |
| CPU temperature unknown | No supported readable CPU sensor was found in the guest. |
| Container unknown | No recognized container marker was found. This is separate from detecting QEMU virtualization; it is not proof that no container exists. |
| CPU quota / cgroup memory limit unknown | Legacy v1 cannot distinguish an unlimited setting from an unavailable/unreadable limit. Linux `cpu.max` and `memory.max` can contain `max`; this does not mean zero resources. |
| Physical cores / sockets / SMT / NUMA | Guest-visible topology, which need not match the physical host or represent dedicated cores. |
| Effective capacity 4.000 | Four guest CPU equivalents under the limits the detector could observe; not a guarantee of four exclusive host cores. |

`Cgroup visibility: visible hierarchy` concerns guest-visible cgroup ancestry. It does not reveal host-side VM quotas, scheduling or resource contention. Guest `MemTotal`, available memory and effective headroom likewise describe memory visible inside the VM. See the Linux [CPUFreq](https://docs.kernel.org/admin-guide/pm/cpufreq.html) and [cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html) interfaces for the underlying field semantics.

## Repeat runs and compare series

Application 0.1.2 source adds sequential fresh-process repetitions:

```bash
./serverbench benchmark --full --repeat 3 --repeat-pause 60s --no-progress \
  --label "Server A" --output results/server-a-series.json
./serverbench compare results/server-a-series.json results/server-b-series.json
```

Three full runs with default pauses take approximately **26m45s**, plus initialization. The manifest and sibling `server-a-series.runs/` directory contain incremental status and the original child results; copy both when moving results. Existing destinations are refused. `--repeat 1` preserves single-result behavior. `--continue-on-error` finishes later attempts while keeping a failed series unsuccessful.

Summaries report distinct-run median/range/SD/CV/MAD and per-workload medians. Changed allocations form separate cohorts; copied results do not add repetitions. Optional `compare ... --uncertainty independent-runs` requests conditional statistical intervals under explicit assumptions; three runs do **not** provide a finite 95% median interval. Read the [repeat guide](docs/repeated-runs.md) for failures, cancellation, copying bundles and inferential limits. Download availability follows publication of application 0.1.2.

## Survey a shared VPS

```bash
./serverbench survey --duration 24h --interval 2h --jitter 5m --label "VPS Medium"
```

Survey runs full benchmarks at absolute monotonic start slots with random scheduling jitter. Late slots are skipped rather than queued. Every completed result is saved immediately; incremental `survey.json` retains chronological records and descriptive allocation/protocol cohorts. With one unchanged cohort it also summarizes CPU Index median/P10/P90/min/max/CV and sustained steal. Changed cohorts suppress the pooled summary. Finished runs survive an interruption; the incomplete current run is not scored. A run started before the survey deadline may finish after it.

## More CLI examples

```bash
./serverbench info --json
./serverbench validate --expected-sha256 TRUSTED_EXECUTABLE_SHA256
./serverbench benchmark --full --sustained 10m --strict
./serverbench benchmark --label "VPS Medium" --provider "Provider X" --monthly-cost 48.85 --currency EUR
./serverbench --help
```

`validate` checks datasets, workload known answers, reference, version contract, timer, memory and affinity. Without `--expected-sha256`, it explicitly reports executable integrity as unverified; workload checks still run. Price is user metadata and contributes only to separate points-per-currency metrics, never CPU Index.

| Exit | Meaning |
|---:|---|
| 0 | Success |
| 1 | Execution/I/O failure, or survey with no successful runs |
| 2 | Invalid arguments/configuration |
| 3 | Unsupported platform |
| 4 | Workload, reference, checksum or result validation failure |
| 5 | Constrained system or strict preflight failure |

## Fair comparisons, limits and privacy

- Use the same release, mode and durations. Repeat full runs at different times and inspect the raw measurements.
- Comparisons flag copied or relabelled runs as duplicates. Incompatible contracts suppress score differences; preview results remain explicitly uncalibrated.
- Workers respect process affinity, cpusets and visible cgroup v1/v2 ancestor quotas. Fractional quotas are handled conservatively. Hidden host ancestors are reported as incomplete visibility.
- Governor, frequency, topology, virtualization, NUMA and limits are recorded when available. Unknown facts stay unknown. ServerBench changes no system configuration.
- A score is workload-specific. Say “scored 44.7% higher on ServerBench CPU schema 1,” rather than claiming that every application will run 44.7% faster.
- The current scope is CPU. There are no disk, network, database, GPU or standalone memory-bandwidth benchmarks.
- No automatic uploads, accounts or telemetry service. No IP/MAC addresses, usernames, hostnames, home directories, serial numbers, machine IDs, cloud metadata or credentials are collected by default. User-supplied labels may identify a server.
- Store results on a Linux filesystem with hardlink support; result publication uses a fully written, synced temporary file and a non-overwriting hardlink.

Application and benchmark versions are separate. Presentation fixes can preserve score compatibility; changing workloads, datasets, scoring or fundamental durations requires a new benchmark contract. The embedded [reference](reference/cpu-v1.json) has a pinned SHA-256. Read [versioning](docs/versioning.md) and the [empirical validation checklist](docs/validation.md) before treating the experimental baseline as a standard.

## Build and contribute

Release users only need the binary. Developers need the pinned Rust 1.85.1 toolchain and a Linux linker:

```bash
cargo build --release --locked -p serverbench-cli
bash scripts/check.sh
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
bash scripts/package.sh x86_64-unknown-linux-musl
bash scripts/package.sh aarch64-unknown-linux-musl
```

Static musl builds use Rust's bundled linker. GNU ARM64 cross-builds need `aarch64-linux-gnu-gcc`. Build dependencies are not required on servers running the executable. Do not use `target-cpu=native` for generic comparable releases.

The workspace separates CLI, core models, system detection, CPU workloads, runtime metrics, scoring, reporting and survey orchestration. See [Contributing](CONTRIBUTING.md), [Changelog](CHANGELOG.md) and [Publishing](docs/publishing.md).

PR 0 adds [validated document contracts and regression fixtures](docs/result-contracts.md). The schema 2 preview is read-only groundwork; `benchmark` continues to use the existing v1 engine and scores.

Planned benchmark work is documented in the [research and implementation design](docs/plans/benchmark-improvements.md), [ordered PR backlog](docs/plans/implementation-backlog.md) and [proposed data contracts](docs/plans/result-formats.md). Beyond PR 0 these describe future work; the documented release commands above remain the currently supported interface.

## Publish a GitHub release

Push the source to `https://github.com/elroy910/ServerBenchmark.git`, then push a matching version tag:

```bash
git tag -a v0.1.2 -m "ServerBench 0.1.2"
git push origin v0.1.2
```

GitHub Actions checks the version, runs quality/smoke tests, builds four Linux targets, verifies checksums and publishes the complete release. `wget` links become live when that workflow finishes. See [the publishing guide](docs/publishing.md) for first-time Git setup and later versions.

## License

[MIT](LICENSE).

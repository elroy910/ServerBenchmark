use serde::{Deserialize, Serialize};
use serverbench_core::{
    statistics, BenchmarkResult, Durations, Error, Metadata, Mode, PricePerformance, ProgressEvent,
    Result, Statistics,
};
use serverbench_cpu::{
    idle_observed,
    workloads::{Kind, Workload, DATASET_SHA256},
    Engine,
};
use serverbench_report::{binary_hash, save_json, save_result, timestamp, unique_name};
use serverbench_score::{calculate, reference, reference_hash};
use serverbench_system::{cgroup_limits, current, pin, preflight, required_memory};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct RunOptions {
    pub mode: Mode,
    pub sustained: Option<u64>,
    pub strict: bool,
    pub metadata: Metadata,
}
impl RunOptions {
    pub fn durations(&self) -> Result<Durations> {
        let mut d = self.mode.durations();
        if let Some(s) = self.sustained {
            if s < 10 {
                return Err(Error::new(2, "sustained duration must be at least 10s"));
            }
            d.sustained_seconds = s;
        }
        Ok(d)
    }
}
fn measured(
    engine: &Engine<'_>,
    phase: &str,
    kind: Kind,
    workers: usize,
    seconds: u64,
    progress: &mut impl FnMut(ProgressEvent<'_>),
) -> Result<serverbench_core::Measurement> {
    progress(ProgressEvent::Message(phase));
    engine.measure_observed(kind, workers, Duration::from_secs(seconds), |sample| {
        progress(ProgressEvent::Sample {
            phase,
            duration_seconds: seconds as f64,
            sample,
        })
    })
}
pub fn run(
    options: &RunOptions,
    mut progress: impl FnMut(ProgressEvent<'_>),
) -> Result<BenchmarkResult> {
    let run_timestamp = timestamp();
    let durations = options.durations()?;
    let system = current()?;
    let mut warnings = preflight(&system, options.strict)?;
    if cfg!(debug_assertions) {
        warnings.push(
            "Unoptimized development build: use an official release binary for comparisons.".into(),
        );
    }
    if options
        .metadata
        .monthly_cost
        .is_some_and(|p| !p.is_finite() || p <= 0.)
    {
        return Err(Error::new(2, "monthly cost must be positive and finite"));
    }
    if options.metadata.monthly_cost.is_some() && options.metadata.currency.is_none() {
        return Err(Error::new(2, "monthly cost requires currency"));
    }
    progress(ProgressEvent::Message(&format!(
        "Current load average: {:?}; workers: {}; effective CPU capacity: {:.3}",
        system.load_average, system.cpu.usable_cpus, system.cpu.effective_capacity
    )));
    for warning in &warnings {
        progress(ProgressEvent::Message(&format!("WARNING: {warning}")));
    }
    progress(ProgressEvent::Message(
        "Pre-flight: checking workload and reference integrity",
    ));
    Workload::validate()?;
    reference()?;
    let hash = binary_hash()?;
    let dirs = cgroup_limits(Path::new("/")).directories;
    let cpus = &system.cpu.affinity_cpus;
    let workers = system.cpu.usable_cpus;
    let engine = Engine {
        root: Path::new("/"),
        cpus,
        cgroup_dirs: &dirs,
        interval: Duration::from_secs(durations.sample_seconds),
    };
    progress(ProgressEvent::Message("Idle baseline"));
    let idle_samples = idle_observed(
        engine.root,
        cpus,
        &dirs,
        Duration::from_secs(durations.idle_seconds),
        engine.interval,
        |sample| {
            progress(ProgressEvent::Sample {
                phase: "Idle baseline",
                duration_seconds: durations.idle_seconds as f64,
                sample,
            })
        },
    );
    let idle_usage: Vec<_> = idle_samples
        .iter()
        .filter_map(|s| s.cpu_utilization_percent)
        .collect();
    if !idle_usage.is_empty() && statistics(&idle_usage).average > 20. {
        if options.strict {
            return Err(Error::new(
                5,
                "strict preflight failed: idle CPU utilization exceeds 20%",
            ));
        }
        warnings.push(
            "Idle CPU utilization exceeds 20%; existing activity may influence results.".into(),
        );
    }
    measured(
        &engine,
        "Warm-up (excluded from scores)",
        Kind::Mixed,
        workers,
        durations.warmup_seconds,
        &mut progress,
    )?;
    let mut single = BTreeMap::new();
    let mut multi = BTreeMap::new();
    for kind in Kind::ALL {
        single.insert(
            kind.name().into(),
            measured(
                &engine,
                &format!("Single-core: {}", kind.name()),
                kind,
                1,
                durations.workload_seconds,
                &mut progress,
            )?,
        );
    }
    for kind in Kind::ALL {
        let mut measurement = measured(
            &engine,
            &format!("Multi-core: {}", kind.name()),
            kind,
            workers,
            durations.workload_seconds,
            &mut progress,
        )?;
        measurement.scaling_efficiency = (single[kind.name()].throughput.median > 0.).then_some(
            measurement.throughput.median
                / (single[kind.name()].throughput.median * workers as f64),
        );
        multi.insert(kind.name().into(), measurement);
    }
    let sustained = measured(
        &engine,
        "Sustained combined CPU workload",
        Kind::Mixed,
        workers,
        durations.sustained_seconds,
        &mut progress,
    )?;
    for m in single
        .values()
        .chain(multi.values())
        .chain(std::iter::once(&sustained))
    {
        if m.affinity_applied.iter().any(|p| !*p) {
            warnings.push(format!(
                "{}: CPU affinity could not be applied to every worker",
                m.workload
            ));
        }
        if m.worker_finish_skew_seconds > 0.1 {
            warnings.push(format!("{}: worker finish skew exceeded 100ms", m.workload));
        }
    }
    let steal_values: Vec<_> = idle_samples
        .iter()
        .chain(single.values().flat_map(|m| &m.samples))
        .chain(multi.values().flat_map(|m| &m.samples))
        .chain(&sustained.samples)
        .filter_map(|s| s.steal_percent)
        .collect();
    let scores = calculate(&single, &multi, &sustained, durations.sample_seconds)?;
    let price_performance = options.metadata.monthly_cost.map(|cost| PricePerformance {
        cpu_points_per_currency: scores.cpu_index / cost,
        reliable_points_per_currency: scores.reliable / cost,
    });
    Ok(BenchmarkResult {
        schema_version: serverbench_core::SCHEMA_VERSION,
        application_version: env!("CARGO_PKG_VERSION").into(),
        benchmark_version: serverbench_core::BENCHMARK_VERSION.into(),
        benchmark_schema_version: serverbench_core::BENCHMARK_SCHEMA,
        contract: serverbench_core::CONTRACT.into(),
        baseline_status: "experimental".into(),
        reference_sha256: reference_hash(),
        dataset_sha256: DATASET_SHA256.into(),
        binary_sha256: Some(hash),
        build_target: env!("SERVERBENCH_TARGET").into(),
        compiler_version: env!("SERVERBENCH_COMPILER").into(),
        optimized_build: !cfg!(debug_assertions),
        hash_implementation: "sha2 0.10.8 SHA-256 force-soft; hardware acceleration disabled"
            .into(),
        timestamp: run_timestamp,
        benchmark_mode: options.mode,
        durations,
        system,
        metadata: options.metadata.clone(),
        warnings,
        idle_samples,
        single,
        multi,
        sustained,
        steal: (!steal_values.is_empty()).then(|| statistics(&steal_values)),
        scores,
        price_performance,
    })
}

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub binary_sha256: String,
    pub binary_integrity_verified: bool,
    pub checks: Vec<String>,
    pub warnings: Vec<String>,
}
pub fn validate(expected_sha256: Option<&str>) -> Result<ValidationReport> {
    let system = current()?;
    Workload::validate()?;
    reference()?;
    let hash = binary_hash()?;
    if let Some(expected) = expected_sha256 {
        if hash != expected.to_lowercase() {
            return Err(Error::new(
                4,
                "binary integrity failed: SHA-256 does not match trusted checksum",
            ));
        }
    }
    if system
        .memory_headroom_bytes
        .is_none_or(|m| m < required_memory(system.cpu.usable_cpus))
    {
        return Err(Error::new(5, "insufficient or unknown available memory"));
    }
    let before = Instant::now();
    thread::sleep(Duration::from_millis(10));
    if before.elapsed() < Duration::from_millis(10) {
        return Err(Error::new(4, "monotonic timer self-test failed"));
    }
    // Pin a temporary thread so the caller's affinity remains unchanged.
    let cpu = system.cpu.affinity_cpus[0];
    let affinity_ok = thread::spawn(move || pin(cpu)).join().unwrap_or(false);
    let mut warnings = preflight(&system, false)?;
    if !affinity_ok {
        warnings.push(
            "CPU affinity could not be applied; benchmark will report unpinned workers.".into(),
        );
    }
    if expected_sha256.is_none() {
        warnings.push("Binary hash computed, but authenticity/integrity is unverified without --expected-sha256 from a trusted release manifest.".into());
    }
    Ok(ValidationReport {
        valid: true,
        binary_sha256: hash,
        binary_integrity_verified: expected_sha256.is_some(),
        checks: vec![
            "fixed dataset SHA-256".into(),
            "workload known answers and compression round-trip".into(),
            "reference SHA-256 and benchmark contract".into(),
            "monotonic high-resolution timer".into(),
            "CPU affinity probe".into(),
            "memory headroom".into(),
        ],
        warnings,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyEntry {
    pub timestamp: String,
    pub result_file: PathBuf,
    pub cpu_index: f64,
    pub sustained_steal: Vec<f64>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SurveySummary {
    pub schema_version: u32,
    pub contract: String,
    pub benchmark_mode: Mode,
    pub durations: Durations,
    pub timestamp: String,
    pub requested_duration_seconds: u64,
    pub interval_seconds: u64,
    pub jitter_seconds: u64,
    pub completed: bool,
    pub runs: Vec<SurveyEntry>,
    pub cpu_index: Statistics,
    pub steal: Option<Statistics>,
    pub failures: Vec<String>,
}
pub fn jitter(max_seconds: u64) -> Result<Duration> {
    if max_seconds == 0 {
        return Ok(Duration::ZERO);
    }
    let range = max_seconds
        .checked_add(1)
        .ok_or_else(|| Error::new(2, "jitter too large"))?;
    let zone = u64::MAX - u64::MAX % range;
    let mut file = File::open("/dev/urandom")?;
    loop {
        let mut bytes = [0u8; 8];
        file.read_exact(&mut bytes)?;
        let n = u64::from_le_bytes(bytes);
        if n < zone {
            return Ok(Duration::from_secs(n % range));
        }
    }
}
/// Absolute monotonic slots; skip overruns instead of running a catch-up burst.
pub fn next_slot(elapsed: Duration, interval: Duration) -> u64 {
    (elapsed.as_nanos() / interval.as_nanos() + 1).min(u64::MAX as u128) as u64
}
pub struct SurveyOptions {
    pub run: RunOptions,
    pub duration: u64,
    pub interval: u64,
    pub jitter: u64,
    pub output: PathBuf,
}
pub fn survey(
    options: &SurveyOptions,
    mut progress: impl FnMut(ProgressEvent<'_>),
) -> Result<PathBuf> {
    if options.duration == 0 || options.interval == 0 || options.jitter >= options.interval {
        return Err(Error::new(
            2,
            "duration/interval must be positive; jitter must be less than interval",
        ));
    }
    let durations = options.run.durations()?;
    current()?;
    fs::create_dir_all(&options.output)?;
    let dir = options.output.join(unique_name("survey"));
    fs::create_dir(&dir)?;
    let summary_path = dir.join("survey.json");
    let mut summary = SurveySummary {
        schema_version: 1,
        contract: serverbench_core::CONTRACT.into(),
        benchmark_mode: options.run.mode,
        durations,
        timestamp: timestamp(),
        requested_duration_seconds: options.duration,
        interval_seconds: options.interval,
        jitter_seconds: options.jitter,
        completed: false,
        runs: Vec::new(),
        cpu_index: Statistics::default(),
        steal: None,
        failures: Vec::new(),
    };
    save_json(&summary_path, &summary, false)?;
    let start = Instant::now();
    let total = Duration::from_secs(options.duration);
    let interval = Duration::from_secs(options.interval);
    let mut slot = 0u64;
    loop {
        let delay = jitter(options.jitter)?;
        let due = interval
            .checked_mul(u32::try_from(slot).map_err(|_| Error::new(2, "too many survey slots"))?)
            .and_then(|d| d.checked_add(delay))
            .ok_or_else(|| Error::new(2, "survey schedule overflow"))?;
        if due >= total || start.elapsed() >= total {
            break;
        }
        while start.elapsed() < due {
            thread::sleep(
                due.saturating_sub(start.elapsed())
                    .min(Duration::from_secs(1)),
            );
        }
        if start.elapsed() >= total {
            break;
        }
        progress(ProgressEvent::Message(&format!(
            "Survey slot {slot}; saving to {}",
            dir.display()
        )));
        match run(&options.run, &mut progress) {
            Ok(result) => {
                let path = dir.join(format!("{}.json", unique_name("serverbench")));
                save_result(&path, &result)?;
                summary.runs.push(SurveyEntry {
                    timestamp: result.timestamp.clone(),
                    result_file: path,
                    cpu_index: result.scores.cpu_index,
                    sustained_steal: result
                        .sustained
                        .samples
                        .iter()
                        .filter_map(|s| s.steal_percent)
                        .collect(),
                });
                summary.cpu_index =
                    statistics(&summary.runs.iter().map(|r| r.cpu_index).collect::<Vec<_>>());
                let steal: Vec<_> = summary
                    .runs
                    .iter()
                    .flat_map(|r| r.sustained_steal.iter().copied())
                    .collect();
                summary.steal = (!steal.is_empty()).then(|| statistics(&steal));
            }
            Err(e) => {
                summary
                    .failures
                    .push(format!("{} slot {slot}: {e}", timestamp()));
                progress(ProgressEvent::Message(&format!("Survey run failed: {e}")));
            }
        }
        save_json(&summary_path, &summary, true)?;
        slot = next_slot(start.elapsed(), interval);
    }
    summary.completed = true;
    save_json(&summary_path, &summary, true)?;
    if summary.runs.is_empty() {
        return Err(Error::new(
            1,
            format!(
                "survey produced no successful runs; see {}",
                summary_path.display()
            ),
        ));
    }
    progress(ProgressEvent::Message(&format!("Survey: {} runs; CPU Index median {:.1}, P10 {:.1}, P90 {:.1}, min {:.1}, max {:.1}, CV {:.2}%",summary.runs.len(),summary.cpu_index.median,summary.cpu_index.p10,summary.cpu_index.p90,summary.cpu_index.minimum,summary.cpu_index.maximum,summary.cpu_index.coefficient_of_variation*100.)));
    Ok(summary_path)
}
pub fn parse_duration(input: &str) -> Result<u64> {
    let (number, multiplier) = match input.as_bytes().last() {
        Some(b's') => (&input[..input.len() - 1], 1u64),
        Some(b'm') => (&input[..input.len() - 1], 60),
        Some(b'h') => (&input[..input.len() - 1], 3600),
        Some(b'd') => (&input[..input.len() - 1], 86400),
        _ => (input, 1),
    };
    number
        .parse::<u64>()
        .ok()
        .and_then(|v| v.checked_mul(multiplier))
        .filter(|s| *s > 0 && *s <= 31_536_000)
        .ok_or_else(|| {
            Error::new(
                2,
                "duration must be positive integer seconds or use s/m/h/d (maximum 365d)",
            )
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduling_skips_overruns() {
        assert_eq!(
            next_slot(Duration::from_secs(125), Duration::from_secs(60)),
            3
        );
        assert_eq!(
            next_slot(Duration::from_secs(60), Duration::from_secs(60)),
            2
        );
    }
    #[test]
    fn durations_are_checked() {
        assert_eq!(parse_duration("24h").unwrap(), 86400);
        assert_eq!(parse_duration("5m").unwrap(), 300);
        for s in ["0", "-1", "NaN", "1.5h", "99999999999999999d", ""] {
            assert!(parse_duration(s).is_err());
        }
    }
}

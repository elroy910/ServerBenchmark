use serde::Serialize;
mod documents;
mod progress;
mod repeat;
pub use documents::{
    distinct_run_count, duplicate_kind, load_document, read_document, DuplicateKind,
    LoadedDocument, LoadedSeries, ReadOutcome, SeriesRun,
};
pub use progress::{live_line, ProgressOutput, ProgressReporter};
pub use repeat::{require_series, series_console};
use serverbench_core::{BenchmarkResult, Result, SystemInfo};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as FmtWrite,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn timestamp() -> String {
    timestamp_at(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
}
pub fn timestamp_at(seconds: u64) -> String {
    // Proleptic Gregorian conversion; wall time is only used for metadata.
    let days = (seconds / 86400) as i64;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}
pub fn unique_name(prefix: &str) -> String {
    format!(
        "{}-{}-{}",
        prefix,
        timestamp().replace(':', ""),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
    )
}
pub fn binary_hash() -> Result<String> {
    let mut file = File::open(std::env::current_exe()?)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
/// Fully write and sync a sibling temp file. Never overwrite an existing result.
pub fn save_json<T: Serialize>(path: &Path, value: &T, replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.tmp", unique_name("serverbench")));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        if replace {
            fs::rename(&temp, path)?;
        } else {
            fs::hard_link(&temp, path)?;
            fs::remove_file(&temp)?;
        }
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub fn save_result(path: &Path, r: &BenchmarkResult) -> Result<()> {
    serverbench_score::validate_result(r)?;
    save_json(path, r, false)
}
pub fn load_result(path: &Path) -> Result<BenchmarkResult> {
    documents::legacy_result(load_document(path)?)
}
fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|s| s.to_string()).unwrap_or("unknown".into())
}
pub fn info(s: &SystemInfo) -> String {
    format!("ServerBench {} (CPU schema {}, experimental)\n\nSystem\nArchitecture          {}\nKernel                {}\nDistribution          {}\nVirtualized           {}\nHypervisor            {}\nContainer             {}\n\nCPU\nModel                 {}\nLogical CPUs          {}\nBenchmark workers     {}\nEffective capacity    {:.3}\nAllowed CPU IDs       {:?}\nCpuset                {:?}\nCPU quota             {}\nCgroup version        {}\nCgroup visibility     {}\nPhysical cores*       {}\nSockets*              {}\nSMT*                  {}\nNUMA nodes            {}\nGovernor              {}\nCurrent MHz           {}\nNominal MHz           {}\n\nMemory\nTotal bytes           {}\nAvailable bytes       {}\nCgroup limit bytes    {}\nEffective headroom    {}\n\nLoad average          {:?}\nUptime seconds        {}\n* Topology describes CPUs visible to this process.\n",env!("CARGO_PKG_VERSION"),serverbench_core::BENCHMARK_SCHEMA,s.architecture,opt(s.kernel.as_deref()),opt(s.distribution.as_deref()),opt(s.virtualized),opt(s.hypervisor.as_deref()),opt(s.container.as_deref()),opt(s.cpu.model.as_deref()),s.cpu.logical_cpus,s.cpu.usable_cpus,s.cpu.effective_capacity,s.cpu.affinity_cpus,s.cpu.cpuset,opt(s.cpu.quota_cpus),opt(s.cpu.cgroup_version),if s.cpu.cgroup_limits_complete {"visible hierarchy"} else {"incomplete/namespace"},opt(s.cpu.physical_cores),opt(s.cpu.sockets),opt(s.cpu.smt),opt(s.cpu.numa_nodes),opt(s.cpu.governor.as_deref()),opt(s.cpu.current_mhz),opt(s.cpu.nominal_mhz),opt(s.memory_total_bytes),opt(s.memory_available_bytes),opt(s.memory_limit_bytes),opt(s.memory_headroom_bytes),s.load_average,opt(s.uptime_seconds))
}
pub fn console(r: &BenchmarkResult) -> String {
    let s = &r.scores;
    let mut out=format!("ServerBench {} / {} / {:?}\nExperimental CPU baseline; not a public frozen standard.\n\nSingle-core           {:.0}\nMulti-core            {:.0}\nPer-vCPU              {:.0}\nAverage performance   {:.0}\nReliable CPU (P10)    {:.0}\nCPU Index             {:.0}\n\nConsistency           {:.1} / 100 ({})\nThroughput variation  {:.2}%\n",r.application_version,r.contract,r.benchmark_mode,s.single,s.multi,s.per_vcpu,s.average,s.reliable,s.cpu_index,s.consistency,s.consistency_label,r.sustained.throughput.coefficient_of_variation*100.);
    let mut identity = format!("Server label          {}\n", result_name(r, "unlabelled"));
    if let Some(provider) = r
        .metadata
        .provider
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let _ = writeln!(identity, "Provider              {}", display_text(provider));
    }
    out.insert_str(
        out.find("\n\nSingle-core").expect("report score heading") + 2,
        &identity,
    );
    if let Some(t) = &r.steal {
        let _ = writeln!(
            out,
            "Steal avg/median      {:.2}% / {:.2}%\nSteal P95/P99/max     {:.2}% / {:.2}% / {:.2}%",
            t.average, t.median, t.p95, t.p99, t.maximum
        );
    } else {
        out.push_str("Steal                 unknown\n");
    }
    out.push_str("\nWorkload       Single median    Multi median     Per worker     Scaling\n");
    for name in serverbench_core::WORKLOADS {
        let a = &r.single[name];
        let b = &r.multi[name];
        let bytes = a.unit == "bytes/s";
        let divisor = if bytes { 1e6 } else { 1. };
        let unit = if bytes { "MB/s" } else { a.unit.as_str() };
        let _ = writeln!(
            out,
            "{name:<14} {:>10.2} {:>15.2} {:>14.2} {:>9.1}%  {unit}",
            a.throughput.median / divisor,
            b.throughput.median / divisor,
            b.throughput.median / divisor / b.workers as f64,
            b.scaling_efficiency.unwrap_or(0.) * 100.
        );
    }
    let _ = writeln!(
        out,
        "Hash operations/s     {:.2} single / {:.2} multi (64 KiB per hash)",
        r.single["hash"].throughput.median / 65536.,
        r.multi["hash"].throughput.median / 65536.
    );
    if let Some(p) = &r.price_performance {
        let _=writeln!(out,"\nMonthly cost          {} {}\nCPU points / unit     {:.2}\nReliable / unit       {:.2}",r.metadata.monthly_cost.unwrap_or_default(),r.metadata.currency.as_deref().unwrap_or("unspecified"),p.cpu_points_per_currency,p.reliable_points_per_currency);
    }
    for warning in &r.warnings {
        let _ = writeln!(out, "WARNING: {warning}");
    }
    out
}
pub fn incompatibilities(a: &BenchmarkResult, b: &BenchmarkResult) -> Vec<&'static str> {
    let mut why = Vec::new();
    if a.schema_version != b.schema_version {
        why.push("result schema");
    }
    if a.benchmark_schema_version != b.benchmark_schema_version {
        why.push("benchmark schema");
    }
    if a.benchmark_version.split('.').next() != b.benchmark_version.split('.').next() {
        why.push("benchmark major version");
    }
    if a.benchmark_mode != b.benchmark_mode {
        why.push("benchmark mode");
    }
    if a.contract != b.contract
        || a.reference_sha256 != b.reference_sha256
        || a.dataset_sha256 != b.dataset_sha256
    {
        why.push("workload/reference contract");
    }
    if a.durations != b.durations {
        why.push("benchmark durations");
    }
    if a.compiler_version != b.compiler_version || a.optimized_build != b.optimized_build {
        why.push("compiler/build configuration");
    }
    why
}
fn display_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn result_name(r: &BenchmarkResult, fallback: &str) -> String {
    r.metadata
        .label
        .as_deref()
        .map(display_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| display_text(fallback))
}
pub fn compare(a: &BenchmarkResult, b: &BenchmarkResult) -> Result<String> {
    compare_named(a, b, None, None)
}
pub fn compare_files(a: &Path, b: &Path) -> Result<String> {
    compare_files_with_uncertainty(a, b, false)
}
pub fn compare_files_with_uncertainty(
    a: &Path,
    b: &Path,
    independent_runs: bool,
) -> Result<String> {
    use serverbench_core::documents::StoredDocument;
    let left = load_document(a)?;
    let right = load_document(b)?;
    if independent_runs
        || matches!(left, ReadOutcome::Series(_))
        || matches!(right, ReadOutcome::Series(_))
    {
        let sources = format!(
            "File A: {}\nFile B: {}\n",
            display_text(a.file_name().and_then(|v| v.to_str()).unwrap_or("unknown")),
            display_text(b.file_name().and_then(|v| v.to_str()).unwrap_or("unknown"))
        );
        return Ok(sources + &repeat::compare_runs(&left, &right, independent_runs)?);
    }
    let file_a = a.file_name().and_then(|s| s.to_str());
    let file_b = b.file_name().and_then(|s| s.to_str());
    let duplicate = match (&left, &right) {
        (ReadOutcome::Supported(a), ReadOutcome::Supported(b)) => duplicate_kind(a, b),
        _ => None,
    };
    let mut out = match (&left, &right) {
        (ReadOutcome::Supported(a), ReadOutcome::Supported(b)) => {
            match (&a.document, &b.document) {
                (StoredDocument::LegacyBenchmarkV1(a), StoredDocument::LegacyBenchmarkV1(b)) => {
                    compare_named(a, b, file_a, file_b)?
                }
                _ => unscored_comparison(&left, &right, file_a, file_b),
            }
        }
        _ => unscored_comparison(&left, &right, file_a, file_b),
    };
    if let Some(kind) = duplicate {
        let _ = writeln!(
            out,
            "NOTICE: duplicate input ({kind:?}); one distinct run, not independent measurements."
        );
    }
    Ok(out)
}
fn unscored_comparison(
    a: &ReadOutcome,
    b: &ReadOutcome,
    file_a: Option<&str>,
    file_b: Option<&str>,
) -> String {
    use serverbench_core::documents::StoredDocument;
    let mut out = String::new();
    for (name, result, file) in [("A", a, file_a), ("B", b, file_b)] {
        match result {
            ReadOutcome::Series(s) => {
                let _ = writeln!(
                    out,
                    "Server {name}: repeat series ({:?})",
                    s.document.status
                );
            }
            ReadOutcome::Unsupported(d) => {
                let _ = writeln!(
                    out,
                    "Server {name}: {}\n  Unsupported: {:?}; schema {}, type {}, contract {}",
                    display_text(file.unwrap_or("unlabelled")),
                    d.reason,
                    d.header.schema_version,
                    display_text(d.header.document_type.as_deref().unwrap_or("legacy")),
                    display_text(
                        d.header
                            .benchmark_contract_id
                            .as_deref()
                            .unwrap_or("unknown")
                    )
                );
            }
            ReadOutcome::Supported(d) => match &d.document {
                StoredDocument::RepeatSeries(_) => {
                    out.push_str("Repeat series requires a resolved child bundle.\n");
                }
                StoredDocument::LegacyBenchmarkV1(r) => {
                    let _ = writeln!(
                        out,
                        "Server {name}: {}\n  Contract: {} (legacy v1)",
                        result_name(r, file.unwrap_or("unlabelled")),
                        r.contract
                    );
                }
                StoredDocument::PreviewBenchmarkV2(r) => {
                    let label = r
                        .metadata
                        .label
                        .as_deref()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(file.unwrap_or("unlabelled"));
                    let _=writeln!(out,"Server {name}: {}\n  Contract: {} / {}; status {:?}; uncalibrated, scores null",display_text(label),r.benchmark_contract_id,r.protocol_id,r.status);
                }
            },
        }
    }
    out.push_str("WARNING: no comparable calibrated score pair; differences suppressed.\n");
    out
}
fn compare_named(
    a: &BenchmarkResult,
    b: &BenchmarkResult,
    file_a: Option<&str>,
    file_b: Option<&str>,
) -> Result<String> {
    let why = incompatibilities(a, b);
    let compatible = why.is_empty();
    if compatible {
        serverbench_score::validate_result(a)?;
        serverbench_score::validate_result(b)?;
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Server A: {}",
        result_name(a, file_a.unwrap_or("unlabelled"))
    );
    if let Some(file) = file_a {
        let _ = writeln!(out, "  File: {}", display_text(file));
    }
    let _ = writeln!(
        out,
        "Server B: {}",
        result_name(b, file_b.unwrap_or("unlabelled"))
    );
    if let Some(file) = file_b {
        let _ = writeln!(out, "  File: {}", display_text(file));
    }
    out.push('\n');
    if !compatible {
        let _ = writeln!(
            out,
            "WARNING: incompatible {}; differences suppressed.",
            why.join(", ")
        );
    }
    out.push_str("Metric                       Server A       Server B      Difference\n");
    for (label, x, y) in [
        ("CPU Index", a.scores.cpu_index, b.scores.cpu_index),
        ("Single", a.scores.single, b.scores.single),
        ("Multi", a.scores.multi, b.scores.multi),
        ("Per-vCPU", a.scores.per_vcpu, b.scores.per_vcpu),
        ("Average", a.scores.average, b.scores.average),
        ("Reliable", a.scores.reliable, b.scores.reliable),
        ("Consistency", a.scores.consistency, b.scores.consistency),
    ] {
        let difference = if !compatible {
            "n/a".into()
        } else if label == "Consistency" {
            format!("{:+.1} points", y - x)
        } else if x > 0. {
            format!("{:+.1}%", (y / x - 1.) * 100.)
        } else {
            "n/a".into()
        };
        let _ = writeln!(out, "{label:<24} {x:>12.1} {y:>14.1} {difference:>15}");
    }
    out.push_str(
        "Scores describe ServerBench CPU workloads; they do not predict every application.\n",
    );
    Ok(out)
}
pub fn default_path() -> PathBuf {
    Path::new("results").join(format!("{}.json", unique_name("serverbench")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_calendar() {
        assert_eq!(timestamp_at(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp_at(1709251200), "2024-03-01T00:00:00Z");
        assert_eq!(timestamp_at(1793275200), "2026-10-29T12:00:00Z");
    }
    #[test]
    fn atomic_writer_does_not_overwrite() {
        let dir = std::env::temp_dir().join(unique_name("serverbench-report-test"));
        let path = dir.join("test.json");
        save_json(&path, &vec![1, 2], false).unwrap();
        assert!(save_json(&path, &vec![3], false).is_err());
        let v: Vec<u32> = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        assert_eq!(v, vec![1, 2]);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}

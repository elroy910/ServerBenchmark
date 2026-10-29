use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub mod documents;
pub mod series;
pub mod v2;

pub const SCHEMA_VERSION: u32 = 1;
pub const BENCHMARK_VERSION: &str = "0.1.0";
pub const BENCHMARK_SCHEMA: u32 = 1;
pub const CONTRACT: &str = "cpu-v1-experimental-1";
pub const DATASET_SHA256: &str = "fed880d6c2ee7e7c6bdeaf14b99a18fbaa8172e5574fa00e05d35a69f79a4a61";
pub fn workload_contract(name: &str) -> Option<(&'static str, u64)> {
    match name {
        "integer" => Some(("integer_iterations/s", 32768)),
        "compression" | "decompression" | "hash" => Some(("bytes/s", 65536)),
        "data" => Some(("objects/s", 256)),
        "mixed" => Some(("mixed_cycles/s", 1)),
        _ => None,
    }
}
pub const WORKLOADS: [&str; 5] = ["integer", "compression", "decompression", "hash", "data"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuInfo {
    pub model: Option<String>,
    pub logical_cpus: usize,
    pub affinity_cpus: Vec<usize>,
    pub cpuset: Option<Vec<usize>>,
    pub quota_cpus: Option<f64>,
    pub cgroup_version: Option<u8>,
    pub cgroup_limits_complete: bool,
    pub usable_cpus: usize,
    pub effective_capacity: f64,
    pub physical_cores: Option<usize>,
    pub sockets: Option<usize>,
    pub smt: Option<bool>,
    pub numa_nodes: Option<usize>,
    pub governor: Option<String>,
    pub current_mhz: Option<f64>,
    pub nominal_mhz: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemInfo {
    pub architecture: String,
    pub kernel: Option<String>,
    pub distribution: Option<String>,
    pub virtualized: Option<bool>,
    pub hypervisor: Option<String>,
    pub containerized: Option<bool>,
    pub container: Option<String>,
    pub memory_total_bytes: Option<u64>,
    pub memory_available_bytes: Option<u64>,
    pub memory_limit_bytes: Option<u64>,
    pub memory_headroom_bytes: Option<u64>,
    pub uptime_seconds: Option<f64>,
    pub load_average: Option<[f64; 3]>,
    pub cpu: CpuInfo,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Quick,
    Full,
    Extended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Durations {
    pub idle_seconds: u64,
    pub warmup_seconds: u64,
    pub workload_seconds: u64,
    pub sustained_seconds: u64,
    pub sample_seconds: u64,
}

impl Mode {
    pub fn durations(self) -> Durations {
        match self {
            Self::Quick => Durations {
                idle_seconds: 5,
                warmup_seconds: 5,
                workload_seconds: 4,
                sustained_seconds: 30,
                sample_seconds: 1,
            },
            Self::Full => Durations {
                idle_seconds: 30,
                warmup_seconds: 15,
                workload_seconds: 15,
                sustained_seconds: 300,
                sample_seconds: 1,
            },
            Self::Extended => Durations {
                idle_seconds: 30,
                warmup_seconds: 15,
                workload_seconds: 15,
                sustained_seconds: 1605,
                sample_seconds: 1,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub label: Option<String>,
    pub provider: Option<String>,
    pub monthly_cost: Option<f64>,
    pub currency: Option<String>,
    pub anonymous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub elapsed_seconds: f64,
    pub interval_seconds: f64,
    pub throughput: Option<f64>,
    pub cpu_utilization_percent: Option<f64>,
    pub steal_percent: Option<f64>,
    pub context_switches_per_second: Option<f64>,
    pub load_average: Option<[f64; 3]>,
    pub frequency_mhz: Option<f64>,
    pub cgroup_throttled_seconds: Option<f64>,
    /// Observational telemetry added in app 0.1.1; never used in scoring.
    #[serde(default)]
    pub memory: Option<MemoryUsage>,
    #[serde(default)]
    pub cpu_temperature_celsius: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub host_total_bytes: u64,
    pub host_used_bytes: u64,
    pub cgroup_used_bytes: Option<u64>,
    pub cgroup_limit_bytes: Option<u64>,
}

pub enum ProgressEvent<'a> {
    Message(&'a str),
    Sample {
        phase: &'a str,
        duration_seconds: f64,
        sample: &'a Sample,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Statistics {
    pub count: usize,
    pub average: f64,
    pub median: f64,
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub standard_deviation: f64,
    pub coefficient_of_variation: f64,
}

/// Linear interpolation at (n - 1) * p, also called R-7.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let position = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = position.floor() as usize;
    let hi = position.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (position - lo as f64)
}

pub fn statistics(values: &[f64]) -> Statistics {
    let mut v: Vec<_> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return Statistics::default();
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let mean = v.iter().sum::<f64>() / n as f64;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
    Statistics {
        count: n,
        average: mean,
        median: percentile(&v, 0.5),
        p10: percentile(&v, 0.1),
        p50: percentile(&v, 0.5),
        p90: percentile(&v, 0.9),
        p95: percentile(&v, 0.95),
        p99: percentile(&v, 0.99),
        minimum: v[0],
        maximum: v[n - 1],
        standard_deviation: sd,
        coefficient_of_variation: if mean > 0.0 { sd / mean } else { 0.0 },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub workload: String,
    pub unit: String,
    pub workers: usize,
    pub elapsed_seconds: f64,
    pub completed_units: u64,
    pub completed_batches: u64,
    pub total_throughput: f64,
    pub throughput_per_vcpu: f64,
    pub scaling_efficiency: Option<f64>,
    pub throughput: Statistics,
    pub samples: Vec<Sample>,
    pub affinity_applied: Vec<bool>,
    pub checksums_valid: bool,
    pub worker_finish_skew_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyInputs {
    pub coefficient_of_variation: f64,
    pub p10_to_median: f64,
    pub degradation_fraction: f64,
    pub mean_steal_percent: Option<f64>,
    pub outlier_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scores {
    pub single: f64,
    pub multi: f64,
    pub per_vcpu: f64,
    pub average: f64,
    pub reliable: f64,
    pub cpu_index: f64,
    pub consistency: f64,
    pub consistency_label: String,
    pub consistency_inputs: ConsistencyInputs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePerformance {
    pub cpu_points_per_currency: f64,
    pub reliable_points_per_currency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub schema_version: u32,
    pub application_version: String,
    pub benchmark_version: String,
    pub benchmark_schema_version: u32,
    pub contract: String,
    pub baseline_status: String,
    pub reference_sha256: String,
    pub dataset_sha256: String,
    pub binary_sha256: Option<String>,
    pub build_target: String,
    pub compiler_version: String,
    pub optimized_build: bool,
    pub hash_implementation: String,
    pub timestamp: String,
    pub benchmark_mode: Mode,
    pub durations: Durations,
    pub system: SystemInfo,
    pub metadata: Metadata,
    pub warnings: Vec<String>,
    pub idle_samples: Vec<Sample>,
    pub single: BTreeMap<String, Measurement>,
    pub multi: BTreeMap<String, Measurement>,
    pub sustained: Measurement,
    pub steal: Option<Statistics>,
    pub scores: Scores,
    pub price_performance: Option<PricePerformance>,
}

#[derive(Debug)]
pub struct Error {
    pub code: u8,
    pub message: String,
}
impl Error {
    pub fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(1, e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new(4, e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentiles_and_population_variance() {
        let s = statistics(&[4., 1., 3., 2.]);
        assert_eq!(s.median, 2.5);
        assert!((s.p10 - 1.3).abs() < 1e-12);
        assert!((s.standard_deviation - 1.25_f64.sqrt()).abs() < 1e-12);
        assert_eq!(statistics(&[]).count, 0);
        assert_eq!(statistics(&[7.]).p99, 7.);
    }
    #[test]
    fn mode_contract() {
        let d = Mode::Full.durations();
        assert_eq!(
            d.idle_seconds + d.warmup_seconds + 10 * d.workload_seconds + d.sustained_seconds,
            495
        );
        assert_eq!(serde_json::to_string(&Mode::Quick).unwrap(), "\"quick\"");
    }
}

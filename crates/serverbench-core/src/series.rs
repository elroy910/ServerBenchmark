//! Repeat-series storage models. Child results retain their original contract.
use crate::{
    documents::{ComparisonKey, DocumentType, Validity},
    v2::{Counter, ScoreStatus},
    BenchmarkResult, Durations, Error, Metadata, Mode, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL: &str = "repeat-v1-sequential-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSpec {
    pub mode: Mode,
    pub durations: Durations,
    pub strict: bool,
    pub metadata: Metadata,
}
impl RunSpec {
    pub fn matches(&self, result: &BenchmarkResult) -> bool {
        self.mode == result.benchmark_mode
            && self.durations == result.durations
            && serde_json::to_value(&self.metadata).ok()
                == serde_json::to_value(&result.metadata).ok()
    }
    pub fn validate(&self) -> Result<()> {
        let expected = self.mode.durations();
        crate::v2::ensure(
            self.durations.idle_seconds == expected.idle_seconds
                && self.durations.warmup_seconds == expected.warmup_seconds
                && self.durations.workload_seconds == expected.workload_seconds
                && self.durations.sample_seconds == expected.sample_seconds
                && (10..=31_536_000).contains(&self.durations.sustained_seconds),
            "invalid repeat run durations",
        )?;
        if let Some(cost) = self.metadata.monthly_cost {
            crate::v2::ensure(
                cost.is_finite() && cost > 0. && self.metadata.currency.is_some(),
                "invalid repeat price metadata",
            )?;
        }
        Ok(())
    }
    pub fn estimated_seconds(&self, count: u32, pause: u64) -> Result<u64> {
        self.validate()?;
        self.durations
            .workload_seconds
            .checked_mul(10)
            .and_then(|v| v.checked_add(self.durations.idle_seconds))
            .and_then(|v| v.checked_add(self.durations.warmup_seconds))
            .and_then(|v| v.checked_add(self.durations.sustained_seconds))
            .and_then(|v| v.checked_mul(u64::from(count)))
            .and_then(|v| {
                pause
                    .checked_mul(u64::from(count.saturating_sub(1)))
                    .and_then(|p| v.checked_add(p))
            })
            .ok_or_else(|| Error::new(2, "repeat runtime overflow"))
    }
}
/// Only stable, observable legacy allocation fields, never load/frequency/temperature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAllocation {
    pub architecture: String,
    pub logical_cpus: usize,
    pub affinity_cpus: Vec<usize>,
    pub cpuset: Option<Vec<usize>>,
    pub quota_cpus: Option<f64>,
    pub effective_capacity: f64,
    pub workers: usize,
    pub cgroup_version: Option<u8>,
    pub visibility_complete: bool,
    pub physical_cores: Option<usize>,
    pub sockets: Option<usize>,
    pub smt: Option<bool>,
    pub numa_nodes: Option<usize>,
}
impl From<&BenchmarkResult> for LegacyAllocation {
    fn from(r: &BenchmarkResult) -> Self {
        let c = &r.system.cpu;
        Self {
            architecture: r.system.architecture.clone(),
            logical_cpus: c.logical_cpus,
            affinity_cpus: c.affinity_cpus.clone(),
            cpuset: c.cpuset.clone(),
            quota_cpus: c.quota_cpus,
            effective_capacity: c.effective_capacity,
            workers: c.usable_cpus,
            cgroup_version: c.cgroup_version,
            visibility_complete: c.cgroup_limits_complete,
            physical_cores: c.physical_cores,
            sockets: c.sockets,
            smt: c.smt,
            numa_nodes: c.numa_nodes,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunStatistics {
    pub count: u32,
    pub median: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub mean: f64,
    pub standard_deviation: f64,
    pub coefficient_of_variation: Option<f64>,
    pub median_absolute_deviation: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortSummary {
    pub cohort_id: String,
    pub comparison_key: ComparisonKey,
    pub allocation: LegacyAllocation,
    pub distinct_runs: u32,
    pub duplicate_runs: u32,
    pub run_fingerprints: Vec<String>,
    pub metrics: BTreeMap<String, RunStatistics>,
    pub single_workloads: BTreeMap<String, RunStatistics>,
    pub multi_workloads: BTreeMap<String, RunStatistics>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Valid,
    Failed,
    Invalid,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptReason {
    SpawnFailed,
    ChildFailed,
    InvalidResult,
    OptionsMismatch,
    Interrupted,
    DuplicateInput,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub ordinal: u32,
    pub child_file: String,
    pub requested: RunSpec,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub status: AttemptStatus,
    pub reason: Option<AttemptReason>,
    pub exact_file_sha256: Option<String>,
    pub canonical_run_sha256: Option<String>,
    pub cohort_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub requested_runs: u32,
    pub pause_seconds: Counter,
    pub continue_on_error: bool,
    pub options: RunSpec,
    pub finished: bool,
    pub attempts: Vec<Attempt>,
    pub cohorts: Vec<CohortSummary>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub document_type: DocumentType,
    pub application_version: String,
    pub benchmark_schema_version: u32,
    pub benchmark_contract_id: String,
    pub protocol_id: String,
    pub score_model_id: Option<String>,
    pub score_status: ScoreStatus,
    pub status: Validity,
    pub timestamp: String,
    pub payload: Payload,
    pub scores: serde_json::Value,
}
impl Document {
    pub fn validate(&self) -> Result<()> {
        use crate::v2::ensure;
        ensure(
            self.schema_version == 2
                && self.document_type == DocumentType::RepeatSeries
                && self.benchmark_schema_version == crate::BENCHMARK_SCHEMA
                && self.benchmark_contract_id == crate::CONTRACT
                && self.protocol_id == PROTOCOL,
            "unsupported repeat contract",
        )?;
        ensure(
            self.score_model_id.is_none()
                && self.score_status == ScoreStatus::NotApplicable
                && self.scores.is_null(),
            "series cannot contain a synthetic benchmark score",
        )?;
        let p = &self.payload;
        crate::v2::timestamp(&self.timestamp)?;
        crate::v2::token(&self.application_version)?;
        p.options.validate()?;
        ensure(
            (2..=100).contains(&p.requested_runs)
                && (1..=31_536_000).contains(&p.pause_seconds.0)
                && p.attempts.len() <= p.requested_runs as usize,
            "invalid repeat count/pause",
        )?;
        ensure(
            self.status != Validity::Unavailable,
            "unavailable series status",
        )?;
        let mut paths = std::collections::BTreeSet::new();
        for (i, a) in p.attempts.iter().enumerate() {
            crate::v2::timestamp(&a.started_at)?;
            if let Some(time) = &a.finished_at {
                crate::v2::timestamp(time)?;
            }
            ensure(
                a.ordinal as usize == i + 1,
                "noncontiguous attempt ordinals",
            )?;
            ensure(valid_child_path(&a.child_file), "unsafe child path")?;
            ensure(paths.insert(&a.child_file), "duplicate child path")?;
            a.requested.validate()?;
            ensure(
                serde_json::to_value(&a.requested)? == serde_json::to_value(&p.options)?,
                "attempt options disagree with series",
            )?;
            if a.status == AttemptStatus::Running {
                ensure(
                    !p.finished
                        && i + 1 == p.attempts.len()
                        && a.finished_at.is_none()
                        && a.exit_code.is_none()
                        && a.reason.is_none(),
                    "invalid running attempt",
                )?;
            } else {
                ensure(a.finished_at.is_some(), "finished attempt lacks timestamp")?;
            }
            if a.status == AttemptStatus::Valid {
                ensure(
                    a.exit_code == Some(0)
                        && matches!(a.reason, None | Some(AttemptReason::DuplicateInput)),
                    "invalid successful attempt",
                )?;
                for h in [&a.exact_file_sha256, &a.canonical_run_sha256, &a.cohort_id] {
                    crate::v2::hash(
                        h.as_deref()
                            .ok_or_else(|| Error::new(4, "successful attempt lacks provenance"))?,
                    )?;
                }
            } else {
                ensure(
                    a.exact_file_sha256.is_none()
                        && a.canonical_run_sha256.is_none()
                        && a.cohort_id.is_none(),
                    "unsuccessful attempt carries validated provenance",
                )?;
                if a.status != AttemptStatus::Running {
                    ensure(a.reason.is_some(), "failed attempt lacks reason")?;
                }
            }
        }
        if self.status == Validity::Valid {
            ensure(
                p.finished
                    && p.attempts.len() == p.requested_runs as usize
                    && p.attempts
                        .iter()
                        .all(|a| a.status == AttemptStatus::Valid && a.reason.is_none()),
                "incomplete/failed/duplicate series declared valid",
            )?;
        }
        if p.finished {
            ensure(
                p.attempts
                    .iter()
                    .all(|a| a.status != AttemptStatus::Running)
                    && self.status != Validity::Incomplete,
                "finished series has pending state",
            )?;
        } else {
            ensure(
                self.status == Validity::Incomplete,
                "unfinished series requires incomplete status",
            )?;
        }
        Ok(())
    }
}
pub fn valid_child_path(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_control)
        && !value.contains(['\\', ':'])
        && !value.starts_with('/')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

//! Read-only preview contracts. No v2 benchmark engine or scoring is implemented.
use crate::{
    documents::{Capability, DocumentType, ReasonCode, Validity},
    Error, Result,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub const PREVIEW_CONTRACT: &str = "cpu-v2-preview-1";
pub const CORE_PROTOCOL: &str = "cpu-v2-preview/core-v1";
pub const GENERIC_BUILD_POLICY: &str = "rust-1.85.1-generic-v1";

/// Exact, canonical JSON decimal string; rejects floats, signs, overflow and leading zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Counter(pub u64);
impl Serialize for Counter {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for Counter {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if text.is_empty()
            || (text.len() > 1 && text.starts_with('0'))
            || !text.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(de::Error::custom(
                "expected canonical unsigned decimal string",
            ));
        }
        text.parse().map(Self).map_err(de::Error::custom)
    }
}
impl fmt::Display for Counter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildPolicy {
    pub policy_id: String,
    pub compiler_version: String,
    pub target: String,
    pub optimization_level: u8,
    pub lto: String,
    pub codegen_units: u32,
    pub panic: String,
    pub target_cpu: String,
    pub extra_target_features: Vec<String>,
    pub dependency_lock_sha256: String,
    pub binary_sha256: Option<String>,
    pub source_commit: Option<String>,
}
impl BuildPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.policy_id == GENERIC_BUILD_POLICY,
            "unknown build policy",
        )?;
        ensure(
            self.compiler_version == "rustc 1.85.1 (4eb161250 2025-03-15)",
            "compiler disagrees with build policy",
        )?;
        ensure(
            [
                "x86_64-unknown-linux-musl",
                "aarch64-unknown-linux-musl",
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
            ]
            .contains(&self.target.as_str()),
            "unsupported build target",
        )?;
        ensure(
            self.optimization_level == 3
                && self.lto == "thin"
                && self.codegen_units == 1
                && self.panic == "unwind"
                && self.target_cpu == "generic"
                && self.extra_target_features.is_empty(),
            "build flags disagree with generic policy",
        )?;
        hash(&self.dependency_lock_sha256)?;
        if let Some(value) = &self.binary_sha256 {
            hash(value)?;
        }
        if let Some(value) = &self.source_commit {
            ensure(
                value.len() == 40 && value.bytes().all(|c| c.is_ascii_hexdigit()),
                "invalid source commit",
            )?;
        }
        Ok(())
    }
    /// Architectures may differ; libc family and code-generation policy may not.
    pub fn comparison_identity(&self) -> Result<String> {
        self.validate()?;
        let family = if self.target.ends_with("musl") {
            "musl"
        } else {
            "gnu"
        };
        Ok(format!(
            "{}|{}|{}|{}",
            self.policy_id, self.compiler_version, family, self.dependency_lock_sha256
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Quota {
    Limited {
        quota_us: Counter,
        period_us: Counter,
    },
    Unlimited,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationSignature {
    pub allowed_cpu_ids: Vec<u32>,
    pub selected_cpu_ids: Vec<u32>,
    pub worker_count: u32,
    pub quota: Quota,
    pub visible_cpu_equivalents: f64,
    pub visibility_complete: bool,
    pub placement_policy: String,
    pub topology_confidence: String,
    pub buffer_ownership: String,
}
impl AllocationSignature {
    pub fn validate(&self) -> Result<()> {
        ensure(
            !self.allowed_cpu_ids.is_empty() && self.allowed_cpu_ids.len() <= 1_048_576,
            "empty or oversized allowed CPU set",
        )?;
        ensure(
            self.allowed_cpu_ids.windows(2).all(|w| w[0] < w[1])
                && self.allowed_cpu_ids.iter().all(|v| *v <= 1_048_576),
            "CPU IDs must be unique, ordered and bounded",
        )?;
        ensure(
            self.worker_count > 0 && self.worker_count as usize == self.selected_cpu_ids.len(),
            "worker placement count mismatch",
        )?;
        let selected: BTreeSet<_> = self.selected_cpu_ids.iter().collect();
        ensure(
            selected.len() == self.selected_cpu_ids.len()
                && self
                    .selected_cpu_ids
                    .iter()
                    .all(|v| self.allowed_cpu_ids.binary_search(v).is_ok()),
            "invalid selected CPU set",
        )?;
        let bound = match self.quota {
            Quota::Limited {
                quota_us,
                period_us,
            } => {
                ensure(
                    quota_us.0 > 0 && period_us.0 > 0,
                    "nonpositive quota rational",
                )?;
                quota_us.0 as f64 / period_us.0 as f64
            }
            _ => self.allowed_cpu_ids.len() as f64,
        };
        let capacity = bound.min(self.allowed_cpu_ids.len() as f64);
        ensure(
            self.visible_cpu_equivalents.is_finite()
                && self.visible_cpu_equivalents > 0.
                && near(self.visible_cpu_equivalents, capacity),
            "capacity disagrees with visible limits",
        )?;
        if let Quota::Limited {
            quota_us,
            period_us,
        } = self.quota
        {
            let ceiling = quota_us.0 / period_us.0 + u64::from(quota_us.0 % period_us.0 != 0);
            ensure(
                u64::from(self.worker_count)
                    == ceiling.min(self.allowed_cpu_ids.len() as u64).max(1),
                "worker count disagrees with quota ceiling",
            )?;
        } else {
            ensure(
                self.worker_count as usize == self.allowed_cpu_ids.len(),
                "worker count disagrees with visible allocation",
            )?;
        }
        ensure(
            self.placement_policy == "physical-first-v1"
                && self.buffer_ownership == "private-worker-first-touch-v1",
            "unknown placement/allocation ownership policy",
        )?;
        ensure(
            ["exposed", "unknown"].contains(&self.topology_confidence.as_str()),
            "unknown topology confidence",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadIdentity {
    pub workload_id: String,
    pub algorithm_version: u32,
    pub dataset_sha256: Option<String>,
    pub input_bytes: Counter,
    pub working_set_bytes: Counter,
    pub units_per_batch: Counter,
    pub unit: String,
    pub verification_policy: String,
    pub isa_backend: String,
}
impl WorkloadIdentity {
    pub fn validate(&self) -> Result<()> {
        let (unit, units) = crate::workload_contract(&self.workload_id)
            .ok_or_else(|| Error::new(4, "unknown preview workload"))?;
        ensure(
            self.algorithm_version == 1 && self.units_per_batch.0 == units && self.unit == unit,
            "workload identity/units disagree",
        )?;
        ensure(
            self.verification_policy == "in-batch-known-answer-v1"
                && self.isa_backend == "portable-software-v1",
            "unknown verification or ISA policy",
        )?;
        let corpus = matches!(
            self.workload_id.as_str(),
            "compression" | "decompression" | "hash" | "mixed"
        );
        ensure(
            if corpus {
                self.dataset_sha256.as_deref() == Some(crate::DATASET_SHA256)
            } else {
                self.dataset_sha256.is_none()
            },
            "dataset identity mismatch",
        )?;
        if matches!(
            self.workload_id.as_str(),
            "compression" | "decompression" | "hash"
        ) {
            ensure(
                self.input_bytes.0 == 65536,
                "input size disagrees with workload",
            )?;
        }
        ensure(
            self.working_set_bytes.0 >= self.input_bytes.0,
            "working set smaller than input",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolDurations {
    pub idle_ns: Counter,
    pub warmup_ns: Counter,
    pub workload_ns: Counter,
    pub sustained_ns: Counter,
    pub sample_ns: Counter,
}
impl ProtocolDurations {
    pub fn for_mode(mode: crate::Mode) -> Self {
        let d = mode.durations();
        Self {
            idle_ns: Counter(d.idle_seconds * 1_000_000_000),
            warmup_ns: Counter(d.warmup_seconds * 1_000_000_000),
            workload_ns: Counter(d.workload_seconds * 1_000_000_000),
            sustained_ns: Counter(d.sustained_seconds * 1_000_000_000),
            sample_ns: Counter(1_000_000_000),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProtocol {
    pub protocol_id: String,
    pub mode: crate::Mode,
    pub durations: ProtocolDurations,
    pub timer_policy: String,
    pub window_policy: String,
    pub placement_policy: String,
    pub instrumentation_mode: String,
    pub telemetry_policy: String,
}
impl ExecutionProtocol {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.protocol_id == CORE_PROTOCOL,
            "unknown execution protocol",
        )?;
        ensure(
            self.timer_policy == "monotonic-completion-v1"
                && self.window_policy == "worker-completion-1s-v1"
                && self.placement_policy == "physical-first-v1"
                && self.instrumentation_mode == "throughput"
                && self.telemetry_policy == "standard-v1",
            "execution policies disagree with core protocol",
        )?;
        let normal = ProtocolDurations::for_mode(self.mode);
        ensure(
            self.durations.idle_ns == normal.idle_ns
                && self.durations.warmup_ns == normal.warmup_ns
                && self.durations.workload_ns == normal.workload_ns
                && self.durations.sample_ns == normal.sample_ns
                && (10_000_000_000..=86_400_000_000_000).contains(&self.durations.sustained_ns.0)
                && self.durations.sustained_ns.0 % 1_000_000_000 == 0,
            "invalid protocol durations",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseRole {
    Warmup,
    Single,
    Multi,
    Sustained,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerResult {
    pub ordinal: u32,
    pub cpu_id: u32,
    pub status: Validity,
    pub completed_batches: Counter,
    pub completed_units: Counter,
    pub cpu_time_ns: Option<Counter>,
    pub start_offset_ns: Counter,
    pub finish_offset_ns: Counter,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThroughputWindow {
    pub start_ns: Counter,
    pub end_ns: Counter,
    pub per_worker_units: Vec<Counter>,
    pub completed_units: Counter,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSample {
    pub start_ns: Counter,
    pub end_ns: Counter,
    pub scope: crate::documents::MetricScope,
    pub cpu_utilization_percent: Option<f64>,
    pub steal_percent: Option<f64>,
    pub counters: BTreeMap<String, Counter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    pub phase_id: String,
    pub role: PhaseRole,
    pub status: Validity,
    pub reason_codes: Vec<ReasonCode>,
    pub workload: WorkloadIdentity,
    pub nominal_duration_ns: Counter,
    pub elapsed_ns: Counter,
    pub completed_batches: Counter,
    pub completed_units: Counter,
    pub total_throughput: f64,
    pub checksums_valid: bool,
    pub workers: Vec<WorkerResult>,
    pub throughput_windows: Vec<ThroughputWindow>,
    pub tail_units: Vec<Counter>,
    pub resource_samples: Vec<ResourceSample>,
}
impl Phase {
    pub fn validate(
        &self,
        allocation: &AllocationSignature,
        protocol: &ExecutionProtocol,
    ) -> Result<()> {
        token(&self.phase_id)?;
        self.workload.validate()?;
        ensure(
            self.nominal_duration_ns.0 > 0
                && self.nominal_duration_ns.0 <= 86_400_000_000_000
                && self.elapsed_ns.0 > 0,
            "invalid phase duration",
        )?;
        ensure(
            self.total_throughput.is_finite() && self.total_throughput >= 0.,
            "invalid throughput",
        )?;
        ensure(
            !self.workers.is_empty() && self.tail_units.len() == self.workers.len(),
            "missing worker or tail data",
        )?;
        let expected_workers = if self.role == PhaseRole::Single {
            1
        } else {
            allocation.worker_count as usize
        };
        ensure(
            self.workers.len() == expected_workers,
            "phase worker count mismatch",
        )?;
        let expected_ns = match self.role {
            PhaseRole::Warmup => protocol.durations.warmup_ns,
            PhaseRole::Sustained => protocol.durations.sustained_ns,
            _ => protocol.durations.workload_ns,
        };
        ensure(
            self.nominal_duration_ns == expected_ns,
            "phase duration disagrees with protocol",
        )?;
        if matches!(self.role, PhaseRole::Warmup | PhaseRole::Sustained) {
            ensure(
                self.workload.workload_id == "mixed",
                "mixed phase has wrong workload",
            )?;
        } else {
            ensure(
                self.workload.workload_id != "mixed",
                "core phase has mixed workload",
            )?;
        }
        let mut batches = 0u64;
        let mut units = 0u64;
        for (i, w) in self.workers.iter().enumerate() {
            ensure(
                w.ordinal as usize == i && allocation.selected_cpu_ids.get(i) == Some(&w.cpu_id),
                "worker ordinal or placement mismatch",
            )?;
            ensure(
                w.start_offset_ns <= w.finish_offset_ns && w.finish_offset_ns <= self.elapsed_ns,
                "worker timing outside phase",
            )?;
            ensure(
                w.status != Validity::Unavailable
                    && w.cpu_time_ns.is_none_or(|v| v <= self.elapsed_ns),
                "invalid worker status or CPU time",
            )?;
            ensure(
                w.completed_batches
                    .0
                    .checked_mul(self.workload.units_per_batch.0)
                    == Some(w.completed_units.0),
                "worker work counters disagree",
            )?;
            batches = add(batches, w.completed_batches.0)?;
            units = add(units, w.completed_units.0)?;
            if self.status == Validity::Valid {
                ensure(
                    w.status == Validity::Valid && w.finish_offset_ns >= self.nominal_duration_ns,
                    "failed or early-finished worker in valid phase",
                )?;
            }
        }
        ensure(
            batches == self.completed_batches.0 && units == self.completed_units.0,
            "phase counters disagree with workers",
        )?;
        ensure(
            near(
                self.total_throughput,
                units as f64 / (self.elapsed_ns.0 as f64 / 1e9),
            ),
            "total throughput disagrees with units/elapsed",
        )?;
        let mut cursor = 0u64;
        let mut per_worker = vec![0u64; self.workers.len()];
        for window in &self.throughput_windows {
            ensure(
                window.start_ns.0 == cursor
                    && window.end_ns.0.checked_sub(cursor) == Some(1_000_000_000)
                    && window.end_ns <= self.nominal_duration_ns
                    && window.end_ns <= self.elapsed_ns,
                "noncontiguous or invalid throughput window",
            )?;
            ensure(
                window.per_worker_units.len() == self.workers.len(),
                "window worker count mismatch",
            )?;
            let mut total = 0u64;
            for (i, value) in window.per_worker_units.iter().enumerate() {
                ensure(
                    value.0 % self.workload.units_per_batch.0 == 0,
                    "window contains fractional batch work",
                )?;
                if value.0 > 0 {
                    ensure(
                        self.workers[i].start_offset_ns < window.end_ns
                            && self.workers[i].finish_offset_ns >= window.start_ns,
                        "window work outside worker lifetime",
                    )?;
                }
                per_worker[i] = add(per_worker[i], value.0)?;
                total = add(total, value.0)?;
            }
            ensure(
                total == window.completed_units.0,
                "window aggregate mismatch",
            )?;
            cursor = window.end_ns.0;
        }
        for (i, w) in self.workers.iter().enumerate() {
            ensure(
                self.tail_units[i].0 % self.workload.units_per_batch.0 == 0,
                "tail contains fractional batch work",
            )?;
            ensure(
                add(per_worker[i], self.tail_units[i].0)? == w.completed_units.0,
                "window/tail units disagree with worker",
            )?;
        }
        if self.status == Validity::Valid {
            ensure(
                self.reason_codes.is_empty()
                    && self.checksums_valid
                    && batches > 0
                    && cursor == self.nominal_duration_ns.0
                    && self.elapsed_ns >= self.nominal_duration_ns,
                "incomplete/invalid phase declared valid",
            )?;
        } else {
            ensure(
                !self.reason_codes.is_empty(),
                "non-valid phase requires reasons",
            )?;
        }
        let mut previous = BTreeMap::new();
        for sample in &self.resource_samples {
            sample.scope.validate()?;
            if let crate::documents::MetricScope::Worker { ordinal } = sample.scope {
                ensure(
                    (ordinal as usize) < self.workers.len(),
                    "resource sample references absent worker",
                )?;
            }
            let cursor = previous.entry(sample.scope.clone()).or_insert(0u64);
            ensure(
                sample.start_ns.0 >= *cursor
                    && sample.end_ns > sample.start_ns
                    && sample.end_ns <= self.elapsed_ns,
                "invalid resource sample range",
            )?;
            for name in sample.counters.keys() {
                token(name)?;
            }
            for value in [sample.cpu_utilization_percent, sample.steal_percent]
                .into_iter()
                .flatten()
            {
                ensure(
                    value.is_finite() && (0. ..=100.).contains(&value),
                    "invalid resource percentage",
                )?;
            }
            *cursor = sample.end_ns.0;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkPayload {
    pub protocol: ExecutionProtocol,
    pub phases: Vec<Phase>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreStatus {
    Uncalibrated,
    Available,
    NotApplicable,
    Invalid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub label: Option<String>,
    pub provider: Option<String>,
    pub monthly_cost: Option<f64>,
    pub currency: Option<String>,
    pub anonymous: bool,
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
    pub reason_codes: Vec<ReasonCode>,
    pub timestamp: String,
    pub build: BuildPolicy,
    pub allocation: AllocationSignature,
    pub metadata: Metadata,
    pub capabilities: BTreeMap<String, Capability<serde_json::Value>>,
    pub payload: BenchmarkPayload,
    pub scores: serde_json::Value,
}
impl Document {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.schema_version == 2
                && self.document_type == DocumentType::Benchmark
                && self.benchmark_schema_version == 2
                && self.benchmark_contract_id == PREVIEW_CONTRACT,
            "unsupported preview document/contract",
        )?;
        token(&self.application_version)?;
        timestamp(&self.timestamp)?;
        ensure(
            self.score_model_id.is_none()
                && self.score_status == ScoreStatus::Uncalibrated
                && self.scores.is_null(),
            "preview results must remain uncalibrated and unscored",
        )?;
        self.build.validate()?;
        self.allocation.validate()?;
        self.payload.protocol.validate()?;
        ensure(
            self.protocol_id == self.payload.protocol.protocol_id,
            "envelope protocol mismatch",
        )?;
        if let Some(cost) = self.metadata.monthly_cost {
            ensure(
                cost.is_finite() && cost > 0. && self.metadata.currency.is_some(),
                "invalid price metadata",
            )?;
        }
        for (name, capability) in &self.capabilities {
            token(name)?;
            capability.validate()?;
            let scope = match capability {
                Capability::Available { scope, .. } | Capability::Unavailable { scope, .. } => {
                    scope
                }
            };
            if let crate::documents::MetricScope::Worker { ordinal } = scope {
                ensure(
                    *ordinal < self.allocation.worker_count,
                    "capability references absent worker",
                )?;
            }
        }
        let mut schedule = vec![(PhaseRole::Warmup, "mixed")];
        for role in [PhaseRole::Single, PhaseRole::Multi] {
            for name in crate::WORKLOADS {
                schedule.push((role, name));
            }
        }
        schedule.push((PhaseRole::Sustained, "mixed"));
        ensure(
            self.payload.phases.len() <= schedule.len(),
            "too many core phases",
        )?;
        let mut ids = BTreeSet::new();
        let mut identities = BTreeMap::new();
        for (index, phase) in self.payload.phases.iter().enumerate() {
            ensure(
                (phase.role, phase.workload.workload_id.as_str()) == schedule[index],
                "phase order disagrees with core protocol",
            )?;
            ensure(ids.insert(&phase.phase_id), "duplicate phase ID")?;
            phase.validate(&self.allocation, &self.payload.protocol)?;
            if let Some(previous) = identities.insert(&phase.workload.workload_id, &phase.workload)
            {
                ensure(
                    previous == &phase.workload,
                    "workload identity changes between phases",
                )?;
            }
        }
        if self.status == Validity::Valid {
            ensure(
                self.reason_codes.is_empty()
                    && self
                        .payload
                        .phases
                        .iter()
                        .all(|p| p.status == Validity::Valid),
                "valid document contains invalid phases/reasons",
            )?;
            ensure(
                self.payload.phases.len() == schedule.len(),
                "complete preview is missing required phases",
            )?;
        } else {
            ensure(
                self.status != Validity::Unavailable && !self.reason_codes.is_empty(),
                "invalid document status/reasons",
            )?;
        }
        Ok(())
    }
}

pub(crate) fn ensure(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::new(4, message))
    }
}
pub(crate) fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| Error::new(4, "counter overflow"))
}
pub(crate) fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-8 * b.abs().max(1.)
}
pub(crate) fn hash(value: &str) -> Result<()> {
    ensure(
        value.len() == 64
            && value
                .bytes()
                .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase()),
        "invalid canonical SHA-256",
    )
}
pub(crate) fn token(value: &str) -> Result<()> {
    ensure(
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || b"-_.:/+ ()".contains(&v))
            && !value.contains('\\')
            && !value.starts_with('/'),
        "invalid portable identity token",
    )
}
pub(crate) fn timestamp(value: &str) -> Result<()> {
    let b = value.as_bytes();
    ensure(
        b.len() >= 20
            && b.is_ascii()
            && b[4] == b'-'
            && b[7] == b'-'
            && b[10] == b'T'
            && b[13] == b':'
            && b[16] == b':'
            && b.last() == Some(&b'Z'),
        "invalid UTC timestamp",
    )?;
    for (start, end) in [(0, 4), (5, 7), (8, 10), (11, 13), (14, 16), (17, 19)] {
        ensure(
            b[start..end].iter().all(u8::is_ascii_digit),
            "invalid UTC timestamp digits",
        )?;
    }
    let n = |a: usize, z: usize| value[a..z].parse::<u32>().ok();
    let (Some(y), Some(m), Some(d), Some(h), Some(min), Some(s)) =
        (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19))
    else {
        return Err(Error::new(4, "invalid UTC timestamp fields"));
    };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    ensure(
        d >= 1 && d <= days && h < 24 && min < 60 && s < 60,
        "invalid UTC calendar value",
    )?;
    ensure(
        b.len() == 20
            || (b.len() >= 22
                && b.len() <= 30
                && b[19] == b'.'
                && b[20..b.len() - 1].iter().all(u8::is_ascii_digit)),
        "invalid UTC timestamp precision",
    )
}

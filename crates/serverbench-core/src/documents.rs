//! Explicit read-only format dispatch. Legacy execution and score models stay separate.
use crate::{v2, BenchmarkResult, Durations, Error, Mode, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Benchmark,
    RepeatSeries,
    ScalingStudy,
    LatencyStudy,
    ComparisonReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Validity {
    Valid,
    Incomplete,
    Invalid,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    UnsupportedSchema,
    UnsupportedDocumentType,
    UnsupportedContract,
    UnsupportedProtocol,
    UnsupportedScoreModel,
    UnsupportedBuildPolicy,
    NotImplemented,
    NotSupported,
    NotVisible,
    PermissionDenied,
    Malformed,
    NotApplicable,
    CounterReset,
    IncompleteInterval,
    Interrupted,
    WorkerFailed,
    ChecksumFailed,
    AllocationChanged,
    InsufficientResources,
}

/// Logical scopes only: no host paths or serialized process/thread IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MetricScope {
    SystemVisible,
    AllowedCpuSet,
    Process,
    Worker { ordinal: u32 },
    Cgroup { ancestor: u32 },
}
impl MetricScope {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Worker { ordinal } => {
                v2::ensure(*ordinal < 1_048_576, "worker scope out of bounds")
            }
            Self::Cgroup { ancestor } => {
                v2::ensure(*ancestor < 1024, "ancestor scope out of bounds")
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "snake_case", deny_unknown_fields)]
pub enum Capability<T> {
    Available {
        source: String,
        scope: MetricScope,
        value: T,
    },
    Unavailable {
        source: String,
        scope: MetricScope,
        reason: ReasonCode,
    },
}
impl Capability<Value> {
    pub fn validate(&self) -> Result<()> {
        let (source, scope) = match self {
            Self::Available {
                source,
                scope,
                value,
            } => {
                v2::ensure(
                    !value.is_null(),
                    "available capability must contain a value",
                )?;
                (source, scope)
            }
            Self::Unavailable {
                source,
                scope,
                reason,
            } => {
                v2::ensure(
                    matches!(
                        reason,
                        ReasonCode::NotSupported
                            | ReasonCode::NotVisible
                            | ReasonCode::PermissionDenied
                            | ReasonCode::Malformed
                            | ReasonCode::NotApplicable
                    ),
                    "invalid capability availability reason",
                )?;
                (source, scope)
            }
        };
        v2::token(source)?;
        v2::ensure(
            !source.contains('/') && !source.contains(':') && !source.contains(' '),
            "capability source must be a logical ID, not a path",
        )?;
        scope.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentHeader {
    pub schema_version: u32,
    pub document_type: Option<String>,
    pub benchmark_schema_version: Option<u32>,
    pub benchmark_contract_id: Option<String>,
    pub protocol_id: Option<String>,
    pub score_model_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedDocument {
    pub header: DocumentHeader,
    pub reason: ReasonCode,
}

#[derive(Debug, Clone)]
pub enum StoredDocument {
    LegacyBenchmarkV1(Box<BenchmarkResult>),
    PreviewBenchmarkV2(Box<v2::Document>),
    RepeatSeries(Box<crate::series::Document>),
}
#[derive(Debug, Clone)]
pub enum DecodeOutcome {
    Supported(StoredDocument),
    Unsupported(UnsupportedDocument),
}

/// Decode by header before touching a payload. This never falls back to v1.
/// V1 score validation belongs to the score registry (which depends on core).
pub fn decode_document(bytes: &[u8]) -> Result<DecodeOutcome> {
    let value = strict_json(bytes)?;
    let obj = value
        .as_object()
        .ok_or_else(|| Error::new(4, "document must be a JSON object"))?;
    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| Error::new(4, "missing or invalid schema_version"))?;
    let text = |key: &str| -> Result<Option<String>> {
        match obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            _ => Err(Error::new(4, format!("invalid header field {key}"))),
        }
    };
    let benchmark_schema_version = obj
        .get("benchmark_schema_version")
        .map(|v| {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| Error::new(4, "invalid benchmark_schema_version"))
        })
        .transpose()?;
    let header = DocumentHeader {
        schema_version,
        document_type: text("document_type")?,
        benchmark_schema_version,
        benchmark_contract_id: text(if schema_version == 1 {
            "contract"
        } else {
            "benchmark_contract_id"
        })?,
        protocol_id: text("protocol_id")?,
        score_model_id: text("score_model_id")?,
    };
    let unsupported = |reason| {
        Ok(DecodeOutcome::Unsupported(UnsupportedDocument {
            header: header.clone(),
            reason,
        }))
    };
    if ![1, 2].contains(&schema_version) {
        return unsupported(ReasonCode::UnsupportedSchema);
    }
    let kind = header.document_type.as_deref();
    if schema_version == 1 {
        if !matches!(kind, None | Some("benchmark")) {
            return unsupported(ReasonCode::UnsupportedDocumentType);
        }
        // A null/empty discriminator is not the historical absence of a discriminator.
        if obj.contains_key("document_type") && kind.is_none() {
            return Err(Error::new(4, "null document_type"));
        }
        if header.benchmark_contract_id.as_deref() != Some(crate::CONTRACT)
            || benchmark_schema_version != Some(crate::BENCHMARK_SCHEMA)
            || text("benchmark_version")?.as_deref() != Some(crate::BENCHMARK_VERSION)
        {
            return unsupported(ReasonCode::UnsupportedContract);
        }
        if header.score_model_id.is_some() {
            return unsupported(ReasonCode::UnsupportedScoreModel);
        }
        if header.protocol_id.is_some() {
            return unsupported(ReasonCode::UnsupportedProtocol);
        }
        return Ok(DecodeOutcome::Supported(StoredDocument::LegacyBenchmarkV1(
            Box::new(serde_json::from_value(value)?),
        )));
    }
    let kind: DocumentType = match kind {
        None => return Err(Error::new(4, "schema 2 requires document_type")),
        Some(kind) => match serde_json::from_value(Value::String(kind.into())) {
            Ok(v) => v,
            Err(_) => return unsupported(ReasonCode::UnsupportedDocumentType),
        },
    };
    if kind == DocumentType::RepeatSeries {
        if header.benchmark_contract_id.as_deref() != Some(crate::CONTRACT)
            || benchmark_schema_version != Some(crate::BENCHMARK_SCHEMA)
        {
            return unsupported(ReasonCode::UnsupportedContract);
        }
        if header.protocol_id.as_deref() != Some(crate::series::PROTOCOL) {
            return unsupported(ReasonCode::UnsupportedProtocol);
        }
        if header.score_model_id.is_some() {
            return unsupported(ReasonCode::UnsupportedScoreModel);
        }
        for field in ["score_model_id", "scores"] {
            v2::ensure(
                obj.contains_key(field),
                "series requires explicit nullable score fields",
            )?;
        }
        let series: crate::series::Document = serde_json::from_value(value)?;
        series.validate()?;
        return Ok(DecodeOutcome::Supported(StoredDocument::RepeatSeries(
            Box::new(series),
        )));
    }
    if kind != DocumentType::Benchmark {
        return unsupported(ReasonCode::NotImplemented);
    }
    if header.benchmark_contract_id.as_deref() != Some(v2::PREVIEW_CONTRACT)
        || benchmark_schema_version != Some(2)
    {
        return unsupported(ReasonCode::UnsupportedContract);
    }
    if header.protocol_id.as_deref() != Some(v2::CORE_PROTOCOL) {
        return unsupported(ReasonCode::UnsupportedProtocol);
    }
    if header.score_model_id.is_some() {
        return unsupported(ReasonCode::UnsupportedScoreModel);
    }
    if let Some(policy) = value.pointer("/build/policy_id").and_then(Value::as_str) {
        if policy != v2::GENERIC_BUILD_POLICY {
            return unsupported(ReasonCode::UnsupportedBuildPolicy);
        }
    }
    // Nullable is intentional, not optional: an omitted score must not look calibrated.
    for field in ["score_model_id", "scores"] {
        v2::ensure(
            obj.contains_key(field),
            "schema 2 requires explicit nullable score fields",
        )?;
    }
    let document: v2::Document = serde_json::from_value(value)?;
    document.validate()?;
    Ok(DecodeOutcome::Supported(
        StoredDocument::PreviewBenchmarkV2(Box::new(document)),
    ))
}

/// Versioned normalization identity; future calibrations must have a new identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationIdentity {
    pub contract: String,
    pub reference_sha256: String,
    pub index: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyComparisonKey {
    pub schema_version: u32,
    pub benchmark_schema_version: u32,
    pub benchmark_major: String,
    pub mode: Mode,
    pub durations: Durations,
    pub contract: String,
    pub normalization: NormalizationIdentity,
    pub dataset_sha256: String,
    pub compiler_version: String,
    pub optimized_build: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewComparisonKey {
    pub contract: String,
    pub protocol: v2::ExecutionProtocol,
    pub build_policy: String,
    pub workloads: Vec<v2::WorkloadIdentity>,
    pub normalization: Option<NormalizationIdentity>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "family", content = "key", rename_all = "snake_case")]
pub enum ComparisonKey {
    LegacyV1(LegacyComparisonKey),
    PreviewV2(PreviewComparisonKey),
}
impl ComparisonKey {
    pub fn legacy(r: &BenchmarkResult) -> Self {
        Self::LegacyV1(LegacyComparisonKey {
            schema_version: r.schema_version,
            benchmark_schema_version: r.benchmark_schema_version,
            benchmark_major: r.benchmark_version.split('.').next().unwrap_or("").into(),
            mode: r.benchmark_mode,
            durations: r.durations.clone(),
            contract: r.contract.clone(),
            normalization: NormalizationIdentity {
                contract: r.contract.clone(),
                reference_sha256: r.reference_sha256.clone(),
                index: 1000,
            },
            dataset_sha256: r.dataset_sha256.clone(),
            compiler_version: r.compiler_version.clone(),
            optimized_build: r.optimized_build,
        })
    }
    pub fn from_document(document: &StoredDocument) -> Result<Self> {
        match document {
            StoredDocument::RepeatSeries(_) => Err(Error::new(
                4,
                "series has cohort comparison keys, not one benchmark key",
            )),
            StoredDocument::LegacyBenchmarkV1(r) => Ok(Self::legacy(r)),
            StoredDocument::PreviewBenchmarkV2(r) => {
                r.validate()?;
                let mut workloads: BTreeMap<String, v2::WorkloadIdentity> = BTreeMap::new();
                for phase in &r.payload.phases {
                    if let Some(previous) =
                        workloads.insert(phase.workload.workload_id.clone(), phase.workload.clone())
                    {
                        v2::ensure(
                            previous == phase.workload,
                            "workload identity changes between phases",
                        )?;
                    }
                }
                Ok(Self::PreviewV2(PreviewComparisonKey {
                    contract: r.benchmark_contract_id.clone(),
                    protocol: r.payload.protocol.clone(),
                    build_policy: r.build.comparison_identity()?,
                    workloads: workloads.into_values().collect(),
                    normalization: None,
                }))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprints {
    pub exact_file_sha256: String,
    pub canonical_run_sha256: String,
}
impl Fingerprints {
    /// Fingerprint v1 of a *validated* typed document; not proof of authenticity.
    pub fn new(bytes: &[u8], document: &StoredDocument) -> Result<Self> {
        let mut value = match document {
            StoredDocument::RepeatSeries(_) => {
                return Err(Error::new(4, "series is not an individual run"))
            }
            StoredDocument::LegacyBenchmarkV1(r) => serde_json::to_value(r)?,
            StoredDocument::PreviewBenchmarkV2(r) => serde_json::to_value(r)?,
        };
        let obj = value.as_object_mut().expect("document object");
        for key in [
            "metadata",
            "application_version",
            "warnings",
            "scores",
            "price_performance",
        ] {
            obj.remove(key);
        }
        // Value maps are ordered (serde_json preserve_order is deliberately disabled).
        let canonical = serde_json::to_vec(&value)?;
        let mut hash = Sha256::new();
        hash.update(b"serverbench:canonical-run:v1\0");
        hash.update(canonical);
        Ok(Self {
            exact_file_sha256: format!("{:x}", Sha256::digest(bytes)),
            canonical_run_sha256: format!("{:x}", hash.finalize()),
        })
    }
}

// Reject ambiguous duplicate keys at every depth, including the dispatch header.
fn strict_json(bytes: &[u8]) -> Result<Value> {
    struct Unique(Value);
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("JSON without duplicate object keys")
                }
                fn visit_map<M: serde::de::MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> std::result::Result<Unique, M::Error> {
                    let mut object = serde_json::Map::new();
                    while let Some((key, Unique(value))) = map.next_entry::<String, Unique>()? {
                        if object.insert(key, value).is_some() {
                            return Err(serde::de::Error::custom("duplicate JSON key"));
                        }
                    }
                    Ok(Unique(Value::Object(object)))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> std::result::Result<Unique, A::Error> {
                    let mut values = Vec::new();
                    while let Some(Unique(v)) = seq.next_element()? {
                        values.push(v);
                    }
                    Ok(Unique(Value::Array(values)))
                }
                fn visit_bool<E: serde::de::Error>(
                    self,
                    v: bool,
                ) -> std::result::Result<Unique, E> {
                    Ok(Unique(Value::Bool(v)))
                }
                fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                    Ok(Unique(Value::from(v)))
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                    Ok(Unique(Value::from(v)))
                }
                fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                    serde_json::Number::from_f64(v)
                        .map(|n| Unique(Value::Number(n)))
                        .ok_or_else(|| E::custom("nonfinite JSON number"))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                    Ok(Unique(Value::String(v.into())))
                }
                fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Unique, E> {
                    Ok(Unique(Value::Null))
                }
                fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Unique, E> {
                    Ok(Unique(Value::Null))
                }
            }
            d.deserialize_any(Visitor)
        }
    }
    Ok(serde_json::from_slice::<Unique>(bytes)?.0)
}

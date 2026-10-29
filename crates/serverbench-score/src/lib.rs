use serde::Deserialize;
use serverbench_core::{
    statistics, BenchmarkResult, ConsistencyInputs, Error, Measurement, Result, Scores, WORKLOADS,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const REFERENCE: &str = include_str!("../../../reference/cpu-v1.json");
pub const REFERENCE_SHA256: &str =
    "8b4fdb4d2998f820e85799e7a1e2151e7fc6213fd228f6e2920aacd9722cda05";
pub const WEIGHTS: [f64; 3] = [0.35, 0.40, 0.25];

/// Registry dispatch is deliberately separate from the frozen v1 arithmetic.
#[derive(Debug, Clone)]
pub enum ScoreDisposition {
    LegacyV1(Scores),
    Uncalibrated,
    NotApplicable,
}
pub fn validate_document(
    document: &serverbench_core::documents::StoredDocument,
) -> Result<ScoreDisposition> {
    use serverbench_core::documents::StoredDocument;
    match document {
        StoredDocument::RepeatSeries(document) => {
            document.validate()?;
            Ok(ScoreDisposition::NotApplicable)
        }
        StoredDocument::LegacyBenchmarkV1(result) => {
            validate_result(result)?;
            Ok(ScoreDisposition::LegacyV1(result.scores.clone()))
        }
        StoredDocument::PreviewBenchmarkV2(result) => {
            result.validate()?;
            Ok(ScoreDisposition::Uncalibrated)
        }
    }
}
#[derive(Deserialize)]
pub struct Reference {
    pub benchmark_schema_version: u32,
    pub contract: String,
    pub status: String,
    pub index: f64,
    pub throughput: BTreeMap<String, f64>,
    pub mixed_cycles_per_second: f64,
}
pub fn reference_hash() -> String {
    format!("{:x}", Sha256::digest(REFERENCE.as_bytes()))
}
pub fn reference() -> Result<Reference> {
    if reference_hash() != REFERENCE_SHA256 {
        return Err(Error::new(
            4,
            "reference checksum mismatch; scoring contract changed",
        ));
    }
    let r: Reference = serde_json::from_str(REFERENCE)?;
    if r.benchmark_schema_version != serverbench_core::BENCHMARK_SCHEMA
        || r.contract != serverbench_core::CONTRACT
        || r.index != 1000.
        || r.throughput.len() != WORKLOADS.len()
        || !r.mixed_cycles_per_second.is_finite()
        || r.mixed_cycles_per_second <= 0.
        || WORKLOADS.iter().any(|k| {
            !r.throughput
                .get(*k)
                .is_some_and(|n| n.is_finite() && *n > 0.)
        })
    {
        return Err(Error::new(4, "invalid reference data"));
    }
    Ok(r)
}
pub fn geometric_mean(values: &[(f64, f64)]) -> Result<f64> {
    if values.is_empty()
        || values
            .iter()
            .any(|(v, w)| !v.is_finite() || *v < 0. || !w.is_finite() || *w < 0.)
    {
        return Err(Error::new(4, "negative or non-finite geometric mean input"));
    }
    let weight = values.iter().map(|(_, w)| w).sum::<f64>();
    if weight <= 0. {
        return Err(Error::new(4, "zero geometric mean weight"));
    }
    if values.iter().any(|(v, w)| *v == 0. && *w > 0.) {
        return Ok(0.);
    }
    let result = (values
        .iter()
        .filter(|(_, w)| *w > 0.)
        .map(|(v, w)| v.ln() * w)
        .sum::<f64>()
        / weight)
        .exp();
    if !result.is_finite() {
        return Err(Error::new(4, "score overflow"));
    }
    Ok(result)
}
pub fn cpu_index(single: f64, multi: f64, reliable: f64) -> Result<f64> {
    geometric_mean(&[
        (single, WEIGHTS[0]),
        (multi, WEIGHTS[1]),
        (reliable, WEIGHTS[2]),
    ])
}
pub fn consistency(i: &ConsistencyInputs) -> f64 {
    // Frozen experimental contract: all penalties are dimensionless.
    let penalty = 0.35 * (i.coefficient_of_variation / 0.5).clamp(0., 1.)
        + 0.25 * (1. - i.p10_to_median).clamp(0., 1.)
        + 0.20 * i.degradation_fraction.clamp(0., 1.)
        + 0.10 * (i.mean_steal_percent.unwrap_or(0.) / 20.).clamp(0., 1.)
        + 0.10 * i.outlier_fraction.clamp(0., 1.);
    (100. * (1. - penalty)).clamp(0., 100.)
}
pub fn consistency_label(score: f64) -> &'static str {
    if score >= 97. {
        "excellent"
    } else if score >= 90. {
        "very good"
    } else if score >= 80. {
        "good"
    } else if score >= 70. {
        "variable"
    } else {
        "inconsistent"
    }
}
fn workload_score(measurements: &BTreeMap<String, Measurement>, r: &Reference) -> Result<f64> {
    if measurements.len() != WORKLOADS.len() {
        return Err(Error::new(4, "missing workload measurements"));
    }
    let values = WORKLOADS
        .iter()
        .map(|k| {
            let m = measurements
                .get(*k)
                .ok_or_else(|| Error::new(4, "missing workload"))?;
            if !m.checksums_valid || m.throughput.count == 0 {
                return Err(Error::new(4, "invalid workload results"));
            }
            Ok((m.throughput.median / r.throughput[*k], 1.))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(r.index * geometric_mean(&values)?)
}
pub fn calculate(
    single: &BTreeMap<String, Measurement>,
    multi: &BTreeMap<String, Measurement>,
    sustained: &Measurement,
    sample_seconds: u64,
) -> Result<Scores> {
    let r = reference()?;
    let s = workload_score(single, &r)?;
    let m = workload_score(multi, &r)?;
    let t = &sustained.throughput;
    if !sustained.checksums_valid || t.count < 3 || t.median < 0. || t.p10 < 0. {
        return Err(Error::new(
            4,
            "insufficient sustained work for a valid score",
        ));
    }
    let rates: Vec<_> = sustained
        .samples
        .iter()
        .filter(|s| s.interval_seconds >= sample_seconds as f64 * 0.9)
        .filter_map(|s| s.throughput)
        .collect();
    let third = (rates.len() / 3).max(1);
    let first = statistics(&rates[..third]).median;
    let last = statistics(&rates[rates.len() - third..]).median;
    let steal: Vec<_> = sustained
        .samples
        .iter()
        .filter_map(|s| s.steal_percent)
        .collect();
    let inputs = ConsistencyInputs {
        coefficient_of_variation: t.coefficient_of_variation,
        p10_to_median: if t.median > 0. { t.p10 / t.median } else { 0. },
        degradation_fraction: if first > 0. {
            (1. - last / first).max(0.)
        } else {
            0.
        },
        mean_steal_percent: (!steal.is_empty()).then(|| statistics(&steal).average),
        outlier_fraction: rates.iter().filter(|v| **v < t.median * 0.75).count() as f64
            / rates.len() as f64,
    };
    let c = consistency(&inputs);
    let reliable = r.index * t.p10 / r.mixed_cycles_per_second;
    Ok(Scores {
        single: s,
        multi: m,
        per_vcpu: m / sustained.workers as f64,
        average: r.index * t.average / r.mixed_cycles_per_second,
        reliable,
        cpu_index: cpu_index(s, m, reliable)?,
        consistency: c,
        consistency_label: consistency_label(c).into(),
        consistency_inputs: inputs,
    })
}

pub fn validate_result(r: &BenchmarkResult) -> Result<()> {
    if r.schema_version != serverbench_core::SCHEMA_VERSION
        || r.benchmark_schema_version != serverbench_core::BENCHMARK_SCHEMA
        || r.contract != serverbench_core::CONTRACT
        || r.reference_sha256 != reference_hash()
        || r.baseline_status != "experimental"
        || r.dataset_sha256 != serverbench_core::DATASET_SHA256
        || r.benchmark_version != serverbench_core::BENCHMARK_VERSION
    {
        return Err(Error::new(4, "unsupported or inconsistent result contract"));
    }
    for (expected, map) in [(1, &r.single), (r.system.cpu.usable_cpus, &r.multi)] {
        if map.len() != WORKLOADS.len() {
            return Err(Error::new(4, "unexpected workload set"));
        }
        for name in WORKLOADS {
            let m = map
                .get(name)
                .ok_or_else(|| Error::new(4, "missing result workload"))?;
            if m.workers != expected || m.workload != name {
                return Err(Error::new(4, "invalid worker count or workload identity"));
            }
            validate_measurement(m, r.durations.sample_seconds)?;
        }
    }
    validate_measurement(&r.sustained, r.durations.sample_seconds)?;
    if r.sustained.workers != r.system.cpu.usable_cpus || r.sustained.workload != "mixed" {
        return Err(Error::new(4, "invalid sustained workload"));
    }
    let expected = calculate(
        &r.single,
        &r.multi,
        &r.sustained,
        r.durations.sample_seconds,
    )?;
    for (a, b) in [
        (r.scores.single, expected.single),
        (r.scores.multi, expected.multi),
        (r.scores.per_vcpu, expected.per_vcpu),
        (r.scores.average, expected.average),
        (r.scores.reliable, expected.reliable),
        (r.scores.cpu_index, expected.cpu_index),
        (r.scores.consistency, expected.consistency),
    ] {
        if !near(a, b) {
            return Err(Error::new(
                4,
                "stored scores disagree with raw measurements",
            ));
        }
    }
    if r.scores.consistency_label != expected.consistency_label {
        return Err(Error::new(4, "inconsistent classification"));
    }
    Ok(())
}
fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-8 * b.abs().max(1.)
}
fn validate_measurement(m: &Measurement, sample_seconds: u64) -> Result<()> {
    let (unit, batch_units) = serverbench_core::workload_contract(&m.workload)
        .ok_or_else(|| Error::new(4, "unknown workload"))?;
    if !m.checksums_valid
        || m.workers == 0
        || m.completed_batches == 0
        || m.elapsed_seconds <= 0.
        || !m.elapsed_seconds.is_finite()
        || m.samples.is_empty()
        || m.affinity_applied.len() != m.workers
        || sample_seconds == 0
        || m.unit != unit
        || m.completed_batches.checked_mul(batch_units) != Some(m.completed_units)
        || !near(m.throughput_per_vcpu, m.total_throughput / m.workers as f64)
    {
        return Err(Error::new(4, "invalid benchmark result"));
    }
    let mut previous = 0.;
    for s in &m.samples {
        if !s.elapsed_seconds.is_finite()
            || s.elapsed_seconds <= previous
            || !s.interval_seconds.is_finite()
            || s.interval_seconds <= 0.
            || s.throughput.is_none_or(|v| !v.is_finite() || v < 0.)
            || [s.cpu_utilization_percent, s.steal_percent]
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0. ..=100.).contains(v))
        {
            return Err(Error::new(4, "invalid raw measurement sample"));
        }
        previous = s.elapsed_seconds;
    }
    let rates: Vec<_> = m
        .samples
        .iter()
        .filter(|s| s.interval_seconds >= sample_seconds as f64 * 0.9)
        .filter_map(|s| s.throughput)
        .collect();
    if rates.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err(Error::new(4, "invalid sample throughput"));
    }
    let expected = statistics(&rates);
    let a = &m.throughput;
    if !statistics_match(a, &expected)
        || !near(
            m.total_throughput,
            m.completed_units as f64 / m.elapsed_seconds,
        )
    {
        return Err(Error::new(4, "inconsistent measurement statistics"));
    }
    Ok(())
}
fn statistics_match(a: &serverbench_core::Statistics, b: &serverbench_core::Statistics) -> bool {
    a.count == b.count
        && [
            (a.average, b.average),
            (a.median, b.median),
            (a.p10, b.p10),
            (a.p50, b.p50),
            (a.p90, b.p90),
            (a.p95, b.p95),
            (a.p99, b.p99),
            (a.minimum, b.minimum),
            (a.maximum, b.maximum),
            (a.standard_deviation, b.standard_deviation),
            (a.coefficient_of_variation, b.coefficient_of_variation),
        ]
        .iter()
        .all(|(a, b)| near(*a, *b))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_is_pinned() {
        assert_eq!(reference_hash(), REFERENCE_SHA256);
        reference().unwrap();
    }
    #[test]
    fn means_and_index() {
        assert!((geometric_mean(&[(4., 1.), (16., 1.)]).unwrap() - 8.).abs() < 1e-12);
        assert!((cpu_index(1000., 1000., 1000.).unwrap() - 1000.).abs() < 1e-8);
        let expected = 1000_f64.powf(0.35) * 2000_f64.powf(0.40) * 500_f64.powf(0.25);
        assert!((cpu_index(1000., 2000., 500.).unwrap() - expected).abs() < 1e-8);
        assert_eq!(cpu_index(0., 1000., 1000.).unwrap(), 0.);
        assert!(cpu_index(-1., 1000., 1000.).is_err());
        assert!(geometric_mean(&[(f64::NAN, 1.)]).is_err());
    }
    #[test]
    fn unstable_is_penalized() {
        let mut i = ConsistencyInputs {
            coefficient_of_variation: 0.,
            p10_to_median: 1.,
            degradation_fraction: 0.,
            mean_steal_percent: None,
            outlier_fraction: 0.,
        };
        assert_eq!(consistency(&i), 100.);
        i.coefficient_of_variation = 0.5;
        i.p10_to_median = 0.4;
        i.degradation_fraction = 0.5;
        i.mean_steal_percent = Some(20.);
        i.outlier_fraction = 0.4;
        assert!(consistency(&i) < 40.);
        assert_eq!(consistency_label(94.), "very good");
    }
    #[test]
    fn consistency_golden_formula() {
        let i = ConsistencyInputs {
            coefficient_of_variation: 0.1,
            p10_to_median: 0.8,
            degradation_fraction: 0.1,
            mean_steal_percent: Some(2.),
            outlier_fraction: 0.1,
        };
        assert!((consistency(&i) - 84.).abs() < 1e-12);
        assert_eq!(consistency_label(97.), "excellent");
        assert_eq!(consistency_label(90.), "very good");
        assert_eq!(consistency_label(80.), "good");
        assert_eq!(consistency_label(70.), "variable");
        assert_eq!(consistency_label(69.), "inconsistent");
    }
}

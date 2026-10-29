use serverbench_core::{
    documents::{ComparisonKey, Fingerprints},
    series::{CohortSummary, LegacyAllocation, RunStatistics},
    statistics, BenchmarkResult, Error, Result, WORKLOADS,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub struct Observation<'a> {
    pub result: &'a BenchmarkResult,
    pub fingerprints: &'a Fingerprints,
}
#[derive(Clone)]
pub struct RunRecord {
    pub cohort_id: String,
    pub comparison_key: ComparisonKey,
    pub allocation: LegacyAllocation,
    pub fingerprint: String,
    pub metrics: BTreeMap<String, f64>,
    pub single: BTreeMap<String, f64>,
    pub multi: BTreeMap<String, f64>,
}
impl RunRecord {
    pub fn new(o: &Observation<'_>) -> Result<Self> {
        Ok(Self {
            cohort_id: cohort_id(o.result)?,
            comparison_key: ComparisonKey::legacy(o.result),
            allocation: LegacyAllocation::from(o.result),
            fingerprint: o.fingerprints.canonical_run_sha256.clone(),
            metrics: [
                "cpu_index",
                "single",
                "multi",
                "per_vcpu",
                "average",
                "reliable",
                "consistency",
            ]
            .into_iter()
            .map(|n| (n.into(), metric(o.result, n)))
            .collect(),
            single: WORKLOADS
                .into_iter()
                .map(|n| (n.into(), o.result.single[n].throughput.median))
                .collect(),
            multi: WORKLOADS
                .into_iter()
                .map(|n| (n.into(), o.result.multi[n].throughput.median))
                .collect(),
        })
    }
}
pub fn cohort_id(result: &BenchmarkResult) -> Result<String> {
    let identity = serde_json::to_vec(&(
        ComparisonKey::legacy(result),
        LegacyAllocation::from(result),
    ))?;
    Ok(format!("{:x}", Sha256::digest(identity)))
}
pub fn describe(values: &[f64]) -> Result<RunStatistics> {
    if values.is_empty()
        || values.len() > u32::MAX as usize
        || values.iter().any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(Error::new(
            4,
            "invalid run-level values (expected finite nonnegative observations)",
        ));
    }
    let s = statistics(values);
    let mad = statistics(
        &values
            .iter()
            .map(|v| (v - s.median).abs())
            .collect::<Vec<_>>(),
    )
    .median;
    if !s.average.is_finite() || !s.standard_deviation.is_finite() {
        return Err(Error::new(4, "run statistics overflow"));
    }
    Ok(RunStatistics {
        count: values.len() as u32,
        median: s.median,
        minimum: s.minimum,
        maximum: s.maximum,
        mean: s.average,
        standard_deviation: s.standard_deviation,
        coefficient_of_variation: (s.average > 0.).then_some(s.coefficient_of_variation),
        median_absolute_deviation: mad,
    })
}
pub fn summarize(observations: &[Observation<'_>]) -> Result<Vec<CohortSummary>> {
    summarize_records(
        &observations
            .iter()
            .map(RunRecord::new)
            .collect::<Result<Vec<_>>>()?,
    )
}
pub fn summarize_records(records: &[RunRecord]) -> Result<Vec<CohortSummary>> {
    let mut groups: BTreeMap<String, Vec<&RunRecord>> = BTreeMap::new();
    for o in records {
        groups.entry(o.cohort_id.clone()).or_default().push(o);
    }
    groups
        .into_iter()
        .map(|(id, group)| {
            let mut seen = BTreeSet::new();
            let mut unique = Vec::new();
            for o in &group {
                if seen.insert(o.fingerprint.clone()) {
                    unique.push(*o);
                }
            }
            let first = unique[0];
            let mut metrics = BTreeMap::new();
            for name in [
                "cpu_index",
                "single",
                "multi",
                "per_vcpu",
                "average",
                "reliable",
                "consistency",
            ] {
                metrics.insert(
                    name.into(),
                    describe(&unique.iter().map(|r| r.metrics[name]).collect::<Vec<_>>())?,
                );
            }
            let workloads = |multi: bool| -> Result<BTreeMap<String, RunStatistics>> {
                WORKLOADS
                    .iter()
                    .map(|name| {
                        Ok((
                            name.to_string(),
                            describe(
                                &unique
                                    .iter()
                                    .map(|r| {
                                        if multi {
                                            r.multi[*name]
                                        } else {
                                            r.single[*name]
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            )?,
                        ))
                    })
                    .collect()
            };
            Ok(CohortSummary {
                cohort_id: id,
                comparison_key: first.comparison_key.clone(),
                allocation: first.allocation.clone(),
                distinct_runs: unique.len() as u32,
                duplicate_runs: (group.len() - unique.len()) as u32,
                run_fingerprints: seen.into_iter().collect(),
                metrics,
                single_workloads: workloads(false)?,
                multi_workloads: workloads(true)?,
            })
        })
        .collect()
}
pub fn metric(r: &BenchmarkResult, name: &str) -> f64 {
    match name {
        "cpu_index" => r.scores.cpu_index,
        "single" => r.scores.single,
        "multi" => r.scores.multi,
        "per_vcpu" => r.scores.per_vcpu,
        "average" => r.scores.average,
        "reliable" => r.scores.reliable,
        "consistency" => r.scores.consistency,
        _ => unreachable!("fixed metric name"),
    }
}

use crate::{LoadedSeries, ReadOutcome};
use serverbench_analysis::{
    median_interval::{median_interval, ratio_interval},
    repeat::{metric, summarize, Observation},
};
use serverbench_core::{
    documents::{StoredDocument, Validity},
    series::CohortSummary,
    Error, Result,
};
use std::{collections::BTreeSet, fmt::Write};

pub fn series_console(series: &LoadedSeries) -> String {
    let mut out=format!("Repeat series: {:?}; {} of {} attempts recorded\nObserved repeatability across distinct completed runs (descriptive).\n",series.document.status,series.document.payload.attempts.len(),series.document.payload.requested_runs);
    if !series.children_verified {
        out.push_str("Child bundle not resolved; saved summaries are unverified.\n");
        return out;
    }
    for (i, c) in series.cohorts.iter().enumerate() {
        let _ = writeln!(
            out,
            "\nCohort {}: {} distinct runs, {} duplicate copies; {} workers",
            i + 1,
            c.distinct_runs,
            c.duplicate_runs,
            c.allocation.workers
        );
        out.push_str(
            "Metric               Median        Min        Max         SD       CV        MAD\n",
        );
        for (name, s) in &c.metrics {
            let cv = s
                .coefficient_of_variation
                .map(|v| format!("{:.2}%", v * 100.))
                .unwrap_or("n/a".into());
            let _ = writeln!(
                out,
                "{name:<16} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {cv:>8} {:>10.2}",
                s.median, s.minimum, s.maximum, s.standard_deviation, s.median_absolute_deviation
            );
        }
        out.push_str("Workload             Single median    Multi median    Unit\n");
        for (name, s) in &c.single_workloads {
            let _ = writeln!(
                out,
                "{name:<20} {:>12.2} {:>15.2}    {}",
                s.median,
                c.multi_workloads[name].median,
                serverbench_core::workload_contract(name).unwrap().0
            );
        }
    }
    if series.cohorts.len() > 1 {
        out.push_str(
            "Allocation/protocol changed: cohorts are separate; no pooled series score.\n",
        );
    }
    let unsuccessful = series
        .document
        .payload
        .attempts
        .iter()
        .filter(|a| a.status != serverbench_core::series::AttemptStatus::Valid)
        .count();
    let _ = writeln!(
        out,
        "Unsuccessful/pending attempts: {unsuccessful}. No population confidence claim is implied."
    );
    out
}
fn cohorts(value: &ReadOutcome) -> Result<Vec<CohortSummary>> {
    match value {
        ReadOutcome::Series(s) if s.children_verified => Ok(s.cohorts.clone()),
        ReadOutcome::Supported(d) => match &d.document {
            StoredDocument::LegacyBenchmarkV1(r) => summarize(&[Observation {
                result: r,
                fingerprints: &d.fingerprints,
            }]),
            _ => Ok(vec![]),
        },
        _ => Ok(vec![]),
    }
}
fn values(value: &ReadOutcome, metric_name: &str) -> Vec<f64> {
    let mut seen = BTreeSet::new();
    match value {
        ReadOutcome::Series(s) => s
            .runs
            .iter()
            .filter(|r| seen.insert(&r.fingerprints.canonical_run_sha256))
            .map(|r| metric(&r.result, metric_name))
            .collect(),
        ReadOutcome::Supported(d) => match &d.document {
            StoredDocument::LegacyBenchmarkV1(r) => vec![metric(r, metric_name)],
            _ => vec![],
        },
        _ => vec![],
    }
}
fn inference_eligible(value: &ReadOutcome) -> bool {
    match value {
        ReadOutcome::Series(s) => s.document.status == Validity::Valid,
        ReadOutcome::Supported(d) => matches!(d.document, StoredDocument::LegacyBenchmarkV1(_)),
        _ => false,
    }
}
pub fn compare_runs(a: &ReadOutcome, b: &ReadOutcome, independent: bool) -> Result<String> {
    let ca = cohorts(a)?;
    let cb = cohorts(b)?;
    let mut out = String::new();
    for (name, value, c) in [("A", a, &ca), ("B", b, &cb)] {
        let label = match value {
            ReadOutcome::Series(s) => s.document.payload.options.metadata.label.as_deref(),
            ReadOutcome::Supported(d) => match &d.document {
                StoredDocument::LegacyBenchmarkV1(r) => r.metadata.label.as_deref(),
                _ => None,
            },
            _ => None,
        }
        .unwrap_or("unlabelled");
        let _ = writeln!(
            out,
            "Server {name}: {}; {} cohort(s), {} distinct run(s)",
            crate::display_text(label),
            c.len(),
            c.iter().map(|c| c.distinct_runs).sum::<u32>()
        );
        if let ReadOutcome::Series(s) = value {
            let _ = writeln!(
                out,
                "  Series status {:?}; {} of {} attempts recorded",
                s.document.status,
                s.document.payload.attempts.len(),
                s.document.payload.requested_runs
            );
        }
        if let ReadOutcome::Unsupported(d) = value {
            let _ = writeln!(
                out,
                "  Unsupported: {:?}; schema {}; contract {}",
                d.reason,
                d.header.schema_version,
                crate::display_text(
                    d.header
                        .benchmark_contract_id
                        .as_deref()
                        .unwrap_or("unknown")
                )
            );
        }
        if let ReadOutcome::Supported(d) = value {
            if matches!(d.document, StoredDocument::PreviewBenchmarkV2(_)) {
                out.push_str("  Preview: uncalibrated; scores null.\n");
            }
        }
    }
    if ca.len() != 1 || cb.len() != 1 || ca[0].comparison_key != cb[0].comparison_key {
        out.push_str("WARNING: incompatible contracts or multiple/empty allocation cohorts; differences and uncertainty suppressed.\n");
        return Ok(out);
    }
    out.push_str("Metric                 A median       B median       Difference\n");
    for name in [
        "cpu_index",
        "single",
        "multi",
        "per_vcpu",
        "average",
        "reliable",
        "consistency",
    ] {
        let x = ca[0].metrics[name].median;
        let y = cb[0].metrics[name].median;
        let diff = if name == "consistency" {
            format!("{:+.1} points", y - x)
        } else if x > 0. {
            format!("{:+.1}%", (y / x - 1.) * 100.)
        } else {
            "n/a".into()
        };
        let _ = writeln!(out, "{name:<18} {x:>12.2} {y:>14.2} {diff:>16}");
    }
    let shared = ca[0]
        .run_fingerprints
        .iter()
        .any(|fp| cb[0].run_fingerprints.contains(fp));
    if shared || ca[0].duplicate_runs > 0 || cb[0].duplicate_runs > 0 {
        out.push_str(
            "NOTICE: duplicate/shared runs detected; copies do not add independent evidence.\n",
        );
    }
    if independent {
        out.push_str("\nConditional inference: requested independence of distinct runs within each cohort and reasonably stable sampling conditions. The tool cannot verify these assumptions.\n");
        if shared || !inference_eligible(a) || !inference_eligible(b) {
            out.push_str(
                "Uncertainty unavailable: shared runs or incomplete/failed/duplicate series.\n",
            );
            return Ok(out);
        }
        for name in [
            "cpu_index",
            "single",
            "multi",
            "per_vcpu",
            "average",
            "reliable",
            "consistency",
        ] {
            let av = values(a, name);
            let bv = values(b, name);
            for (server, v) in [("A", &av), ("B", &bv)] {
                let interval = median_interval(v, 0.95)?;
                match (interval.lower, interval.upper, interval.coverage_bound) {
                    (Some(lo), Some(hi), Some(coverage)) => {
                        let _=writeln!(out,"{name} {server} median interval [{lo:.4}, {hi:.4}]; requested 95%, coverage bound {:.6}%",coverage*100.);
                    }
                    _ => {
                        let _ = writeln!(
                            out,
                            "{name} {server} median interval: insufficient_repeats (n={})",
                            v.len()
                        );
                    }
                }
            }
            let ratio = ratio_interval(&av, &bv)?;
            match (ratio.lower, ratio.upper, ratio.joint_coverage_bound) {
                (Some(lo), Some(hi), Some(coverage)) => {
                    let _=writeln!(out,"{name} median(B)/median(A): [{lo:.4}, {hi:.4}]; joint coverage bound {:.6}% (requested 95%)",coverage*100.);
                }
                (Some(lo), None, Some(coverage)) if ratio.upper_unbounded => {
                    let _=writeln!(out,"{name} median(B)/median(A): [{lo:.4}, unbounded]; joint coverage bound {:.6}%",coverage*100.);
                }
                _ => {
                    let _=writeln!(out,"{name} comparison interval: {} (requires at least 10 distinct runs per cohort)",ratio.reason.unwrap_or("unavailable".into()));
                }
            }
        }
    } else {
        out.push_str("Descriptive comparison only; run windows are not independent repetitions.\n");
    }
    Ok(out)
}

pub fn require_series(outcome: &ReadOutcome) -> Result<&LoadedSeries> {
    if let ReadOutcome::Series(s) = outcome {
        Ok(s)
    } else {
        Err(Error::new(4, "expected repeat series"))
    }
}

use serverbench_analysis::{
    median_interval::{median_interval, ratio_interval},
    repeat::{describe, summarize, Observation},
};
use serverbench_core::{
    documents::{Fingerprints, StoredDocument},
    BenchmarkResult,
};
fn fixture() -> BenchmarkResult {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/results/legacy-v1-stable.json"
    ))
    .unwrap()
}
fn fingerprint(r: &BenchmarkResult) -> Fingerprints {
    Fingerprints::new(
        &serde_json::to_vec(r).unwrap(),
        &StoredDocument::LegacyBenchmarkV1(Box::new(r.clone())),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
#[test]
fn descriptive_statistics_include_population_sd_and_mad() {
    let s = describe(&[1., 2., 3., 4., 10.]).unwrap();
    assert_eq!(s.count, 5);
    near(s.median, 3.);
    near(s.mean, 4.);
    near(s.standard_deviation, 10f64.sqrt());
    near(s.median_absolute_deviation, 1.);
    near(s.coefficient_of_variation.unwrap(), 10f64.sqrt() / 4.);
    assert_eq!(describe(&[0., 0.]).unwrap().coefficient_of_variation, None);
    for bad in [vec![], vec![-1.], vec![f64::NAN], vec![f64::INFINITY]] {
        assert!(describe(&bad).is_err());
    }
}
#[test]
fn exact_order_statistic_intervals_match_golden_cases() {
    for n in [3, 5] {
        let i = median_interval(&(1..=n).map(f64::from).collect::<Vec<_>>(), 0.95).unwrap();
        assert_eq!(i.reason.as_deref(), Some("insufficient_repeats"));
        assert!(i.lower.is_none());
    }
    for (n, lo, hi, coverage) in [
        (6, 1., 6., 0.96875),
        (10, 2., 9., 0.978515625),
        (20, 6., 15., 0.9586105346679688),
    ] {
        let i = median_interval(&(1..=n).map(f64::from).collect::<Vec<_>>(), 0.95).unwrap();
        assert_eq!((i.lower, i.upper), (Some(lo), Some(hi)));
        near(i.coverage_bound.unwrap(), coverage);
    }
    let ties = median_interval(&[7.; 10], 0.95).unwrap();
    assert_eq!((ties.lower, ties.upper), (Some(7.), Some(7.)));
    let large = median_interval(&(1..=100).map(f64::from).collect::<Vec<_>>(), 0.95).unwrap();
    assert_eq!((large.lower, large.upper), (Some(40.), Some(61.)));
    assert!(large.coverage_bound.unwrap() >= 0.95);
    // Independent exact integer binomial sum for the selected n=100 endpoint.
    let choose =
        |j: u32| -> u128 { (0..j).fold(1u128, |c, k| c * u128::from(100 - k) / u128::from(k + 1)) };
    let bound =
        ((1u128 << 100) - 2 * (0..40).map(choose).sum::<u128>()) as f64 / (1u128 << 100) as f64;
    near(large.coverage_bound.unwrap(), bound);
    assert!(median_interval(&[1.], 1.).is_err());
    assert!(median_interval(&[f64::NAN], 0.95).is_err());
}
#[test]
fn comparison_uses_union_bound_and_handles_zero_and_small_cohorts() {
    let r = ratio_interval(&[2.; 10], &[4.; 10]).unwrap();
    assert_eq!(
        (r.point_ratio, r.lower, r.upper),
        (Some(2.), Some(2.), Some(2.))
    );
    assert!(r.joint_coverage_bound.unwrap() >= 0.95);
    assert_eq!(
        ratio_interval(&[1.; 3], &[2.; 10])
            .unwrap()
            .reason
            .as_deref(),
        Some("insufficient_repeats")
    );
    assert_eq!(
        ratio_interval(&[0.; 10], &[2.; 10])
            .unwrap()
            .reason
            .as_deref(),
        Some("undefined_baseline")
    );
    let mut a = vec![0.; 3];
    a.extend([2.; 7]);
    let r = ratio_interval(&a, &[4.; 12]).unwrap();
    assert_eq!(r.point_ratio, Some(2.));
    assert!(r.upper_unbounded);
    assert_eq!(r.upper, None);
    assert_eq!(r.lower, Some(2.));
    assert!(ratio_interval(&[-1.; 10], &[2.; 10]).is_err());
}
#[test]
fn renamed_copies_are_deduplicated_and_changed_allocation_is_partitioned() {
    let a = fixture();
    let mut copy = a.clone();
    copy.metadata.label = Some("a copy".into());
    let mut changed = a.clone();
    changed.timestamp = "2026-01-01T01:00:00Z".into();
    changed.system.cpu.quota_cpus = Some(1.5);
    let fa = fingerprint(&a);
    let fb = fingerprint(&copy);
    let fc = fingerprint(&changed);
    let cohorts = summarize(&[
        Observation {
            result: &a,
            fingerprints: &fa,
        },
        Observation {
            result: &copy,
            fingerprints: &fb,
        },
        Observation {
            result: &changed,
            fingerprints: &fc,
        },
    ])
    .unwrap();
    assert_eq!(cohorts.len(), 2);
    assert_eq!(cohorts.iter().map(|c| c.distinct_runs).sum::<u32>(), 2);
    assert_eq!(cohorts.iter().map(|c| c.duplicate_runs).sum::<u32>(), 1);
    assert!(cohorts.iter().all(|c| c.metrics["cpu_index"].count == 1));
}

use serverbench_core::*;
use serverbench_report::{
    compare, compare_files, console, incompatibilities, save_result, unique_name,
};
use serverbench_score::{calculate, reference_hash, validate_result};
use std::collections::BTreeMap;

fn measurement(name: &str, workers: usize) -> Measurement {
    let (unit, units) = workload_contract(name).unwrap();
    let rate = units as f64 * 10. * workers as f64;
    let samples = (1..=4)
        .map(|i| Sample {
            elapsed_seconds: i as f64,
            interval_seconds: 1.,
            throughput: Some(rate),
            cpu_utilization_percent: Some(100.),
            steal_percent: Some(0.),
            context_switches_per_second: None,
            load_average: None,
            frequency_mhz: None,
            cgroup_throttled_seconds: None,
            memory: None,
            cpu_temperature_celsius: None,
        })
        .collect();
    Measurement {
        workload: name.into(),
        unit: unit.into(),
        workers,
        elapsed_seconds: 4.,
        completed_units: units * 40 * workers as u64,
        completed_batches: 40 * workers as u64,
        total_throughput: rate,
        throughput_per_vcpu: rate / workers as f64,
        scaling_efficiency: None,
        throughput: statistics(&[rate; 4]),
        samples,
        affinity_applied: vec![true; workers],
        checksums_valid: true,
        worker_finish_skew_seconds: 0.,
    }
}
fn fixture() -> BenchmarkResult {
    let single: BTreeMap<_, _> = WORKLOADS
        .iter()
        .map(|name| (name.to_string(), measurement(name, 1)))
        .collect();
    let multi = WORKLOADS
        .iter()
        .map(|name| (name.to_string(), measurement(name, 2)))
        .collect();
    let sustained = measurement("mixed", 2);
    let scores = calculate(&single, &multi, &sustained, 1).unwrap();
    BenchmarkResult {
        schema_version: SCHEMA_VERSION,
        application_version: "0.1.0".into(),
        benchmark_version: BENCHMARK_VERSION.into(),
        benchmark_schema_version: BENCHMARK_SCHEMA,
        contract: CONTRACT.into(),
        baseline_status: "experimental".into(),
        reference_sha256: reference_hash(),
        dataset_sha256: DATASET_SHA256.into(),
        binary_sha256: None,
        build_target: "fixture-linux".into(),
        compiler_version: "fixture".into(),
        optimized_build: true,
        hash_implementation: "software SHA-256".into(),
        timestamp: "2026-10-29T12:00:00Z".into(),
        benchmark_mode: Mode::Full,
        durations: Mode::Full.durations(),
        system: SystemInfo {
            cpu: CpuInfo {
                usable_cpus: 2,
                ..Default::default()
            },
            ..Default::default()
        },
        metadata: Metadata {
            anonymous: true,
            ..Default::default()
        },
        warnings: vec![],
        idle_samples: vec![],
        single,
        multi,
        sustained,
        steal: Some(statistics(&[0.; 4])),
        scores,
        price_performance: None,
    }
}
#[test]
fn roundtrip_and_compare_same_contract() {
    let a = fixture();
    validate_result(&a).unwrap();
    let b: BenchmarkResult = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    validate_result(&b).unwrap();
    let text = compare(&a, &b).unwrap();
    assert!(text.contains("+0.0%"));
    assert!(text.contains("+0.0 points"));
    assert!(!text.contains("WARNING"));
}
#[test]
fn supplied_labels_show_in_reports_without_breaking_columns() {
    let mut a = fixture();
    let mut b = a.clone();
    a.metadata.label = Some("Proxmox VM with a deliberately long label".into());
    b.metadata.label = Some("Dedicated Ubuntu".into());
    let text = compare(&a, &b).unwrap();
    assert!(text.contains("Server A: Proxmox VM with a deliberately long label"));
    assert!(text.contains("Server B: Dedicated Ubuntu"));
    assert!(text.contains("+0.0%"));
    assert!(console(&a).contains("Server label          Proxmox VM"));
    a.metadata.label = Some("VM\n\u{1b}[31m".into());
    let text = console(&a);
    assert!(!text.contains('\u{1b}'));
}
#[test]
fn compare_files_uses_filenames_when_unlabelled() {
    let dir = std::env::temp_dir().join(unique_name("serverbench-names-test"));
    let a = dir.join("proxmox.json");
    let b = dir.join("dedicated.json");
    let result = fixture();
    save_result(&a, &result).unwrap();
    save_result(&b, &result).unwrap();
    let text = compare_files(&a, &b).unwrap();
    assert!(text.contains("Server A: proxmox.json"));
    assert!(text.contains("Server B: dedicated.json"));
    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
#[test]
fn old_samples_and_results_remain_compatible() {
    let a = fixture();
    let mut value = serde_json::to_value(&a).unwrap();
    for phase in ["single", "multi"] {
        for measurement in value[phase].as_object_mut().unwrap().values_mut() {
            for sample in measurement["samples"].as_array_mut().unwrap() {
                let object = sample.as_object_mut().unwrap();
                object.remove("memory");
                object.remove("cpu_temperature_celsius");
            }
        }
    }
    for sample in value["sustained"]["samples"].as_array_mut().unwrap() {
        let object = sample.as_object_mut().unwrap();
        object.remove("memory");
        object.remove("cpu_temperature_celsius");
    }
    let b: BenchmarkResult = serde_json::from_value(value).unwrap();
    assert!(b.sustained.samples[0].memory.is_none());
    assert!(b.sustained.samples[0].cpu_temperature_celsius.is_none());
    assert!(compare(&a, &b).unwrap().contains("+0.0%"));
}
#[test]
fn mismatches_suppress_percentages() {
    let a = fixture();
    let mut cases = Vec::new();
    let mut b = a.clone();
    b.schema_version += 1;
    cases.push(b);
    let mut b = a.clone();
    b.benchmark_version = "2.0.0".into();
    cases.push(b);
    let mut b = a.clone();
    b.benchmark_schema_version += 1;
    cases.push(b);
    let mut b = a.clone();
    b.benchmark_mode = Mode::Quick;
    cases.push(b);
    let mut b = a.clone();
    b.durations.sustained_seconds += 1;
    cases.push(b);
    let mut b = a.clone();
    b.dataset_sha256 = "different".into();
    cases.push(b);
    let mut b = a.clone();
    b.optimized_build = false;
    cases.push(b);
    for b in cases {
        assert!(!incompatibilities(&a, &b).is_empty());
        let text = compare(&a, &b).unwrap();
        assert!(text.contains("WARNING"));
        assert!(!text.contains('%'));
    }
}
#[test]
fn tampered_results_are_rejected() {
    let a = fixture();
    let mut b = a.clone();
    b.scores.cpu_index += 1.;
    assert!(compare(&a, &b).is_err());
    let mut b = a.clone();
    b.single.get_mut("hash").unwrap().checksums_valid = false;
    assert!(validate_result(&b).is_err());
    let mut b = a.clone();
    b.sustained.throughput.p95 += 1.;
    assert!(validate_result(&b).is_err());
    let mut b = a.clone();
    b.sustained.completed_units += 1;
    assert!(validate_result(&b).is_err());
    let mut b = a.clone();
    b.sustained.samples[0].throughput = Some(f64::NAN);
    assert!(validate_result(&b).is_err());
    let mut b = a.clone();
    b.sustained.samples[0].steal_percent = Some(101.);
    assert!(validate_result(&b).is_err());
    let mut b = a;
    b.single.remove("data");
    assert!(validate_result(&b).is_err());
}
#[test]
fn instability_lowers_reliable_and_consistency() {
    let stable = fixture();
    let mut unstable = stable.clone();
    let rates = [1., 2., 20., 20.];
    for (sample, rate) in unstable.sustained.samples.iter_mut().zip(rates) {
        sample.throughput = Some(rate);
        sample.steal_percent = Some(15.);
    }
    unstable.sustained.throughput = statistics(&rates);
    unstable.scores = calculate(&unstable.single, &unstable.multi, &unstable.sustained, 1).unwrap();
    assert!(unstable.scores.reliable < stable.scores.reliable);
    assert!(unstable.scores.consistency < stable.scores.consistency);
    let rates = [0., 0., 20., 20.];
    for (sample, rate) in unstable.sustained.samples.iter_mut().zip(rates) {
        sample.throughput = Some(rate);
    }
    unstable.sustained.throughput = statistics(&rates);
    let scores = calculate(&unstable.single, &unstable.multi, &unstable.sustained, 1).unwrap();
    assert_eq!(scores.reliable, 0.);
    assert_eq!(scores.cpu_index, 0.);
}

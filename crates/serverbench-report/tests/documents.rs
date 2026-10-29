use serde_json::{json, Value};
use serverbench_core::{
    documents::{ComparisonKey, ReasonCode, StoredDocument},
    v2::{Counter, Quota},
    BenchmarkResult,
};
use serverbench_report::{
    compare_files, distinct_run_count, duplicate_kind, incompatibilities, read_document,
    unique_name, DuplicateKind, LoadedDocument, ReadOutcome,
};

const STABLE: &[u8] = include_bytes!("../../../tests/fixtures/results/legacy-v1-stable.json");
const VARIABLE: &[u8] = include_bytes!("../../../tests/fixtures/results/legacy-v1-variable.json");
const PREVIEW: &[u8] = include_bytes!("../../../tests/fixtures/results/preview-v2-incomplete.json");
fn loaded(bytes: &[u8]) -> Box<LoadedDocument> {
    match read_document(bytes).unwrap() {
        ReadOutcome::Supported(d) => d,
        other => panic!("expected supported document, got {other:?}"),
    }
}
fn legacy(bytes: &[u8]) -> BenchmarkResult {
    match loaded(bytes).document {
        StoredDocument::LegacyBenchmarkV1(r) => *r,
        _ => panic!("legacy"),
    }
}
fn encoded(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
fn preview() -> Value {
    serde_json::from_slice(PREVIEW).unwrap()
}
fn unsupported(value: Value, reason: ReasonCode) {
    match read_document(&encoded(&value)).unwrap() {
        ReadOutcome::Unsupported(d) => assert_eq!(d.reason, reason),
        d => panic!("accepted unsupported document: {d:?}"),
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-10 * b.abs().max(1.), "{a} != {b}");
}

#[test]
fn legacy_golden_scores_and_reference_are_unchanged() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/results/legacy-v1-expected.json"
    ))
    .unwrap();
    for (name, bytes) in [("stable", STABLE), ("variable", VARIABLE)] {
        let r = legacy(bytes);
        assert_eq!(r.reference_sha256, expected[name]["reference_sha256"]);
        assert_eq!(r.reference_sha256, serverbench_score::REFERENCE_SHA256);
        assert_eq!(
            serverbench_score::reference_hash(),
            serverbench_score::REFERENCE_SHA256
        );
        let recalculated = serverbench_score::calculate(
            &r.single,
            &r.multi,
            &r.sustained,
            r.durations.sample_seconds,
        )
        .unwrap();
        let actual = serde_json::to_value(recalculated).unwrap();
        for metric in [
            "single",
            "multi",
            "per_vcpu",
            "average",
            "reliable",
            "cpu_index",
            "consistency",
        ] {
            near(
                actual[metric].as_f64().unwrap(),
                expected[name]["scores"][metric].as_f64().unwrap(),
            );
        }
        assert_eq!(
            actual["consistency_label"],
            expected[name]["scores"]["consistency_label"]
        );
        for (key, value) in actual["consistency_inputs"].as_object().unwrap() {
            near(
                value.as_f64().unwrap(),
                expected[name]["scores"]["consistency_inputs"][key]
                    .as_f64()
                    .unwrap(),
            );
        }
        assert!(r.sustained.samples[0].memory.is_none());
        assert!(r.sustained.samples[0].cpu_temperature_celsius.is_none());
    }
}

#[test]
fn legacy_comparison_decisions_match_the_snapshot() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/results/legacy-v1-expected.json"
    ))
    .unwrap();
    let a = legacy(STABLE);
    let key = ComparisonKey::legacy(&a);
    for case in expected["compatibility"].as_array().unwrap() {
        let mut b = serde_json::to_value(&a).unwrap();
        b[case["change"].as_str().unwrap()] = case["value"].clone();
        let b: BenchmarkResult = serde_json::from_value(b).unwrap();
        let compatible = case["compatible"].as_bool().unwrap();
        assert_eq!(key == ComparisonKey::legacy(&b), compatible, "{case}");
        assert_eq!(incompatibilities(&a, &b).is_empty(), compatible, "{case}");
    }
}

#[test]
fn dispatch_never_guesses_foreign_payloads_into_v1() {
    let future: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/results/future-contract.json"
    ))
    .unwrap();
    unsupported(future, ReasonCode::UnsupportedContract);
    unsupported(
        json!({"schema_version":999,"document_type":"benchmark","scores":{"cpu_index":999}}),
        ReasonCode::UnsupportedSchema,
    );
    for kind in ["scaling_study", "latency_study", "comparison_report"] {
        unsupported(
            json!({"schema_version":2,"document_type":kind}),
            ReasonCode::NotImplemented,
        );
    }
    unsupported(
        json!({"schema_version":2,"document_type":"alien"}),
        ReasonCode::UnsupportedDocumentType,
    );
    let mut p = preview();
    p["protocol_id"] = json!("future");
    unsupported(p, ReasonCode::UnsupportedProtocol);
    let mut p = preview();
    p["score_model_id"] = json!("future");
    unsupported(p, ReasonCode::UnsupportedScoreModel);
    let mut p = preview();
    p["build"]["policy_id"] = json!("native-local");
    unsupported(p, ReasonCode::UnsupportedBuildPolicy);
    let mut v: Value = serde_json::from_slice(STABLE).unwrap();
    v["contract"] = json!("future");
    unsupported(v, ReasonCode::UnsupportedContract);
}

#[test]
fn legacy_absence_is_supported_but_ambiguous_headers_are_rejected() {
    let mut v: Value = serde_json::from_slice(STABLE).unwrap();
    v["document_type"] = json!("benchmark");
    loaded(&encoded(&v));
    v["document_type"] = Value::Null;
    assert!(read_document(&encoded(&v)).is_err());
    assert!(read_document(br#"{"schema_version":1,"schema_version":2}"#).is_err());
    assert!(read_document(br#"{"schema_version":999,"nested":{"key":1,"key":2}}"#).is_err());
    assert!(read_document(br#"{"schema_version":2}"#).is_err());
    assert!(read_document(b"[]").is_err());
    assert!(read_document(br#"{"schema_version":999,"number":1e999}"#).is_err());
}

#[test]
fn preview_dispatch_has_no_legacy_score_path() {
    let p = loaded(PREVIEW);
    assert!(matches!(
        serverbench_score::validate_document(&p.document).unwrap(),
        serverbench_score::ScoreDisposition::Uncalibrated
    ));
    assert!(matches!(p.comparison_key, ComparisonKey::PreviewV2(_)));
    for (path, value) in [
        ("/scores", json!({"cpu_index":5000})),
        ("/score_status", json!("available")),
        ("/status", json!("valid")),
    ] {
        let mut p = preview();
        *p.pointer_mut(path).unwrap() = value;
        assert!(read_document(&encoded(&p)).is_err(), "{path}");
    }
    for field in ["scores", "score_model_id"] {
        let mut p = preview();
        p.as_object_mut().unwrap().remove(field);
        assert!(read_document(&encoded(&p)).is_err());
    }
}

#[test]
fn exact_decimal_u64_counters_reject_lossy_and_overflow_inputs() {
    for n in [0, 1, 9_007_199_254_740_993, u64::MAX] {
        let encoded = serde_json::to_string(&Counter(n)).unwrap();
        assert_eq!(encoded, format!("\"{n}\""));
        assert_eq!(
            serde_json::from_str::<Counter>(&encoded).unwrap(),
            Counter(n)
        );
    }
    for input in [
        "0",
        "1.5",
        "-1",
        "null",
        "\"\"",
        "\"01\"",
        "\"+1\"",
        "\"-1\"",
        "\"1.0\"",
        "\"1e3\"",
        "\"18446744073709551616\"",
        "\" 1\"",
    ] {
        assert!(serde_json::from_str::<Counter>(input).is_err(), "{input}");
    }
}

#[test]
fn preview_rejects_inconsistent_accounting_timings_scopes_and_status() {
    let cases = [
        ("/payload/phases/0/completed_units", json!("103")),
        (
            "/payload/phases/0/workers/0/completed_batches",
            json!("18446744073709551615"),
        ),
        (
            "/payload/phases/0/workers/0/finish_offset_ns",
            json!("6000000000"),
        ),
        ("/payload/phases/0/workers/0/finish_offset_ns", json!("0")),
        (
            "/payload/phases/0/workers/0/start_offset_ns",
            json!("5000000000"),
        ),
        (
            "/payload/phases/0/workers/0/cpu_time_ns",
            json!("6000000000"),
        ),
        ("/payload/phases/0/workers/0/cpu_id", json!(99)),
        ("/payload/phases/0/workers/0/status", json!("invalid")),
        (
            "/payload/phases/0/throughput_windows/1/start_ns",
            json!("900000000"),
        ),
        (
            "/payload/phases/0/throughput_windows/0/completed_units",
            json!("21"),
        ),
        ("/payload/phases/0/tail_units/0", json!("2")),
        ("/payload/phases/0/checksums_valid", json!(false)),
        ("/payload/phases/0/total_throughput", json!(-1)),
        ("/payload/phases/0/resource_samples/0/end_ns", json!("0")),
        (
            "/payload/phases/0/resource_samples/0/cpu_utilization_percent",
            json!(101),
        ),
        (
            "/payload/phases/0/resource_samples/0/scope/kind",
            json!("host"),
        ),
        ("/capabilities/cpu_time/scope/ordinal", json!(2)),
        ("/capabilities/cpu_time/source", json!("/proc/123/task/456")),
        ("/capabilities/pressure/reason", json!("interrupted")),
        (
            "/payload/protocol/durations/sample_ns",
            json!(1_000_000_000),
        ),
        ("/allocation/selected_cpu_ids", json!([0, 0])),
        ("/allocation/visible_cpu_equivalents", json!(1.5)),
        ("/build/target_cpu", json!("native")),
        ("/build/extra_target_features", json!(["+avx2"])),
        (
            "/build/compiler_version",
            json!("rustc 1.85.1 (/home/user/compiler)"),
        ),
        ("/timestamp", json!("2026-02-30T12:00:00Z")),
        ("/reason_codes", json!([])),
    ];
    for (path, value) in cases {
        let mut p = preview();
        *p.pointer_mut(path).unwrap() = value;
        assert!(
            read_document(&encoded(&p)).is_err(),
            "accepted invalid {path}"
        );
    }
    let mut p = preview();
    let sample = p["payload"]["phases"][0]["resource_samples"][0].clone();
    p["payload"]["phases"][0]["resource_samples"]
        .as_array_mut()
        .unwrap()
        .push(sample);
    assert!(
        read_document(&encoded(&p)).is_err(),
        "overlapping same-scope observations"
    );
    let mut p = preview();
    p["payload"]["phases"][0]["workload"]["units_per_batch"] = json!("2");
    assert!(read_document(&encoded(&p)).is_err());
}

#[test]
fn quota_ceiling_is_exact_even_for_fractional_and_large_limits() {
    let p = loaded(PREVIEW);
    let StoredDocument::PreviewBenchmarkV2(p) = p.document else {
        panic!("preview")
    };
    let mut a = p.allocation;
    a.quota = Quota::Limited {
        quota_us: Counter(150_000),
        period_us: Counter(100_000),
    };
    a.visible_cpu_equivalents = 1.5;
    a.validate().unwrap();
    a.quota = Quota::Limited {
        quota_us: Counter(50_000),
        period_us: Counter(100_000),
    };
    a.visible_cpu_equivalents = 0.5;
    a.worker_count = 1;
    a.selected_cpu_ids = vec![0];
    a.validate().unwrap();
    a.quota = Quota::Limited {
        quota_us: Counter(u64::MAX),
        period_us: Counter(1),
    };
    a.visible_cpu_equivalents = 2.;
    a.worker_count = 2;
    a.selected_cpu_ids = vec![0, 1];
    a.validate().unwrap();
    a.quota = Quota::Limited {
        quota_us: Counter(1),
        period_us: Counter(0),
    };
    assert!(a.validate().is_err());
}

#[test]
fn generic_build_identity_allows_architecture_but_separates_libc_and_dependencies() {
    let p = loaded(PREVIEW);
    let mut arm = preview();
    arm["build"]["target"] = json!("aarch64-unknown-linux-musl");
    assert_eq!(p.comparison_key, loaded(&encoded(&arm)).comparison_key);
    arm["build"]["target"] = json!("aarch64-unknown-linux-gnu");
    assert_ne!(p.comparison_key, loaded(&encoded(&arm)).comparison_key);
    let mut dep = preview();
    dep["build"]["dependency_lock_sha256"] = json!("2".repeat(64));
    assert_ne!(p.comparison_key, loaded(&encoded(&dep)).comparison_key);
    assert_ne!(p.comparison_key, loaded(STABLE).comparison_key);
}

#[test]
fn fingerprints_deduplicate_copies_reformatting_and_metadata() {
    let a = loaded(STABLE);
    let exact = loaded(STABLE);
    assert_eq!(duplicate_kind(&a, &exact), Some(DuplicateKind::ExactFile));
    let mut value: Value = serde_json::from_slice(STABLE).unwrap();
    value["metadata"]["label"] = json!("Relabelled copy");
    value["metadata"]["monthly_cost"] = json!(49.95);
    value["metadata"]["currency"] = json!("EUR");
    value["application_version"] = json!("0.1.99");
    // Old omitted optional fields and explicit null observations normalize identically.
    value["sustained"]["samples"][0]["memory"] = Value::Null;
    let b = loaded(&encoded(&value));
    assert_ne!(
        a.fingerprints.exact_file_sha256,
        b.fingerprints.exact_file_sha256
    );
    assert_eq!(duplicate_kind(&a, &b), Some(DuplicateKind::CanonicalRun));
    assert_eq!(distinct_run_count(&[&a, &exact, &b]), 1);
    let other = loaded(VARIABLE);
    assert_eq!(distinct_run_count(&[&a, &other]), 2);
    value["sustained"]["samples"][0]["cpu_utilization_percent"] = json!(99.);
    assert_eq!(duplicate_kind(&a, &loaded(&encoded(&value))), None);
    let preview = loaded(PREVIEW);
    assert_eq!(distinct_run_count(&[&preview]), 0);
}

fn with_files<F: FnOnce(&std::path::Path, &std::path::Path)>(a: &[u8], b: &[u8], f: F) {
    let dir = std::env::temp_dir().join(unique_name("document-test"));
    std::fs::create_dir(&dir).unwrap();
    let left = dir.join("left.json");
    let right = dir.join("renamed.json");
    std::fs::write(&left, a).unwrap();
    std::fs::write(&right, b).unwrap();
    f(&left, &right);
    std::fs::remove_dir_all(&dir).unwrap();
}
#[test]
fn file_comparison_suppresses_foreign_scores_and_reports_duplicates() {
    with_files(STABLE, PREVIEW, |a, b| {
        let out = compare_files(a, b).unwrap();
        assert!(out.contains("Synthetic stable"));
        assert!(out.contains("Synthetic preview"));
        assert!(out.contains("uncalibrated"));
        assert!(out.contains("differences suppressed"));
        assert!(!out.contains('%'));
        assert!(!out.contains("CPU Index"));
    });
    with_files(
        STABLE,
        include_bytes!("../../../tests/fixtures/results/future-contract.json"),
        |a, b| {
            let out = compare_files(a, b).unwrap();
            assert!(out.contains("UnsupportedContract"));
            assert!(!out.contains("999999"));
            assert!(!out.contains('%'));
        },
    );
    with_files(STABLE, STABLE, |a, b| {
        let out = compare_files(a, b).unwrap();
        assert!(out.contains("duplicate input"));
        assert!(out.contains("one distinct run"));
        assert!(out.contains("+0.0%"));
    });
    with_files(PREVIEW, PREVIEW, |a, b| {
        let out = compare_files(a, b).unwrap();
        assert!(!out.contains('%'));
        assert!(out.contains("scores null"));
    });
}

#[test]
fn reader_validates_each_known_file_before_comparing_even_if_builds_differ() {
    let mut tampered: Value = serde_json::from_slice(STABLE).unwrap();
    tampered["scores"]["cpu_index"] = json!(999999);
    tampered["compiler_version"] = json!("different");
    with_files(STABLE, &encoded(&tampered), |a, b| {
        assert!(compare_files(a, b).is_err())
    });
}

#[test]
fn complete_preview_requires_the_full_ordered_protocol_and_stays_unscored() {
    use serverbench_core::documents::Validity;
    use serverbench_core::v2::{PhaseRole, ThroughputWindow};
    let StoredDocument::PreviewBenchmarkV2(mut p) = loaded(PREVIEW).document else {
        panic!("preview")
    };
    let template = p.payload.phases[0].clone();
    for role in [PhaseRole::Single, PhaseRole::Multi, PhaseRole::Sustained] {
        let names = if role == PhaseRole::Sustained {
            vec!["mixed"]
        } else {
            serverbench_core::WORKLOADS.to_vec()
        };
        for name in names {
            let mut phase = template.clone();
            let (unit, units) = serverbench_core::workload_contract(name).unwrap();
            let seconds = if role == PhaseRole::Sustained { 30 } else { 4 };
            let workers = if role == PhaseRole::Single { 1 } else { 2 };
            phase.phase_id = format!("{role:?}-{name}");
            phase.role = role;
            phase.workload.workload_id = name.into();
            phase.workload.unit = unit.into();
            phase.workload.units_per_batch = Counter(units);
            phase.workload.dataset_sha256 = if matches!(name, "integer" | "data") {
                None
            } else {
                Some(serverbench_core::DATASET_SHA256.into())
            };
            phase.nominal_duration_ns = Counter(seconds * 1_000_000_000);
            phase.elapsed_ns = Counter(seconds * 1_000_000_000 + 100_000_000);
            phase.workers.truncate(workers);
            for worker in &mut phase.workers {
                worker.completed_batches = Counter(seconds * 10 + 1);
                worker.completed_units = Counter((seconds * 10 + 1) * units);
                worker.finish_offset_ns = phase.elapsed_ns;
                worker.cpu_time_ns = None;
            }
            phase.completed_batches = Counter((seconds * 10 + 1) * workers as u64);
            phase.completed_units = Counter(phase.completed_batches.0 * units);
            phase.total_throughput =
                phase.completed_units.0 as f64 / (phase.elapsed_ns.0 as f64 / 1e9);
            phase.throughput_windows = (0..seconds)
                .map(|i| ThroughputWindow {
                    start_ns: Counter(i * 1_000_000_000),
                    end_ns: Counter((i + 1) * 1_000_000_000),
                    per_worker_units: vec![Counter(10 * units); workers],
                    completed_units: Counter(10 * units * workers as u64),
                })
                .collect();
            phase.tail_units = vec![Counter(units); workers];
            phase.resource_samples.clear();
            p.payload.phases.push(phase);
        }
    }
    p.status = Validity::Valid;
    p.reason_codes.clear();
    let result = loaded(&serde_json::to_vec(&p).unwrap());
    assert_eq!(distinct_run_count(&[&result, &result]), 1);
    assert!(matches!(
        serverbench_score::validate_document(&result.document).unwrap(),
        serverbench_score::ScoreDisposition::Uncalibrated
    ));
    p.payload.phases.swap(1, 2);
    assert!(p.validate().is_err());
}

#[test]
fn preview_rejects_nonfinite_typed_values_and_overflowing_sums() {
    let StoredDocument::PreviewBenchmarkV2(mut p) = loaded(PREVIEW).document else {
        panic!("preview")
    };
    p.payload.phases[0].total_throughput = f64::INFINITY;
    assert!(p.validate().is_err());
    p.payload.phases[0].total_throughput = 20.;
    p.metadata.monthly_cost = Some(f64::NAN);
    assert!(p.validate().is_err());
    p.metadata.monthly_cost = None;
    for worker in &mut p.payload.phases[0].workers {
        worker.completed_batches = Counter(u64::MAX);
        worker.completed_units = Counter(u64::MAX);
    }
    assert!(p.validate().unwrap_err().message.contains("overflow"));
}

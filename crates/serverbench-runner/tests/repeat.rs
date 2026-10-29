use serverbench_core::{
    documents::{StoredDocument, Validity},
    series::{AttemptStatus, RunSpec},
    BenchmarkResult,
};
use serverbench_report::{load_document, require_series, save_result};
use serverbench_runner::{
    process::{Cancellation, ChildExecutor, ChildExit, Invocation},
    repeat::{child_invocation, run_repeat, RepeatOptions},
};
use std::{path::PathBuf, time::Duration};
fn fixture() -> BenchmarkResult {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/results/legacy-v1-stable.json"
    ))
    .unwrap()
}
fn options() -> RepeatOptions {
    let r = fixture();
    RepeatOptions {
        executable: PathBuf::from("fake-serverbench"),
        output: std::env::temp_dir()
            .join(serverbench_report::unique_name("repeat-test"))
            .join("series.json"),
        spec: RunSpec {
            mode: r.benchmark_mode,
            durations: r.durations,
            strict: false,
            metadata: r.metadata,
        },
        count: 3,
        pause_seconds: 60,
        continue_on_error: false,
        quiet: true,
        verbose: false,
        no_progress: true,
    }
}
struct Fake {
    calls: Vec<Invocation>,
    pauses: Vec<Duration>,
    fail_at: Option<usize>,
    missing_at: Option<usize>,
    duplicate: bool,
    cancel_at: Option<usize>,
    change_allocation: bool,
    timestamp_offset: usize,
}
impl Fake {
    fn new() -> Self {
        Self {
            calls: vec![],
            pauses: vec![],
            fail_at: None,
            missing_at: None,
            duplicate: false,
            cancel_at: None,
            change_allocation: false,
            timestamp_offset: 0,
        }
    }
}
impl ChildExecutor for Fake {
    fn execute(
        &mut self,
        invocation: &Invocation,
        cancellation: &Cancellation,
    ) -> serverbench_core::Result<ChildExit> {
        self.calls.push(invocation.clone());
        let n = self.calls.len();
        let output = PathBuf::from(
            &invocation.args[invocation
                .args
                .iter()
                .position(|a| a == "--output")
                .unwrap()
                + 1],
        );
        let manifest = output
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("series.json");
        let pending = load_document(&manifest).unwrap();
        assert_eq!(
            require_series(&pending)
                .unwrap()
                .document
                .payload
                .attempts
                .last()
                .unwrap()
                .status,
            AttemptStatus::Running
        );
        assert!(!invocation
            .args
            .iter()
            .any(|a| a == "--repeat" || a == "--repeat-pause"));
        if self.cancel_at == Some(n) {
            cancellation.cancel();
            return Ok(ChildExit {
                success: false,
                code: None,
                cancelled: true,
            });
        }
        if self.fail_at == Some(n) {
            return Ok(ChildExit {
                success: false,
                code: Some(5),
                cancelled: false,
            });
        }
        if self.missing_at != Some(n) {
            let mut r = fixture();
            if !self.duplicate {
                r.timestamp = format!("2026-01-01T00:00:{:02}Z", n + self.timestamp_offset);
            }
            r.metadata = require_series(&pending)
                .unwrap()
                .document
                .payload
                .options
                .metadata
                .clone();
            if self.change_allocation && n == 2 {
                r.system.cpu.quota_cpus = Some(1.5);
            }
            save_result(&output, &r)?;
        }
        Ok(ChildExit {
            success: true,
            code: Some(0),
            cancelled: false,
        })
    }
    fn pause(&mut self, duration: Duration, _: &Cancellation) -> bool {
        self.pauses.push(duration);
        true
    }
}
fn cleanup(o: &RepeatOptions) {
    std::fs::remove_dir_all(o.output.parent().unwrap()).unwrap();
}
#[test]
fn three_runs_persist_incrementally_with_distinct_evidence_and_exact_pauses() {
    let o = options();
    let mut fake = Fake::new();
    let d = run_repeat(&o, &mut fake, &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(d.status, Validity::Valid);
    assert_eq!(fake.calls.len(), 3);
    assert_eq!(fake.pauses, vec![Duration::from_secs(60); 2]);
    let loaded = load_document(&o.output).unwrap();
    let series = require_series(&loaded).unwrap();
    assert!(series.children_verified);
    assert_eq!(series.runs.len(), 3);
    assert_eq!(series.cohorts[0].distinct_runs, 3);
    assert_eq!(series.cohorts[0].metrics["cpu_index"].count, 3);
    assert_eq!(o.spec.estimated_seconds(3, 60).unwrap(), 360);
    let mut full = o.spec.clone();
    full.mode = serverbench_core::Mode::Full;
    full.durations = full.mode.durations();
    assert_eq!(full.estimated_seconds(3, 60).unwrap(), 1605);
    assert!(run_repeat(&o, &mut fake, &Cancellation::default(), |_| {}).is_err());
    assert_eq!(fake.calls.len(), 3);
    cleanup(&o);
}
#[test]
fn failures_continuation_missing_files_and_duplicates_are_not_success() {
    for (continuation, missing, duplicate, expected) in [
        (false, false, false, 1),
        (true, false, false, 3),
        (false, true, false, 1),
        (true, false, true, 3),
    ] {
        let mut o = options();
        o.continue_on_error = continuation;
        let mut fake = Fake::new();
        if duplicate {
            fake.duplicate = true;
        } else if missing {
            fake.missing_at = Some(1);
        } else {
            fake.fail_at = Some(1);
        }
        let d = run_repeat(&o, &mut fake, &Cancellation::default(), |_| {}).unwrap();
        assert_eq!(d.status, Validity::Invalid);
        assert_eq!(fake.calls.len(), expected);
        let outcome = load_document(&o.output).unwrap();
        let s = require_series(&outcome).unwrap();
        if duplicate {
            assert_eq!(s.cohorts[0].distinct_runs, 1);
            assert_eq!(s.cohorts[0].duplicate_runs, 2);
        }
        cleanup(&o);
    }
}
#[test]
fn cancellation_retains_completed_runs_and_pending_manifest_is_readable() {
    let o = options();
    let mut fake = Fake::new();
    fake.cancel_at = Some(2);
    let d = run_repeat(&o, &mut fake, &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(d.status, Validity::Incomplete);
    assert!(!d.payload.finished);
    assert_eq!(d.payload.attempts[1].status, AttemptStatus::Cancelled);
    let outcome = load_document(&o.output).unwrap();
    assert_eq!(require_series(&outcome).unwrap().runs.len(), 1);
    cleanup(&o);
}
#[test]
fn allocation_changes_split_cohorts_and_tampered_children_are_rejected() {
    let o = options();
    let mut fake = Fake::new();
    fake.change_allocation = true;
    run_repeat(&o, &mut fake, &Cancellation::default(), |_| {}).unwrap();
    let outcome = load_document(&o.output).unwrap();
    let s = require_series(&outcome).unwrap();
    assert_eq!(s.cohorts.len(), 2);
    assert_eq!(
        s.document
            .payload
            .attempts
            .iter()
            .map(|a| a.ordinal)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let child = o
        .output
        .parent()
        .unwrap()
        .join(&s.document.payload.attempts[0].child_file);
    std::fs::write(child, b"{}").unwrap();
    assert!(load_document(&o.output).is_err());
    cleanup(&o);
}
#[test]
fn arguments_preserve_all_measured_options_and_verbosity_without_a_shell() {
    let mut o = options();
    o.spec.strict = true;
    o.spec.metadata.label = Some("--strange label; $(do not execute)".into());
    o.spec.metadata.provider = Some("Some provider".into());
    o.spec.metadata.monthly_cost = Some(49.95);
    o.spec.metadata.currency = Some("EUR".into());
    let call = child_invocation(&o, &PathBuf::from("with spaces/result.json"));
    let args = call
        .args
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for expected in [
        "--quick",
        "--strict",
        "--quiet",
        "--no-progress",
        "--label=--strange label; $(do not execute)",
        "--provider=Some provider",
        "--currency=EUR",
        "49.95",
        "with spaces/result.json",
    ] {
        assert!(args.iter().any(|v| v == expected), "{expected}");
    }
    assert!(!args.iter().any(|v| v == "--repeat"));
}
#[test]
fn reader_rejects_traversal_and_never_scores_a_series() {
    let o = options();
    run_repeat(&o, &mut Fake::new(), &Cancellation::default(), |_| {}).unwrap();
    let mut document = require_series(&load_document(&o.output).unwrap())
        .unwrap()
        .document
        .clone();
    assert!(matches!(
        serverbench_score::validate_document(&StoredDocument::RepeatSeries(Box::new(
            document.clone()
        )))
        .unwrap(),
        serverbench_score::ScoreDisposition::NotApplicable
    ));
    document.payload.attempts[0].child_file = "../unrelated.json".into();
    assert!(document.validate().is_err());
    document.payload.attempts[0].child_file = "series.runs/bad\nname.json".into();
    assert!(document.validate().is_err());
    cleanup(&o);
}

#[cfg(unix)]
#[test]
fn real_process_executor_launches_three_separate_children_and_reaps_them() {
    use std::os::unix::fs::PermissionsExt;
    let mut o = options();
    o.pause_seconds = 1;
    let parent = o.output.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    std::fs::write(
        parent.join("source.json"),
        include_bytes!("../../../tests/fixtures/results/legacy-v1-stable.json"),
    )
    .unwrap();
    o.executable = parent.join("child.sh");
    std::fs::write(
        &o.executable,
        br##"#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--output' ]; then shift; out="$1"; fi
  shift
done
printf '%s\n' "$$" >> "$0.pids"
name="${out##*/}"; index="${name#run-000}"; index="${index%.json}"
sed "s/00:00:00Z/00:00:0${index}Z/g" "$(dirname "$0")/source.json" > "$out"
"##,
    )
    .unwrap();
    std::fs::set_permissions(&o.executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let d = run_repeat(
        &o,
        &mut serverbench_runner::process::ProcessExecutor,
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(d.status, Validity::Valid);
    let pids = std::fs::read_to_string(o.executable.with_extension("sh.pids")).unwrap();
    let unique = pids.lines().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), 3);
    for pid in unique {
        let pid = pid.parse::<i32>().unwrap();
        let mut status = 0;
        // SAFETY: waitpid with WNOHANG observes only the named former child.
        assert_eq!(
            unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
    cleanup(&o);
}

#[cfg(unix)]
#[test]
fn process_cancellation_reaps_a_running_child_and_pause_is_interruptible() {
    use serverbench_runner::process::ProcessExecutor;
    use std::os::unix::fs::PermissionsExt;
    let o = options();
    let parent = o.output.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    let script = parent.join("wait.sh");
    std::fs::write(
        &script,
        b"#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$0.pid\"\nexec sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cancel = Cancellation::default();
    let c = cancel.clone();
    let marker = script.with_extension("sh.pid");
    let thread = std::thread::spawn(move || {
        for _ in 0..100 {
            if marker.exists() {
                c.cancel();
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("child never started");
    });
    let start = std::time::Instant::now();
    let exit = ProcessExecutor
        .execute(
            &Invocation {
                executable: script.clone(),
                args: vec![],
            },
            &cancel,
        )
        .unwrap();
    thread.join().unwrap();
    assert!(exit.cancelled);
    assert!(start.elapsed() < Duration::from_secs(5));
    let pid = std::fs::read_to_string(script.with_extension("sh.pid"))
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let mut status = 0;
    // SAFETY: only probes wait status for the child just cancelled by this test.
    assert_eq!(
        unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    assert!(!ProcessExecutor.pause(Duration::from_secs(60), &cancel));
    cleanup(&o);
}

#[test]
fn series_comparison_recomputes_summaries_and_requires_explicit_inference() {
    let mut a = options();
    a.count = 10;
    a.spec.metadata.label = Some("Server A".into());
    let mut b = options();
    b.count = 10;
    b.spec.metadata.label = Some("Server B".into());
    run_repeat(&a, &mut Fake::new(), &Cancellation::default(), |_| {}).unwrap();
    let mut fake = Fake::new();
    fake.timestamp_offset = 20;
    run_repeat(&b, &mut fake, &Cancellation::default(), |_| {}).unwrap();
    let descriptive = serverbench_report::compare_files(&a.output, &b.output).unwrap();
    assert!(descriptive.contains("Descriptive comparison only"));
    assert!(!descriptive.contains("Conditional inference"));
    assert!(descriptive.contains("10 distinct run(s)"));
    let conditional =
        serverbench_report::compare_files_with_uncertainty(&a.output, &b.output, true).unwrap();
    assert!(conditional.contains("joint coverage bound"));
    assert!(conditional.contains("cannot verify these assumptions"));
    let future = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/results/future-contract.json");
    let unsupported = serverbench_report::compare_files(&a.output, &future).unwrap();
    assert!(unsupported.contains("UnsupportedContract"));
    assert!(!unsupported.contains("999999"));
    let duplicate =
        serverbench_report::compare_files_with_uncertainty(&a.output, &a.output, true).unwrap();
    assert!(duplicate.contains("duplicate/shared"));
    assert!(duplicate.contains("Uncertainty unavailable"));
    let mut d = require_series(&load_document(&a.output).unwrap())
        .unwrap()
        .document
        .clone();
    d.payload.cohorts[0]
        .metrics
        .get_mut("cpu_index")
        .unwrap()
        .median = 999999.;
    serverbench_report::save_json(&a.output, &d, true).unwrap();
    let reloaded = load_document(&a.output).unwrap();
    assert_ne!(
        require_series(&reloaded).unwrap().cohorts[0].metrics["cpu_index"].median,
        999999.
    );
    cleanup(&a);
    cleanup(&b);
}

#[cfg(unix)]
#[test]
fn imported_bundle_cannot_follow_a_symlink_outside_its_directory() {
    let o = options();
    run_repeat(&o, &mut Fake::new(), &Cancellation::default(), |_| {}).unwrap();
    let d = require_series(&load_document(&o.output).unwrap())
        .unwrap()
        .document
        .clone();
    let child = o
        .output
        .parent()
        .unwrap()
        .join(&d.payload.attempts[0].child_file);
    let external = std::env::temp_dir().join(serverbench_report::unique_name("outside-series"));
    std::fs::copy(&child, &external).unwrap();
    std::fs::remove_file(&child).unwrap();
    std::os::unix::fs::symlink(&external, &child).unwrap();
    assert!(load_document(&o.output).is_err());
    cleanup(&o);
    std::fs::remove_file(external).unwrap();
}
#[test]
fn saved_running_manifest_fixture_is_readable_without_claiming_completion() {
    let outcome = serverbench_report::read_document(include_bytes!(
        "../../../tests/fixtures/results/repeat-v1-incomplete.json"
    ))
    .unwrap();
    let series = require_series(&outcome).unwrap();
    assert!(!series.children_verified);
    assert_eq!(series.document.status, Validity::Incomplete);
    assert_eq!(
        series.document.payload.attempts[0].status,
        AttemptStatus::Running
    );
    assert!(serverbench_report::series_console(series).contains("unverified"));
}

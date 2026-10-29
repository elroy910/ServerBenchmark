use crate::process::{Cancellation, ChildExecutor, Invocation};
use serverbench_analysis::repeat::{cohort_id, summarize, Observation};
use serverbench_core::{
    documents::{DocumentType, StoredDocument, Validity},
    series::{Attempt, AttemptReason, AttemptStatus, Document, Payload, RunSpec, PROTOCOL},
    v2::{Counter, ScoreStatus},
    Error, Result,
};
use serverbench_report::{load_document, save_json, timestamp, ReadOutcome, SeriesRun};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct RepeatOptions {
    pub executable: PathBuf,
    pub output: PathBuf,
    pub spec: RunSpec,
    pub count: u32,
    pub pause_seconds: u64,
    pub continue_on_error: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub no_progress: bool,
}
pub fn child_invocation(options: &RepeatOptions, output: &Path) -> Invocation {
    let mut args: Vec<OsString> = vec![
        "benchmark".into(),
        match options.spec.mode {
            serverbench_core::Mode::Quick => "--quick",
            serverbench_core::Mode::Full => "--full",
            serverbench_core::Mode::Extended => "--extended",
        }
        .into(),
        "--sustained".into(),
        options.spec.durations.sustained_seconds.to_string().into(),
        "--output".into(),
        output.as_os_str().into(),
    ];
    for (flag, value) in [
        ("--label", &options.spec.metadata.label),
        ("--provider", &options.spec.metadata.provider),
        ("--currency", &options.spec.metadata.currency),
    ] {
        if let Some(value) = value {
            args.push(format!("{flag}={value}").into());
        }
    }
    if let Some(cost) = options.spec.metadata.monthly_cost {
        args.extend([OsString::from("--monthly-cost"), cost.to_string().into()]);
    }
    for (flag, enabled) in [
        ("--strict", options.spec.strict),
        ("--anonymous", options.spec.metadata.anonymous),
        ("--quiet", options.quiet),
        ("--verbose", options.verbose),
        ("--no-progress", options.no_progress),
    ] {
        if enabled {
            args.push(flag.into());
        }
    }
    Invocation {
        executable: options.executable.clone(),
        args,
    }
}
/// Sequential, fresh-process attempts. Reserve outputs before launching work.
pub fn run_repeat(
    options: &RepeatOptions,
    executor: &mut impl ChildExecutor,
    cancel: &Cancellation,
    mut progress: impl FnMut(&str),
) -> Result<Document> {
    if !(2..=100).contains(&options.count)
        || options.pause_seconds == 0
        || options.pause_seconds > 31_536_000
    {
        return Err(Error::new(
            2,
            "repeat count must be 2–100; pause must be 1s–365d",
        ));
    }
    options.spec.validate()?;
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stem = options
        .output
        .file_stem()
        .and_then(|v| v.to_str())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Error::new(2, "series output needs a portable filename"))?;
    let directory_name = format!("{stem}.runs");
    if !serverbench_core::series::valid_child_path(&directory_name) {
        return Err(Error::new(2, "series output filename is not portable"));
    }
    let directory = parent.join(&directory_name);
    if options.output.exists() || directory.exists() {
        return Err(Error::new(
            2,
            "series output or child directory already exists; choose a new output",
        ));
    }
    fs::create_dir_all(parent)?;
    fs::create_dir(&directory)?;
    let mut document = Document {
        schema_version: 2,
        document_type: DocumentType::RepeatSeries,
        application_version: env!("CARGO_PKG_VERSION").into(),
        benchmark_schema_version: serverbench_core::BENCHMARK_SCHEMA,
        benchmark_contract_id: serverbench_core::CONTRACT.into(),
        protocol_id: PROTOCOL.into(),
        score_model_id: None,
        score_status: ScoreStatus::NotApplicable,
        status: Validity::Incomplete,
        timestamp: timestamp(),
        payload: Payload {
            requested_runs: options.count,
            pause_seconds: Counter(options.pause_seconds),
            continue_on_error: options.continue_on_error,
            options: options.spec.clone(),
            finished: false,
            attempts: vec![],
            cohorts: vec![],
        },
        scores: serde_json::Value::Null,
    };
    // Non-overwriting publication also detects a competing writer after preflight.
    save_json(&options.output, &document, false)?;
    let mut completed: Vec<SeriesRun> = Vec::new();
    for ordinal in 1..=options.count {
        if cancel.is_cancelled() {
            break;
        }
        let child_file = format!("{directory_name}/run-{ordinal:04}.json");
        let child_path = parent.join(&child_file);
        let mut attempt = Attempt {
            ordinal,
            child_file,
            requested: options.spec.clone(),
            started_at: timestamp(),
            finished_at: None,
            exit_code: None,
            status: AttemptStatus::Running,
            reason: None,
            exact_file_sha256: None,
            canonical_run_sha256: None,
            cohort_id: None,
        };
        document.payload.attempts.push(attempt.clone());
        save_json(&options.output, &document, true)?;
        progress(&format!("Repeat {ordinal}/{}", options.count));
        match executor.execute(&child_invocation(options, &child_path), cancel) {
            Err(_) => {
                attempt.status = if cancel.is_cancelled() {
                    AttemptStatus::Cancelled
                } else {
                    AttemptStatus::Failed
                };
                attempt.reason = Some(if cancel.is_cancelled() {
                    AttemptReason::Interrupted
                } else {
                    AttemptReason::SpawnFailed
                });
            }
            Ok(exit) => {
                attempt.exit_code = exit.code;
                if exit.cancelled || cancel.is_cancelled() {
                    attempt.status = AttemptStatus::Cancelled;
                    attempt.reason = Some(AttemptReason::Interrupted);
                } else if !exit.success {
                    attempt.status = AttemptStatus::Failed;
                    attempt.reason = Some(AttemptReason::ChildFailed);
                } else {
                    match load_document(&child_path) {
                        Ok(ReadOutcome::Supported(loaded)) => match loaded.document {
                            StoredDocument::LegacyBenchmarkV1(result)
                                if options.spec.matches(&result) =>
                            {
                                let id = cohort_id(&result)?;
                                let duplicate = completed.iter().any(|r| {
                                    r.fingerprints.canonical_run_sha256
                                        == loaded.fingerprints.canonical_run_sha256
                                });
                                attempt.status = AttemptStatus::Valid;
                                attempt.reason = duplicate.then_some(AttemptReason::DuplicateInput);
                                attempt.exact_file_sha256 =
                                    Some(loaded.fingerprints.exact_file_sha256.clone());
                                attempt.canonical_run_sha256 =
                                    Some(loaded.fingerprints.canonical_run_sha256.clone());
                                attempt.cohort_id = Some(id);
                                completed.push(SeriesRun {
                                    result,
                                    fingerprints: loaded.fingerprints,
                                });
                            }
                            StoredDocument::LegacyBenchmarkV1(_) => {
                                attempt.status = AttemptStatus::Invalid;
                                attempt.reason = Some(AttemptReason::OptionsMismatch);
                            }
                            _ => {
                                attempt.status = AttemptStatus::Invalid;
                                attempt.reason = Some(AttemptReason::InvalidResult);
                            }
                        },
                        _ => {
                            attempt.status = AttemptStatus::Invalid;
                            attempt.reason = Some(AttemptReason::InvalidResult);
                        }
                    }
                }
            }
        }
        attempt.finished_at = Some(timestamp());
        let success = attempt.status == AttemptStatus::Valid && attempt.reason.is_none();
        if !success {
            progress(&format!(
                "Repeat {ordinal}: {:?} ({:?})",
                attempt.status, attempt.reason
            ));
        }
        *document.payload.attempts.last_mut().unwrap() = attempt;
        document.payload.cohorts = summarize(
            &completed
                .iter()
                .map(|r| Observation {
                    result: &r.result,
                    fingerprints: &r.fingerprints,
                })
                .collect::<Vec<_>>(),
        )?;
        save_json(&options.output, &document, true)?;
        if cancel.is_cancelled() {
            break;
        }
        if !success && !options.continue_on_error {
            break;
        }
        if ordinal < options.count {
            progress(&format!("Repeat pause: {}s", options.pause_seconds));
            if !executor.pause(Duration::from_secs(options.pause_seconds), cancel) {
                break;
            }
        }
    }
    let all_success = document.payload.attempts.len() == options.count as usize
        && document
            .payload
            .attempts
            .iter()
            .all(|a| a.status == AttemptStatus::Valid && a.reason.is_none());
    if cancel.is_cancelled() {
        document.status = Validity::Incomplete;
        document.payload.finished = false;
    } else {
        document.status = if all_success {
            Validity::Valid
        } else {
            Validity::Invalid
        };
        document.payload.finished = true;
    }
    document.validate()?;
    save_json(&options.output, &document, true)?;
    Ok(document)
}

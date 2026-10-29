//! Validated document reader shared by file comparison and future offline reports.
use serverbench_core::{
    documents::{
        decode_document, ComparisonKey, DecodeOutcome, Fingerprints, StoredDocument,
        UnsupportedDocument,
    },
    Error, Result,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LoadedDocument {
    pub document: StoredDocument,
    pub fingerprints: Fingerprints,
    pub comparison_key: ComparisonKey,
}
#[derive(Debug, Clone)]
pub enum ReadOutcome {
    Supported(Box<LoadedDocument>),
    Unsupported(UnsupportedDocument),
    Series(Box<LoadedSeries>),
}
#[derive(Debug, Clone)]
pub struct SeriesRun {
    pub result: Box<serverbench_core::BenchmarkResult>,
    pub fingerprints: Fingerprints,
}
#[derive(Debug, Clone)]
pub struct LoadedSeries {
    pub document: serverbench_core::series::Document,
    pub runs: Vec<SeriesRun>,
    pub cohorts: Vec<serverbench_core::series::CohortSummary>,
    pub children_verified: bool,
}

pub fn read_document(bytes: &[u8]) -> Result<ReadOutcome> {
    match decode_document(bytes)? {
        DecodeOutcome::Unsupported(header) => Ok(ReadOutcome::Unsupported(header)),
        DecodeOutcome::Supported(document) => {
            serverbench_score::validate_document(&document)?;
            if let StoredDocument::RepeatSeries(series) = document {
                return Ok(ReadOutcome::Series(Box::new(LoadedSeries {
                    document: *series,
                    runs: vec![],
                    cohorts: vec![],
                    children_verified: false,
                })));
            }
            let comparison_key = ComparisonKey::from_document(&document)?;
            let fingerprints = Fingerprints::new(bytes, &document)?;
            Ok(ReadOutcome::Supported(Box::new(LoadedDocument {
                document,
                fingerprints,
                comparison_key,
            })))
        }
    }
}
pub fn load_document(path: &Path) -> Result<ReadOutcome> {
    let mut outcome = read_document(&std::fs::read(path)?)?;
    if let ReadOutcome::Series(series) = &mut outcome {
        use serverbench_core::series::AttemptStatus;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()?;
        for attempt in &series.document.payload.attempts {
            if attempt.status != AttemptStatus::Valid {
                continue;
            }
            let child = parent
                .join(&attempt.child_file)
                .canonicalize()
                .map_err(|_| Error::new(4, "successful series child is missing or unreadable"))?;
            if !child.starts_with(&parent) {
                return Err(Error::new(
                    4,
                    "series child escapes bundle (including symlinks)",
                ));
            }
            let ReadOutcome::Supported(loaded) = read_document(&std::fs::read(child)?)? else {
                return Err(Error::new(4, "series child is not a supported benchmark"));
            };
            let StoredDocument::LegacyBenchmarkV1(result) = loaded.document else {
                return Err(Error::new(4, "series child has incompatible contract"));
            };
            if !attempt.requested.matches(&result)
                || attempt.exact_file_sha256.as_ref()
                    != Some(&loaded.fingerprints.exact_file_sha256)
                || attempt.canonical_run_sha256.as_ref()
                    != Some(&loaded.fingerprints.canonical_run_sha256)
                || attempt.cohort_id.as_ref()
                    != Some(&serverbench_analysis::repeat::cohort_id(&result)?)
            {
                return Err(Error::new(4, "series child options or provenance mismatch"));
            }
            series.runs.push(SeriesRun {
                result,
                fingerprints: loaded.fingerprints,
            });
        }
        let observations = series
            .runs
            .iter()
            .map(|r| serverbench_analysis::repeat::Observation {
                result: &r.result,
                fingerprints: &r.fingerprints,
            })
            .collect::<Vec<_>>();
        series.cohorts = serverbench_analysis::repeat::summarize(&observations)?;
        let duplicate = series.cohorts.iter().any(|c| c.duplicate_runs > 0);
        if series.document.status == serverbench_core::documents::Validity::Valid && duplicate {
            return Err(Error::new(4, "duplicate runs in series declared valid"));
        }
        series.children_verified = true;
    }
    Ok(outcome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKind {
    ExactFile,
    CanonicalRun,
}
pub fn duplicate_kind(a: &LoadedDocument, b: &LoadedDocument) -> Option<DuplicateKind> {
    if a.fingerprints.exact_file_sha256 == b.fingerprints.exact_file_sha256 {
        Some(DuplicateKind::ExactFile)
    } else if a.fingerprints.canonical_run_sha256 == b.fingerprints.canonical_run_sha256 {
        Some(DuplicateKind::CanonicalRun)
    } else {
        None
    }
}
/// Count distinct valid runs only. This does not imply statistical independence.
pub fn distinct_run_count(documents: &[&LoadedDocument]) -> usize {
    documents
        .iter()
        .filter(|d| match &d.document {
            StoredDocument::RepeatSeries(_) => false,
            StoredDocument::LegacyBenchmarkV1(_) => true,
            StoredDocument::PreviewBenchmarkV2(r) => {
                r.status == serverbench_core::documents::Validity::Valid
            }
        })
        .map(|d| &d.fingerprints.canonical_run_sha256)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}
pub fn legacy_result(outcome: ReadOutcome) -> Result<serverbench_core::BenchmarkResult> {
    match outcome {
        ReadOutcome::Series(_) => Err(Error::new(
            4,
            "expected one legacy benchmark, got repeat series",
        )),
        ReadOutcome::Supported(document) => match document.document {
            StoredDocument::LegacyBenchmarkV1(r) => Ok(*r),
            _ => Err(Error::new(
                4,
                "expected legacy v1 benchmark; preview is uncalibrated",
            )),
        },
        ReadOutcome::Unsupported(d) => Err(Error::new(
            4,
            format!("unsupported document: {:?}", d.reason),
        )),
    }
}

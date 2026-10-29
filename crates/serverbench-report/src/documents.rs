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
}

pub fn read_document(bytes: &[u8]) -> Result<ReadOutcome> {
    match decode_document(bytes)? {
        DecodeOutcome::Unsupported(header) => Ok(ReadOutcome::Unsupported(header)),
        DecodeOutcome::Supported(document) => {
            serverbench_score::validate_document(&document)?;
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
    read_document(&std::fs::read(path)?)
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

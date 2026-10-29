//! Reuse the historical orchestrator without touching timed workers or scores.
pub use serverbench_survey::RunOptions;
pub fn run(
    options: &RunOptions,
    progress: impl FnMut(serverbench_core::ProgressEvent<'_>),
) -> serverbench_core::Result<serverbench_core::BenchmarkResult> {
    serverbench_survey::run(options, progress)
}

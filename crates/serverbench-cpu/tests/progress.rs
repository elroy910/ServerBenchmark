#![cfg(target_os = "linux")]
use serverbench_cpu::{idle_observed, workloads::Kind, Engine};
use std::{path::Path, time::Duration};

#[test]
fn observer_receives_the_same_samples_that_are_saved() {
    let allowed = serverbench_system::affinity().unwrap();
    let cpus = &allowed[..1];
    let engine = Engine {
        root: Path::new("/"),
        cpus,
        cgroup_dirs: &[],
        interval: Duration::from_millis(50),
    };
    let mut observed = Vec::new();
    let result = engine
        .measure_observed(Kind::Integer, 1, Duration::from_millis(200), |s| {
            observed.push(s.elapsed_seconds)
        })
        .unwrap();
    assert!(!observed.is_empty());
    assert_eq!(
        observed,
        result
            .samples
            .iter()
            .map(|s| s.elapsed_seconds)
            .collect::<Vec<_>>()
    );
    assert!(result.completed_batches > 0);
    assert!(result.checksums_valid);
    let mut idle_count = 0;
    let samples = idle_observed(
        engine.root,
        cpus,
        &[],
        Duration::from_millis(100),
        engine.interval,
        |_| idle_count += 1,
    );
    assert_eq!(samples.len(), idle_count);
    assert!(idle_count > 0);
}

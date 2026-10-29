pub mod workloads;
use serverbench_core::{statistics, Error, Measurement, Result, Sample};
use serverbench_metrics::Sampler;
use serverbench_system::pin;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Barrier, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use workloads::{Kind, Workload};

pub fn idle(
    root: &Path,
    cpus: &[usize],
    dirs: &[PathBuf],
    duration: Duration,
    interval: Duration,
) -> Vec<Sample> {
    idle_observed(root, cpus, dirs, duration, interval, |_| {})
}

pub fn idle_observed(
    root: &Path,
    cpus: &[usize],
    dirs: &[PathBuf],
    duration: Duration,
    interval: Duration,
    mut observer: impl FnMut(&Sample),
) -> Vec<Sample> {
    let start = Instant::now();
    let mut sampler = Sampler::new(root, cpus, dirs);
    let mut out = Vec::new();
    while start.elapsed() < duration {
        thread::sleep(interval.min(duration.saturating_sub(start.elapsed())));
        let sample = sampler.sample(start.elapsed().as_secs_f64(), None);
        observer(&sample);
        out.push(sample);
    }
    out
}

pub struct Engine<'a> {
    pub root: &'a Path,
    pub cpus: &'a [usize],
    pub cgroup_dirs: &'a [PathBuf],
    pub interval: Duration,
}
impl Engine<'_> {
    pub fn measure(&self, kind: Kind, workers: usize, duration: Duration) -> Result<Measurement> {
        self.measure_observed(kind, workers, duration, |_| {})
    }

    pub fn measure_observed(
        &self,
        kind: Kind,
        workers: usize,
        duration: Duration,
        mut observer: impl FnMut(&Sample),
    ) -> Result<Measurement> {
        if workers == 0
            || workers > self.cpus.len()
            || duration.is_zero()
            || self.interval.is_zero()
        {
            return Err(Error::new(4, "invalid engine configuration"));
        }
        // All allocation, dataset generation and compression setup precede timing.
        let states = (0..workers)
            .map(|_| Workload::new())
            .collect::<Result<Vec<_>>>()?;
        let counts: Vec<_> = (0..workers).map(|_| AtomicU64::new(0)).collect();
        let barrier = Arc::new(Barrier::new(workers + 1));
        let start = Arc::new(OnceLock::<Instant>::new());
        let stop = AtomicBool::new(false);
        let mut samples = Vec::new();
        let results = thread::scope(|scope| {
            let mut handles = Vec::new();
            for (worker, mut state) in states.into_iter().enumerate() {
                let barrier = barrier.clone();
                let clock = start.clone();
                let counts = &counts;
                let stop = &stop;
                handles.push(scope.spawn(move || {
                    let pinned = pin(self.cpus[worker]);
                    barrier.wait();
                    let started = *clock
                        .get()
                        .expect("coordinator initializes clock before barrier");
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut batches = 0u64;
                        while started.elapsed() < duration && !stop.load(Ordering::Relaxed) {
                            state.execute(kind)?;
                            batches += 1;
                            counts[worker].store(batches, Ordering::Relaxed);
                        }
                        Ok::<_, Error>(batches)
                    }));
                    let done = Instant::now();
                    let outcome =
                        result.unwrap_or_else(|_| Err(Error::new(4, "benchmark worker panicked")));
                    if outcome.is_err() {
                        stop.store(true, Ordering::Relaxed);
                    }
                    (outcome, pinned, done)
                }));
            }
            let mut sampler = Sampler::new(self.root, self.cpus, self.cgroup_dirs);
            let began = Instant::now();
            start.set(began).expect("new measurement clock");
            barrier.wait();
            let mut previous = 0u64;
            let mut previous_time = began;
            while began.elapsed() < duration && !stop.load(Ordering::Relaxed) {
                thread::sleep(self.interval.min(duration.saturating_sub(began.elapsed())));
                let now = Instant::now();
                let count = counts
                    .iter()
                    .map(|c| c.load(Ordering::Relaxed))
                    .sum::<u64>();
                let rate = (count - previous) as f64 * kind.units() as f64
                    / now.duration_since(previous_time).as_secs_f64();
                let sample = sampler.sample(now.duration_since(began).as_secs_f64(), Some(rate));
                observer(&sample);
                samples.push(sample);
                previous = count;
                previous_time = now;
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| Error::new(4, "worker orchestration panicked"))
                })
                .collect::<Result<Vec<_>>>()
        })?;
        let began = *start.get().unwrap();
        let mut batches = 0u64;
        let mut affinity = Vec::new();
        let mut last = began;
        let mut first = None;
        for (result, pinned, finished) in results {
            batches += result?;
            affinity.push(pinned);
            last = last.max(finished);
            first = Some(first.map_or(finished, |old: Instant| old.min(finished)));
        }
        let elapsed = last.duration_since(began).as_secs_f64();
        let units = batches
            .checked_mul(kind.units())
            .ok_or_else(|| Error::new(4, "unit counter overflow"))?;
        if batches == 0 || samples.is_empty() {
            return Err(Error::new(4, "benchmark completed no work"));
        }
        // Only near-full windows enter statistics; keep partial tail samples in raw JSON.
        let rates: Vec<_> = samples
            .iter()
            .filter(|s| s.interval_seconds >= self.interval.as_secs_f64() * 0.9)
            .filter_map(|s| s.throughput)
            .collect();
        if rates.is_empty() {
            return Err(Error::new(4, "no complete measurement windows"));
        }
        Ok(Measurement {
            workload: kind.name().into(),
            unit: kind.unit().into(),
            workers,
            elapsed_seconds: elapsed,
            completed_units: units,
            completed_batches: batches,
            total_throughput: units as f64 / elapsed,
            throughput_per_vcpu: units as f64 / elapsed / workers as f64,
            scaling_efficiency: None,
            throughput: statistics(&rates),
            samples,
            affinity_applied: affinity,
            checksums_valid: true,
            worker_finish_skew_seconds: last.duration_since(first.unwrap()).as_secs_f64(),
        })
    }
}

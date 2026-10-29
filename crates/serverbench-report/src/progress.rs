use serverbench_core::{ProgressEvent, Sample};
use std::io::{self, Write};
use std::{
    sync::mpsc::{sync_channel, SyncSender},
    thread::{self, JoinHandle},
};

enum OwnedEvent {
    Message(String),
    Sample {
        phase: String,
        duration_seconds: f64,
        sample: Box<Sample>,
    },
}
/// Bounded, nonblocking delivery keeps slow SSH/log output off the sampler.
pub struct ProgressOutput {
    sender: Option<SyncSender<OwnedEvent>>,
    worker: Option<JoinHandle<()>>,
    live: bool,
}
impl ProgressOutput {
    pub fn new(enabled: bool, live: bool) -> Self {
        if !enabled {
            return Self {
                sender: None,
                worker: None,
                live,
            };
        }
        let (sender, receiver) = sync_channel::<OwnedEvent>(8);
        let worker = thread::spawn(move || {
            let mut reporter = ProgressReporter::new(io::stderr(), true, live);
            for event in receiver {
                let result = match event {
                    OwnedEvent::Message(message) => {
                        reporter.handle(ProgressEvent::Message(&message))
                    }
                    OwnedEvent::Sample {
                        phase,
                        duration_seconds,
                        sample,
                    } => reporter.handle(ProgressEvent::Sample {
                        phase: &phase,
                        duration_seconds,
                        sample: &sample,
                    }),
                };
                if result.is_err() {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            worker: Some(worker),
            live,
        }
    }
    pub fn handle(&self, event: ProgressEvent<'_>) {
        let Some(sender) = &self.sender else { return };
        let event = match event {
            ProgressEvent::Message(message) => OwnedEvent::Message(message.into()),
            ProgressEvent::Sample {
                phase,
                duration_seconds,
                sample,
            } if self.live => OwnedEvent::Sample {
                phase: phase.into(),
                duration_seconds,
                sample: Box::new(sample.clone()),
            },
            ProgressEvent::Sample { .. } => return,
        };
        // Drop stale updates if output cannot keep up; never block measurement.
        let _ = sender.try_send(event);
    }
    pub fn finish(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for ProgressOutput {
    fn drop(&mut self) {
        self.finish();
    }
}

fn number(value: Option<f64>, suffix: &str) -> String {
    value
        .map(|v| format!("{v:.1}{suffix}"))
        .unwrap_or_else(|| "unknown".into())
}
pub fn live_line(phase: &str, duration: f64, s: &Sample) -> String {
    let memory = s
        .memory
        .as_ref()
        .map(|m| {
            let gib = 1024_f64.powi(3);
            let mut text = format!(
                "{:.1}/{:.1} GiB ({:.0}%)",
                m.host_used_bytes as f64 / gib,
                m.host_total_bytes as f64 / gib,
                100. * m.host_used_bytes as f64 / m.host_total_bytes as f64
            );
            if let Some(limit) = m.cgroup_limit_bytes {
                let used = m
                    .cgroup_used_bytes
                    .map(|v| format!("{:.1}", v as f64 / gib))
                    .unwrap_or_else(|| "unknown".into());
                text.push_str(&format!("; cgroup {used}/{:.1} GiB", limit as f64 / gib));
            }
            text
        })
        .unwrap_or_else(|| "unknown".into());
    let load = s
        .load_average
        .map(|v| format!("{:.2}/{:.2}/{:.2}", v[0], v[1], v[2]))
        .unwrap_or_else(|| "unknown".into());
    format!("{phase} [{:.0}/{duration:.0}s] CPU {} | RAM {memory} | CPU temp {} | Load {load} | Steal {} | Freq {}",
        s.elapsed_seconds,number(s.cpu_utilization_percent,"%"),number(s.cpu_temperature_celsius," C"),number(s.steal_percent,"%"),number(s.frequency_mhz," MHz"))
}

/// Plain-text stderr updates remain readable in SSH sessions and redirected logs.
pub struct ProgressReporter<W: Write> {
    writer: W,
    enabled: bool,
    live: bool,
    phase: String,
    last_elapsed: Option<f64>,
}
impl<W: Write> ProgressReporter<W> {
    pub fn new(writer: W, enabled: bool, live: bool) -> Self {
        Self {
            writer,
            enabled,
            live,
            phase: String::new(),
            last_elapsed: None,
        }
    }
    pub fn handle(&mut self, event: ProgressEvent<'_>) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        match event {
            ProgressEvent::Message(message) => {
                self.last_elapsed = None;
                writeln!(self.writer, "{message}")?;
            }
            ProgressEvent::Sample {
                phase,
                duration_seconds,
                sample,
            } if self.live => {
                if self.phase != phase {
                    self.phase = phase.into();
                    self.last_elapsed = None;
                }
                let due = self
                    .last_elapsed
                    .is_none_or(|last| sample.elapsed_seconds - last >= 5.)
                    || sample.elapsed_seconds >= duration_seconds;
                if due {
                    writeln!(
                        self.writer,
                        "{}",
                        live_line(phase, duration_seconds, sample)
                    )?;
                    self.last_elapsed = Some(sample.elapsed_seconds);
                }
            }
            ProgressEvent::Sample { .. } => {}
        }
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Sample {
        Sample {
            elapsed_seconds: 1.,
            interval_seconds: 1.,
            throughput: None,
            cpu_utilization_percent: Some(75.),
            steal_percent: Some(2.),
            context_switches_per_second: None,
            load_average: Some([1., 2., 3.]),
            frequency_mhz: None,
            cgroup_throttled_seconds: None,
            memory: None,
            cpu_temperature_celsius: None,
        }
    }
    #[test]
    fn unknown_values_are_explicit() {
        let line = live_line("Warm-up", 15., &sample());
        assert!(line.contains("CPU 75.0%"));
        assert!(line.contains("RAM unknown"));
        assert!(line.contains("CPU temp unknown"));
        assert!(line.contains("Load 1.00/2.00/3.00"));
    }
    #[test]
    fn updates_are_throttled_but_final_sample_is_shown() {
        let mut r = ProgressReporter::new(Vec::new(), true, true);
        let mut s = sample();
        for elapsed in [1., 2., 3., 4., 5., 6., 7., 8.] {
            s.elapsed_seconds = elapsed;
            r.handle(ProgressEvent::Sample {
                phase: "Sustained",
                duration_seconds: 8.,
                sample: &s,
            })
            .unwrap();
        }
        let text = String::from_utf8(r.writer).unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.contains("[8/8s]"));
        assert!(!text.contains('\u{1b}'));
    }
    #[test]
    fn quiet_and_no_progress_respected() {
        let s = sample();
        for (enabled, live, expected) in [(false, true, ""), (true, false, "Phase\n")] {
            let mut r = ProgressReporter::new(Vec::new(), enabled, live);
            r.handle(ProgressEvent::Message("Phase")).unwrap();
            r.handle(ProgressEvent::Sample {
                phase: "Phase",
                duration_seconds: 10.,
                sample: &s,
            })
            .unwrap();
            assert_eq!(String::from_utf8(r.writer).unwrap(), expected);
        }
    }
}

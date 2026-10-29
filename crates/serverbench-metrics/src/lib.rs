use serverbench_core::Sample;
mod telemetry;
use serverbench_system::{frequency, load, read};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, Clone, Default)]
pub struct CpuTicks {
    pub values: [u64; 8],
    pub has_steal: bool,
}
#[derive(Debug, Clone, Default)]
pub struct ProcStat {
    pub cpus: BTreeMap<usize, CpuTicks>,
    pub context_switches: Option<u64>,
}
pub fn parse_proc_stat(text: &str) -> ProcStat {
    let mut out = ProcStat::default();
    for line in text.lines() {
        let mut p = line.split_whitespace();
        let Some(name) = p.next() else { continue };
        if name == "ctxt" {
            out.context_switches = p.next().and_then(|s| s.parse().ok());
            continue;
        }
        let Some(id) = name
            .strip_prefix("cpu")
            .and_then(|s| s.parse::<usize>().ok())
        else {
            continue;
        };
        let parsed: Option<Vec<u64>> = p.map(|s| s.parse().ok()).collect();
        let Some(v) = parsed.filter(|v| v.len() >= 4) else {
            continue;
        };
        let mut values = [0; 8];
        for (a, b) in values.iter_mut().zip(&v) {
            *a = *b;
        }
        out.cpus.insert(
            id,
            CpuTicks {
                values,
                has_steal: v.len() >= 8,
            },
        );
    }
    out
}

/// Never double-count guest/guest_nice: they are included in user/nice.
pub fn cpu_delta(a: &ProcStat, b: &ProcStat, cpus: &[usize]) -> (Option<f64>, Option<f64>) {
    let mut total = 0u128;
    let mut idle = 0u128;
    let mut steal = 0u128;
    let mut has_steal = true;
    for id in cpus {
        let (Some(a), Some(b)) = (a.cpus.get(id), b.cpus.get(id)) else {
            return (None, None);
        };
        let mut delta = [0u64; 8];
        for (i, v) in delta.iter_mut().enumerate() {
            let Some(d) = b.values[i].checked_sub(a.values[i]) else {
                return (None, None);
            };
            *v = d;
        }
        total += delta.iter().map(|v| *v as u128).sum::<u128>();
        idle += delta[3] as u128 + delta[4] as u128;
        steal += delta[7] as u128;
        has_steal &= a.has_steal && b.has_steal;
    }
    if total == 0 {
        return (None, None);
    }
    (
        Some((total - idle - steal) as f64 / total as f64 * 100.),
        has_steal.then_some(steal as f64 / total as f64 * 100.),
    )
}
fn throttled(dir: &Path) -> Option<f64> {
    let s = std::fs::read_to_string(dir.join("cpu.stat")).ok()?;
    s.lines().find_map(|line| {
        let mut p = line.split_whitespace();
        let k = p.next()?;
        let n = p.next()?.parse::<f64>().ok()?;
        match k {
            "throttled_usec" => Some(n / 1e6),
            "throttled_time" => Some(n / 1e9),
            _ => None,
        }
    })
}
pub struct Sampler {
    root: PathBuf,
    cpus: Vec<usize>,
    dirs: Vec<PathBuf>,
    previous: Option<ProcStat>,
    throttled: Vec<Option<f64>>,
    last: Instant,
    temperatures: telemetry::Temperatures,
}
impl Sampler {
    pub fn new(root: &Path, cpus: &[usize], dirs: &[PathBuf]) -> Self {
        Self {
            root: root.into(),
            cpus: cpus.into(),
            dirs: dirs.into(),
            previous: read(root, "/proc/stat").map(|s| parse_proc_stat(&s)),
            throttled: dirs.iter().map(|d| throttled(d)).collect(),
            last: Instant::now(),
            temperatures: telemetry::Temperatures::discover(root),
        }
    }
    pub fn sample(&mut self, elapsed: f64, throughput: Option<f64>) -> Sample {
        let now = Instant::now();
        let interval = now.duration_since(self.last).as_secs_f64();
        let next = read(&self.root, "/proc/stat").map(|s| parse_proc_stat(&s));
        let (usage, steal) = self
            .previous
            .as_ref()
            .zip(next.as_ref())
            .map_or((None, None), |(a, b)| cpu_delta(a, b, &self.cpus));
        let ctxt = self
            .previous
            .as_ref()
            .and_then(|p| p.context_switches)
            .zip(next.as_ref().and_then(|p| p.context_switches))
            .and_then(|(a, b)| b.checked_sub(a))
            .map(|d| d as f64 / interval);
        let next_throttle: Vec<_> = self.dirs.iter().map(|d| throttled(d)).collect();
        let throttle = self
            .throttled
            .iter()
            .zip(&next_throttle)
            .filter_map(|(a, b)| a.zip(*b).and_then(|(a, b)| (b >= a).then_some(b - a)))
            .max_by(f64::total_cmp);
        self.previous = next;
        self.throttled = next_throttle;
        self.last = now;
        Sample {
            elapsed_seconds: elapsed,
            interval_seconds: interval,
            throughput,
            cpu_utilization_percent: usage,
            steal_percent: steal,
            context_switches_per_second: ctxt,
            load_average: load(&self.root),
            frequency_mhz: self.cpus.first().and_then(|c| frequency(&self.root, *c)),
            cgroup_throttled_seconds: throttle,
            memory: telemetry::memory(&self.root, &self.dirs),
            cpu_temperature_celsius: self.temperatures.maximum_celsius(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steal_and_guest_accounting() {
        let a = parse_proc_stat("cpu0 100 0 50 800 0 0 0 50 20 0\nctxt 200\n");
        let b = parse_proc_stat("cpu0 120 0 60 860 0 0 0 60 30 0\nctxt 230\n");
        assert_eq!(cpu_delta(&a, &b, &[0]), (Some(30.), Some(10.)));
        assert_eq!(b.context_switches, Some(230));
        assert_eq!(cpu_delta(&a, &b, &[1]), (None, None));
        assert_eq!(cpu_delta(&b, &a, &[0]), (None, None));
    }
    #[test]
    fn unavailable_steal_is_null() {
        let a = parse_proc_stat("cpu0 1 0 1 10");
        let b = parse_proc_stat("cpu0 2 0 2 20");
        assert_eq!(cpu_delta(&a, &b, &[0]).1, None);
        let a = parse_proc_stat("cpu0 x 0 0 0");
        assert!(a.cpus.is_empty());
    }
}

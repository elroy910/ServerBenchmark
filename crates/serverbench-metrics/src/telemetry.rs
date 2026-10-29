use serverbench_core::MemoryUsage;
use serverbench_system::{memory_value, read};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn memory(root: &Path, dirs: &[PathBuf]) -> Option<MemoryUsage> {
    let text = read(root, "/proc/meminfo")?;
    let total = memory_value(&text, "MemTotal")?;
    let available = memory_value(&text, "MemAvailable")?;
    if total == 0 || available > total {
        return None;
    }
    let mut limit = None;
    let mut used = None;
    for dir in dirs {
        let value = |name: &str| {
            fs::read_to_string(dir.join(name))
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()
        };
        let candidate = value("memory.max").or_else(|| value("memory.limit_in_bytes"));
        // Choose the tightest visible finite ancestor, and pair its own usage.
        if let Some(candidate) = candidate.filter(|v| *v > 0 && *v < (1u64 << 60)) {
            if limit.is_none_or(|old| candidate < old) {
                limit = Some(candidate);
                used = value("memory.current").or_else(|| value("memory.usage_in_bytes"));
            }
        }
    }
    Some(MemoryUsage {
        host_total_bytes: total,
        host_used_bytes: total - available,
        cgroup_used_bytes: used,
        cgroup_limit_bytes: limit,
    })
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let mut result: Vec<_> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    result.sort();
    result
}
fn text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}
fn cpu_type(name: &str) -> bool {
    matches!(
        name,
        "x86_pkg_temp" | "cpu-thermal" | "cpu_thermal" | "cpu-thermal0" | "cpu-thermal1"
    )
}

/// Cache only recognized CPU sensors, never arbitrary NVMe/GPU/ACPI sensors.
pub struct Temperatures {
    paths: Vec<PathBuf>,
}
impl Temperatures {
    pub fn discover(root: &Path) -> Self {
        let mut paths = Vec::new();
        for dir in entries(&root.join("sys/class/hwmon")) {
            let name = text(&dir.join("name")).unwrap_or_default();
            if !matches!(name.as_str(), "coretemp" | "k10temp" | "zenpower") && !cpu_type(&name) {
                continue;
            }
            let mut inputs = Vec::new();
            for path in entries(&dir) {
                let Some(file) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                let Some(index) = file
                    .strip_prefix("temp")
                    .and_then(|s| s.strip_suffix("_input"))
                else {
                    continue;
                };
                if index.is_empty() || !index.bytes().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let label = text(&dir.join(format!("temp{index}_label"))).unwrap_or_default();
                inputs.push((path, label));
            }
            let has_die = inputs.iter().any(|(_, label)| label == "Tdie");
            paths.extend(
                inputs
                    .into_iter()
                    .filter(|(_, label)| !has_die || label != "Tctl")
                    .map(|(path, _)| path),
            );
        }
        // Use thermal zones only when hwmon did not identify any CPU sensors.
        if paths.is_empty() {
            for dir in entries(&root.join("sys/class/thermal")) {
                if !dir
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("thermal_zone"))
                {
                    continue;
                }
                if text(&dir.join("type")).is_some_and(|name| cpu_type(&name)) {
                    paths.push(dir.join("temp"));
                }
            }
        }
        Self { paths }
    }
    pub fn maximum_celsius(&self) -> Option<f64> {
        self.paths
            .iter()
            .filter_map(|path| {
                let value = text(path)?.parse::<i64>().ok()? as f64 / 1000.;
                // Reject unplugged sensor sentinels and implausible readings.
                (0. ..=150.).contains(&value).then_some(value)
            })
            .max_by(f64::total_cmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "serverbench-telemetry-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn put(&self, path: &str, value: &str) {
            let p = self.0.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, value).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn memory_uses_available_and_tightest_ancestor() {
        let f = Fixture::new();
        f.put("proc/meminfo", "MemTotal: 8192 kB\nMemAvailable: 6144 kB\n");
        f.put("cg/child/memory.max", "4194304");
        f.put("cg/child/memory.current", "1048576");
        f.put("cg/memory.max", "2097152");
        f.put("cg/memory.current", "1572864");
        let m = memory(&f.0, &[f.0.join("cg/child"), f.0.join("cg")]).unwrap();
        assert_eq!(m.host_used_bytes, 2097152);
        assert_eq!(m.cgroup_limit_bytes, Some(2097152));
        assert_eq!(m.cgroup_used_bytes, Some(1572864));
    }
    #[test]
    fn cpu_sensors_exclude_gpu_and_prefer_die_over_control_offset() {
        let f = Fixture::new();
        f.put("sys/class/hwmon/hwmon0/name", "nvme");
        f.put("sys/class/hwmon/hwmon0/temp1_input", "120000");
        f.put("sys/class/hwmon/hwmon1/name", "k10temp");
        f.put("sys/class/hwmon/hwmon1/temp1_label", "Tctl");
        f.put("sys/class/hwmon/hwmon1/temp1_input", "85000");
        f.put("sys/class/hwmon/hwmon1/temp2_label", "Tdie");
        f.put("sys/class/hwmon/hwmon1/temp2_input", "65000");
        assert_eq!(Temperatures::discover(&f.0).maximum_celsius(), Some(65.));
        f.put("sys/class/hwmon/hwmon1/temp2_input", "-128000");
        assert_eq!(Temperatures::discover(&f.0).maximum_celsius(), None);
    }
    #[test]
    fn thermal_fallback_and_missing_sensor() {
        let f = Fixture::new();
        assert_eq!(Temperatures::discover(&f.0).maximum_celsius(), None);
        assert!(memory(&f.0, &[]).is_none());
        f.put("sys/class/thermal/thermal_zone0/type", "acpitz");
        f.put("sys/class/thermal/thermal_zone0/temp", "95000");
        f.put("sys/class/thermal/thermal_zone1/type", "cpu-thermal");
        f.put("sys/class/thermal/thermal_zone1/temp", "43250");
        assert_eq!(Temperatures::discover(&f.0).maximum_celsius(), Some(43.25));
    }
}

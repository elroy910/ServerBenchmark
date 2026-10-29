use serverbench_core::{CpuInfo, Error, Result, SystemInfo};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub fn read(root: &Path, file: &str) -> Option<String> {
    fs::read_to_string(root.join(file.trim_start_matches('/')))
        .ok()
        .map(|v| v.trim().to_owned())
}
pub fn cpu_list(s: &str) -> Option<Vec<usize>> {
    let mut out = BTreeSet::new();
    for part in s.trim().split(',').filter(|s| !s.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?),
            None => {
                let a = part.parse().ok()?;
                (a, a)
            }
        };
        if b < a || b > 1_048_576 {
            return None;
        }
        out.extend(a..=b);
    }
    Some(out.into_iter().collect())
}
pub fn quota(s: &str) -> Option<f64> {
    let mut p = s.split_whitespace();
    let q = p.next()?;
    let period = p.next()?.parse::<f64>().ok()?;
    if q == "max" || period <= 0.0 {
        return None;
    }
    let q = q.parse::<f64>().ok()?;
    (q > 0.0 && q.is_finite() && period.is_finite()).then_some(q / period)
}

#[derive(Debug, Default)]
pub struct CgroupLimits {
    pub version: Option<u8>,
    pub quota: Option<f64>,
    pub cpuset: Option<Vec<usize>>,
    pub memory_limit: Option<u64>,
    pub memory_headroom: Option<u64>,
    pub directories: Vec<PathBuf>,
    pub complete: bool,
}
fn min_option<T: PartialOrd + Copy>(value: &mut Option<T>, new: T) {
    if value.is_none_or(|old| new < old) {
        *value = Some(new);
    }
}
fn unescape_mount(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\134", "\\")
}

/// Resolve membership against mount root, then inspect every visible ancestor.
/// A namespace can hide host ancestors; that limitation is explicitly reported.
pub fn cgroup_limits(root: &Path) -> CgroupLimits {
    let mut limits = CgroupLimits {
        complete: true,
        ..Default::default()
    };
    let membership = read(root, "/proc/self/cgroup").unwrap_or_default();
    let mountinfo = read(root, "/proc/self/mountinfo").unwrap_or_default();
    for line in membership.lines() {
        let parts: Vec<_> = line.splitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let v2 = parts[0] == "0" && parts[1].is_empty();
        let controllers: Vec<_> = parts[1].split(',').collect();
        if !v2
            && !controllers
                .iter()
                .any(|c| ["cpu", "cpuset", "memory"].contains(c))
        {
            continue;
        }
        limits.version = Some(if v2 { 2 } else { 1 });
        let mut found = false;
        for mount in mountinfo.lines() {
            let Some((before, after)) = mount.split_once(" - ") else {
                continue;
            };
            let b: Vec<_> = before.split_whitespace().collect();
            let a: Vec<_> = after.split_whitespace().collect();
            if b.len() < 5 || a.len() < 3 {
                continue;
            }
            if (v2 && a[0] != "cgroup2")
                || (!v2 && (a[0] != "cgroup" || !a[2].split(',').any(|c| controllers.contains(&c))))
            {
                continue;
            }
            let mount_root = unescape_mount(b[3]);
            let mount_point = unescape_mount(b[4]);
            let member = Path::new(parts[2]);
            // cgroup namespaces can report '/' relative to a non-root mount.
            let relative = if parts[2] == "/" {
                Path::new("")
            } else if let Ok(p) = member.strip_prefix(&mount_root) {
                p
            } else if mount_root != "/" {
                // Membership is relative to a cgroup namespace, while mountinfo
                // can retain the host-side mount root.
                limits.complete = false;
                member.strip_prefix("/").unwrap_or(member)
            } else {
                limits.complete = false;
                continue;
            };
            if relative
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                limits.complete = false;
                continue;
            }
            if mount_root != "/" || parts[2] == "/" {
                limits.complete = false;
            }
            let mount_dir = root.join(mount_point.trim_start_matches('/'));
            let mut dir = mount_dir.join(relative);
            if !dir.is_dir() {
                limits.complete = false;
            }
            found = true;
            loop {
                if !limits.directories.contains(&dir) {
                    limits.directories.push(dir.clone());
                }
                let get = |name: &str| fs::read_to_string(dir.join(name)).ok();
                let q = if v2 {
                    get("cpu.max").and_then(|s| quota(&s))
                } else {
                    get("cpu.cfs_quota_us")
                        .zip(get("cpu.cfs_period_us"))
                        .and_then(|(q, p)| quota(&format!("{} {}", q.trim(), p.trim())))
                };
                if let Some(q) = q {
                    min_option(&mut limits.quota, q);
                }
                let set = if v2 {
                    get("cpuset.cpus.effective")
                } else {
                    get("cpuset.effective_cpus").or_else(|| get("cpuset.cpus"))
                };
                if let Some(set) = set.and_then(|s| cpu_list(&s)).filter(|s| !s.is_empty()) {
                    limits.cpuset = Some(match limits.cpuset.take() {
                        Some(old) => old.into_iter().filter(|c| set.contains(c)).collect(),
                        None => set,
                    });
                }
                let mem = if v2 {
                    get("memory.max")
                } else {
                    get("memory.limit_in_bytes")
                };
                let used = if v2 {
                    get("memory.current")
                } else {
                    get("memory.usage_in_bytes")
                };
                if let Some(mem) = mem
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .filter(|m| *m < (1u64 << 60))
                {
                    min_option(&mut limits.memory_limit, mem);
                    if let Some(used) = used.and_then(|s| s.trim().parse::<u64>().ok()) {
                        min_option(&mut limits.memory_headroom, mem.saturating_sub(used));
                    }
                }
                if dir == mount_dir {
                    break;
                }
                if !dir.pop() || !dir.starts_with(&mount_dir) {
                    limits.complete = false;
                    break;
                }
            }
        }
        if !found {
            limits.complete = false;
        }
    }
    limits
}

#[cfg(target_os = "linux")]
pub fn affinity() -> Result<Vec<usize>> {
    // Dynamically sized kernel affinity bitmap; supports CPU IDs beyond CPU_SETSIZE.
    let mut words = vec![0usize; 16];
    loop {
        let bytes = words.len() * std::mem::size_of::<usize>();
        // SAFETY: words is a writable, aligned bitmap of exactly bytes bytes.
        let rc = unsafe { libc::sched_getaffinity(0, bytes, words.as_mut_ptr().cast()) };
        if rc == 0 {
            return Ok((0..bytes * 8)
                .filter(|i| {
                    words[i / usize::BITS as usize] & (1usize << (i % usize::BITS as usize)) != 0
                })
                .collect());
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL)
            || words.len() >= 16_384
        {
            return Err(Error::new(5, "cannot read process CPU affinity"));
        }
        words.resize(words.len() * 2, 0);
    }
}
#[cfg(not(target_os = "linux"))]
pub fn affinity() -> Result<Vec<usize>> {
    Err(Error::new(
        3,
        "ServerBench requires Linux x86_64 or aarch64",
    ))
}

#[cfg(target_os = "linux")]
pub fn pin(cpu: usize) -> bool {
    let mut words = vec![0usize; (cpu / usize::BITS as usize + 1).max(16)];
    words[cpu / usize::BITS as usize] |= 1usize << (cpu % usize::BITS as usize);
    // SAFETY: bitmap is initialized, aligned, and spans the provided byte count.
    unsafe {
        libc::sched_setaffinity(
            0,
            words.len() * std::mem::size_of::<usize>(),
            words.as_ptr().cast(),
        ) == 0
    }
}
#[cfg(not(target_os = "linux"))]
pub fn pin(_: usize) -> bool {
    false
}

pub fn cpu_model(text: &str) -> Option<String> {
    for key in ["model name", "Hardware", "Processor"] {
        if let Some(v) = text
            .lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(k, v)| k.trim() == key && !v.trim().is_empty())
            .map(|(_, v)| v.trim().to_string())
        {
            return Some(v);
        }
    }
    None
}
pub fn memory_value(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        if k != key {
            return None;
        }
        v.split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    })
}
pub fn load(root: &Path) -> Option<[f64; 3]> {
    let s = read(root, "/proc/loadavg")?;
    let v: Vec<_> = s
        .split_whitespace()
        .take(3)
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}
pub fn frequency(root: &Path, cpu: usize) -> Option<f64> {
    read(
        root,
        &format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq"),
    )
    .and_then(|s| s.parse::<f64>().ok())
    .map(|v| v / 1000.)
}

pub fn detect(root: &Path, allowed: Vec<usize>) -> Result<SystemInfo> {
    if allowed.is_empty() {
        return Err(Error::new(5, "no CPUs available to process"));
    }
    let cpuinfo = read(root, "/proc/cpuinfo").unwrap_or_default();
    let meminfo = read(root, "/proc/meminfo").unwrap_or_default();
    let cg = cgroup_limits(root);
    let usable: Vec<_> = allowed
        .iter()
        .copied()
        .filter(|c| cg.cpuset.as_ref().is_none_or(|s| s.contains(c)))
        .collect();
    let capacity = cg
        .quota
        .unwrap_or(usable.len() as f64)
        .min(usable.len() as f64);
    if capacity <= 0. {
        return Err(Error::new(5, "empty effective CPU set"));
    }
    // Fractional quotas need one worker; floor avoids systematic oversubscription.
    let workers = (capacity.floor() as usize).max(1);
    let dmi = [
        "sys/class/dmi/id/sys_vendor",
        "sys/class/dmi/id/product_name",
    ]
    .iter()
    .filter_map(|p| read(root, p))
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase();
    let hypervisor = if dmi.contains("kvm") {
        Some("kvm")
    } else if dmi.contains("qemu") {
        Some("qemu")
    } else if dmi.contains("vmware") {
        Some("vmware")
    } else if dmi.contains("virtualbox") || dmi.contains("innotek") {
        Some("virtualbox")
    } else if dmi.contains("microsoft") && dmi.contains("virtual") {
        Some("hyper-v")
    } else if read(root, "/sys/hypervisor/type").is_some_and(|s| s == "xen") || dmi.contains("xen")
    {
        Some("xen")
    } else {
        None
    };
    let hypervisor_flag = cpuinfo
        .lines()
        .filter_map(|l| l.split_once(':'))
        .any(|(k, v)| k.trim() == "flags" && v.split_whitespace().any(|f| f == "hypervisor"));
    let container_text = read(root, "/proc/self/cgroup")
        .unwrap_or_default()
        .to_lowercase();
    let container = if container_text.contains("kubepods") {
        Some("kubernetes")
    } else if root.join("run/.containerenv").exists() {
        Some("podman")
    } else if root.join(".dockerenv").exists() || container_text.contains("docker") {
        Some("docker")
    } else if container_text.contains("libpod") {
        Some("podman")
    } else if container_text.contains("lxc") {
        Some("lxc")
    } else {
        None
    };
    let container_owned = container
        .map(str::to_string)
        .or_else(|| read(root, "/run/systemd/container"));
    let topology: Vec<_> = usable
        .iter()
        .filter_map(|c| {
            let base = format!("/sys/devices/system/cpu/cpu{c}/topology");
            Some((
                read(root, &format!("{base}/physical_package_id"))?
                    .parse::<u32>()
                    .ok()?,
                read(root, &format!("{base}/core_id"))?
                    .parse::<u32>()
                    .ok()?,
            ))
        })
        .collect();
    let topology_known = topology.len() == usable.len();
    let cores = topology.iter().copied().collect::<BTreeSet<_>>().len();
    let sockets = topology
        .iter()
        .map(|(s, _)| *s)
        .collect::<BTreeSet<_>>()
        .len();
    let numa = read(root, "/sys/devices/system/node/online")
        .and_then(|s| cpu_list(&s))
        .map(|v| v.len());
    let os = read(root, "/etc/os-release").and_then(|s| {
        s.lines().find_map(|l| {
            l.strip_prefix("PRETTY_NAME=")
                .map(|v| v.trim_matches('"').to_string())
        })
    });
    let available = memory_value(&meminfo, "MemAvailable");
    let headroom = match (available, cg.memory_headroom) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let first = usable[0];
    Ok(SystemInfo {
        architecture: std::env::consts::ARCH.into(),
        kernel: read(root, "/proc/sys/kernel/osrelease"),
        distribution: os,
        // Absence of a hypervisor/container signature is not proof of bare metal.
        virtualized: if hypervisor.is_some() || hypervisor_flag {
            Some(true)
        } else {
            None
        },
        hypervisor: hypervisor.map(str::to_string),
        containerized: container_owned.as_ref().map(|_| true),
        container: container_owned,
        memory_total_bytes: memory_value(&meminfo, "MemTotal"),
        memory_available_bytes: available,
        memory_limit_bytes: cg.memory_limit,
        memory_headroom_bytes: headroom,
        uptime_seconds: read(root, "/proc/uptime")
            .and_then(|s| s.split_whitespace().next()?.parse().ok()),
        load_average: load(root),
        cpu: CpuInfo {
            model: cpu_model(&cpuinfo),
            logical_cpus: cpuinfo
                .lines()
                .filter(|l| {
                    l.split_once(':')
                        .is_some_and(|(k, _)| k.trim() == "processor")
                })
                .count()
                .max(allowed.len()),
            affinity_cpus: usable,
            cpuset: cg.cpuset,
            quota_cpus: cg.quota,
            cgroup_version: cg.version,
            cgroup_limits_complete: cg.complete,
            usable_cpus: workers,
            effective_capacity: capacity,
            physical_cores: topology_known.then_some(cores),
            sockets: topology_known.then_some(sockets),
            smt: topology_known.then_some(cores < topology.len()),
            numa_nodes: numa,
            governor: read(
                root,
                &format!("/sys/devices/system/cpu/cpu{first}/cpufreq/scaling_governor"),
            ),
            current_mhz: frequency(root, first),
            nominal_mhz: read(
                root,
                &format!("/sys/devices/system/cpu/cpu{first}/cpufreq/base_frequency"),
            )
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v / 1000.),
        },
    })
}
pub fn current() -> Result<SystemInfo> {
    if !cfg!(target_os = "linux") || !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return Err(Error::new(
            3,
            "supported platforms: Linux x86_64 and aarch64",
        ));
    }
    detect(Path::new("/"), affinity()?)
}
pub fn preflight(s: &SystemInfo, strict: bool) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    if s.containerized == Some(true) {
        warnings.push("ServerBench appears to be running inside a container. CPU quotas, cpusets and container limits may affect results. Running directly on the host is recommended.".into());
    }
    if !s.cpu.cgroup_limits_complete {
        warnings.push("Some cgroup ancestors may be hidden by a namespace; host limits cannot be established completely.".into());
    }
    if s.cpu.numa_nodes.is_some_and(|n| n > 1) {
        warnings.push(
            "Multiple NUMA nodes detected. CPU results may be influenced by topology.".into(),
        );
    }
    let loaded = s
        .load_average
        .is_some_and(|l| l[0] > s.cpu.effective_capacity);
    let low_memory = s
        .memory_headroom_bytes
        .is_some_and(|m| m < required_memory(s.cpu.usable_cpus));
    if loaded {
        warnings.push("Existing system load may influence benchmark results.".into());
    }
    if low_memory {
        warnings.push("Limited memory headroom for benchmark workers.".into());
    }
    if strict && (loaded || low_memory || s.memory_headroom_bytes.is_none()) {
        return Err(Error::new(
            5,
            "strict preflight failed: load or memory preconditions",
        ));
    }
    // Even non-strict must not knowingly overrun severely constrained memory.
    if s.memory_headroom_bytes
        .is_some_and(|m| m < required_memory(s.cpu.usable_cpus) / 2)
    {
        return Err(Error::new(5, "insufficient memory for benchmark workers"));
    }
    Ok(warnings)
}
pub fn required_memory(workers: usize) -> u64 {
    64 * 1024 * 1024 + workers as u64 * 8 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsers() {
        assert_eq!(cpu_list("0-2,5,8-9"), Some(vec![0, 1, 2, 5, 8, 9]));
        assert_eq!(cpu_list("4-2"), None);
        assert_eq!(quota("400000 100000"), Some(4.));
        assert_eq!(quota("max 100000"), None);
        assert_eq!(quota("-1 100000"), None);
        assert_eq!(
            cpu_model("processor : 0\nmodel name : EPYC\n"),
            Some("EPYC".into())
        );
        assert_eq!(memory_value("MemTotal: 1024 kB", "MemTotal"), Some(1048576));
    }
}

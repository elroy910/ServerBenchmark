use serverbench_system::{cgroup_limits, detect};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "serverbench-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        let f = Self(path);
        f.put(
            "proc/cpuinfo",
            "processor : 0\nmodel name : Test CPU\nprocessor : 1\nprocessor : 2\nprocessor : 3\n",
        );
        f.put(
            "proc/meminfo",
            "MemTotal: 8388608 kB\nMemAvailable: 7340032 kB\n",
        );
        f.put("proc/loadavg", "0.20 0.10 0.05 1/100 1\n");
        f.put("proc/uptime", "12345.00 20000.00\n");
        f.put("proc/sys/kernel/osrelease", "6.8-fixture\n");
        f.put("etc/os-release", "PRETTY_NAME=\"Fixture Linux\"\n");
        f
    }
    fn put(&self, path: &str, value: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, value).unwrap();
    }
    fn root(&self) -> &Path {
        &self.0
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn bare_metal_like_does_not_claim_proof() {
    let f = Fixture::new();
    for cpu in 0..4 {
        f.put(
            &format!("sys/devices/system/cpu/cpu{cpu}/topology/core_id"),
            &cpu.to_string(),
        );
        f.put(
            &format!("sys/devices/system/cpu/cpu{cpu}/topology/physical_package_id"),
            "0",
        );
    }
    f.put("sys/devices/system/node/online", "0");
    let s = detect(f.root(), vec![0, 1, 2, 3]).unwrap();
    assert_eq!(s.cpu.usable_cpus, 4);
    assert_eq!(s.cpu.physical_cores, Some(4));
    assert_eq!(s.cpu.smt, Some(false));
    assert_eq!(s.virtualized, None);
    assert_eq!(s.containerized, None);
}
#[test]
fn kvm_fixture() {
    let f = Fixture::new();
    f.put("sys/class/dmi/id/product_name", "KVM");
    let s = detect(f.root(), vec![0, 1]).unwrap();
    assert_eq!(s.virtualized, Some(true));
    assert_eq!(s.hypervisor.as_deref(), Some("kvm"));
}
#[test]
fn container_with_ancestor_quota_and_cpuset_intersection() {
    let f = Fixture::new();
    f.put(".dockerenv", "");
    f.put("proc/self/cgroup", "0::/tenants/docker/run\n");
    f.put(
        "proc/self/mountinfo",
        "42 20 0:28 / /sys/fs/cgroup rw - cgroup2 cgroup rw\n",
    );
    f.put("sys/fs/cgroup/cpu.max", "max 100000");
    f.put("sys/fs/cgroup/tenants/cpu.max", "150000 100000");
    f.put("sys/fs/cgroup/tenants/docker/run/cpu.max", "max 100000");
    f.put("sys/fs/cgroup/tenants/cpuset.cpus.effective", "0-2");
    f.put(
        "sys/fs/cgroup/tenants/docker/run/cpuset.cpus.effective",
        "1-3",
    );
    f.put("sys/fs/cgroup/tenants/memory.max", "134217728");
    f.put("sys/fs/cgroup/tenants/memory.current", "33554432");
    let s = detect(f.root(), vec![0, 1, 2, 3]).unwrap();
    assert_eq!(s.container.as_deref(), Some("docker"));
    assert_eq!(s.cpu.affinity_cpus, vec![1, 2]);
    assert_eq!(s.cpu.quota_cpus, Some(1.5));
    assert_eq!(s.cpu.usable_cpus, 1);
    assert_eq!(s.cpu.effective_capacity, 1.5);
    assert_eq!(s.memory_headroom_bytes, Some(100663296));
    assert!(serverbench_system::preflight(&s, false)
        .unwrap()
        .iter()
        .any(|w| w.contains("container")));
}
#[test]
fn cgroup_v1_and_fractional_quota() {
    let f = Fixture::new();
    f.put("proc/self/cgroup", "2:cpu,cpuacct:/vps\n3:cpuset:/vps\n");
    f.put("proc/self/mountinfo","40 20 0:29 / /sys/fs/cgroup/cpu rw - cgroup cgroup rw,cpu,cpuacct\n41 20 0:30 / /sys/fs/cgroup/cpuset rw - cgroup cgroup rw,cpuset\n");
    f.put("sys/fs/cgroup/cpu/vps/cpu.cfs_quota_us", "50000");
    f.put("sys/fs/cgroup/cpu/vps/cpu.cfs_period_us", "100000");
    f.put("sys/fs/cgroup/cpuset/vps/cpuset.cpus", "2-3");
    let s = detect(f.root(), vec![0, 1, 2, 3]).unwrap();
    assert_eq!(s.cpu.cgroup_version, Some(1));
    assert_eq!(s.cpu.effective_capacity, 0.5);
    assert_eq!(s.cpu.usable_cpus, 1);
    assert_eq!(s.cpu.affinity_cpus, vec![2, 3]);
}
#[test]
fn namespaced_mount_and_empty_allowed_set() {
    let f = Fixture::new();
    f.put("proc/self/cgroup", "0::/\n");
    f.put(
        "proc/self/mountinfo",
        "40 20 0:28 /host/tenant /sys/fs/cgroup rw - cgroup2 cgroup rw\n",
    );
    f.put("sys/fs/cgroup/cpu.max", "200000 100000");
    let c = cgroup_limits(f.root());
    assert_eq!(c.quota, Some(2.));
    assert!(!c.complete);
    assert_eq!(detect(f.root(), vec![]).unwrap_err().code, 5);
}
#[test]
fn namespaced_child_uses_visible_mount() {
    let f = Fixture::new();
    f.put("proc/self/cgroup", "0::/child\n");
    f.put(
        "proc/self/mountinfo",
        "40 20 0:28 /host/tenant /sys/fs/cgroup rw - cgroup2 cgroup rw\n",
    );
    f.put("sys/fs/cgroup/child/cpu.max", "50000 100000");
    let c = cgroup_limits(f.root());
    assert_eq!(c.quota, Some(0.5));
    assert!(!c.complete);
}
#[test]
fn strict_rejects_load_and_memory_limit_is_effective() {
    let f = Fixture::new();
    f.put("proc/loadavg", "8.00 2.00 1.00 1/100 1");
    let s = detect(f.root(), vec![0, 1]).unwrap();
    assert_eq!(serverbench_system::preflight(&s, true).unwrap_err().code, 5);
}

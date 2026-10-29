use serverbench_core::Result;
use std::{
    ffi::OsString,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

static SIGNAL_CANCELLED: AtomicBool = AtomicBool::new(false);
#[derive(Default, Clone)]
pub struct Cancellation {
    flag: Arc<AtomicBool>,
    observe_signals: bool,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
            || (self.observe_signals && SIGNAL_CANCELLED.load(Ordering::SeqCst))
    }
}
pub struct SignalGuard {
    #[cfg(unix)]
    old: Vec<(libc::c_int, libc::sigaction)>,
}
#[cfg(unix)]
extern "C" fn cancel_signal(_: libc::c_int) {
    SIGNAL_CANCELLED.store(true, Ordering::SeqCst);
}
pub fn signal_cancellation() -> Result<(Cancellation, SignalGuard)> {
    #[cfg(unix)]
    {
        SIGNAL_CANCELLED.store(false, Ordering::SeqCst);
        let mut guard = SignalGuard { old: vec![] };
        for signal in [libc::SIGINT, libc::SIGTERM] {
            // SAFETY: zero initializes sigaction, both pointers are valid, and
            // the handler performs only an atomic store; no allocation or I/O.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                let mut previous = std::mem::zeroed();
                action.sa_sigaction = cancel_signal as *const () as usize;
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(signal, &action, &mut previous) != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                guard.old.push((signal, previous));
            }
        }
        Ok((
            Cancellation {
                observe_signals: true,
                ..Default::default()
            },
            guard,
        ))
    }
    #[cfg(not(unix))]
    {
        Err(serverbench_core::Error::new(
            3,
            "repeat cancellation requires Linux/Unix",
        ))
    }
}
impl Drop for SignalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        for (signal, action) in &self.old {
            // SAFETY: restores saved actions for the same process/signals.
            unsafe {
                libc::sigaction(*signal, action, std::ptr::null_mut());
            }
        }
    }
}
#[derive(Debug, Clone)]
pub struct Invocation {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
}
#[derive(Debug, Clone, Copy)]
pub struct ChildExit {
    pub code: Option<i32>,
    pub success: bool,
    pub cancelled: bool,
}
pub trait ChildExecutor {
    fn execute(
        &mut self,
        invocation: &Invocation,
        cancellation: &Cancellation,
    ) -> Result<ChildExit>;
    fn pause(&mut self, duration: Duration, cancellation: &Cancellation) -> bool;
}
pub struct ProcessExecutor;
struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl ChildExecutor for ProcessExecutor {
    fn execute(
        &mut self,
        invocation: &Invocation,
        cancellation: &Cancellation,
    ) -> Result<ChildExit> {
        let mut command = Command::new(&invocation.executable);
        command.args(&invocation.args).stdin(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut guard = ChildGuard(Some(command.spawn()?));
        let child = guard.0.as_mut().unwrap();
        loop {
            if let Some(status) = child.try_wait()? {
                if cancellation.is_cancelled() {
                    kill_group(child.id());
                }
                guard.0.take();
                return Ok(ChildExit {
                    code: status.code(),
                    success: status.success(),
                    cancelled: cancellation.is_cancelled(),
                });
            }
            if cancellation.is_cancelled() {
                #[cfg(unix)]
                {
                    // SAFETY: child owns a dedicated process group whose ID is
                    // its positive PID. Signal that group, never the caller's.
                    unsafe {
                        libc::kill(-(child.id() as i32), libc::SIGINT);
                    }
                }
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(2) {
                    if let Some(status) = child.try_wait()? {
                        kill_group(child.id());
                        guard.0.take();
                        return Ok(ChildExit {
                            code: status.code(),
                            success: false,
                            cancelled: true,
                        });
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                #[cfg(unix)]
                {
                    // SAFETY: same dedicated child process group as above.
                    unsafe {
                        libc::kill(-(child.id() as i32), libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let status = child.wait()?;
                guard.0.take();
                return Ok(ChildExit {
                    code: status.code(),
                    success: false,
                    cancelled: true,
                });
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    fn pause(&mut self, duration: Duration, cancellation: &Cancellation) -> bool {
        let start = Instant::now();
        while start.elapsed() < duration {
            if cancellation.is_cancelled() {
                return false;
            }
            thread::sleep(
                duration
                    .saturating_sub(start.elapsed())
                    .min(Duration::from_millis(100)),
            );
        }
        !cancellation.is_cancelled()
    }
}
fn kill_group(pid: u32) {
    #[cfg(unix)]
    {
        // SAFETY: every executor child starts a dedicated group with this PID.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

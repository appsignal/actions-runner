//! Graceful shutdown coordination.
//!
//! SIGTERM/SIGINT set a global flag; a watcher thread then SIGKILLs every
//! registered Firecracker process group so blocked `child.wait()` calls in the
//! slot threads return. The slot loops observe the flag, run their teardown
//! (unmount, PID file removal) and exit, letting the manager exit 0 instead of
//! being SIGKILLed by systemd after `TimeoutStopSec`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tracing::{info, warn};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
static CHILDREN: Mutex<BTreeMap<(String, usize), u32>> = Mutex::new(BTreeMap::new());

/// Whether a shutdown signal has been received.
pub fn requested() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

extern "C" fn handle_signal(_: libc::c_int) {
    // Only async-signal-safe work here: set the flag, nothing else.
    SHUTDOWN.store(true, Ordering::SeqCst);
}

/// Install the SIGTERM/SIGINT handler and spawn the watcher thread that kills
/// running Firecracker VMs once shutdown is requested. Call before spawning
/// slot threads.
pub fn install() -> anyhow::Result<()> {
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};

    let action = SigAction::new(
        SigHandler::Handler(handle_signal),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    unsafe {
        sigaction(Signal::SIGTERM, &action)?;
        sigaction(Signal::SIGINT, &action)?;
    }

    std::thread::Builder::new()
        .name("shutdown-watcher".to_string())
        .spawn(|| {
            while !requested() {
                std::thread::sleep(Duration::from_millis(250));
            }
            info!("shutdown requested, stopping running VMs");
            kill_children();
        })?;

    Ok(())
}

/// Record a running Firecracker child for a slot so the watcher can stop it.
pub fn register_child(role: &str, idx: usize, pid: u32) {
    CHILDREN
        .lock()
        .unwrap()
        .insert((role.to_string(), idx), pid);
    // The watcher may have already swept before this child was registered.
    if requested() {
        kill_children();
    }
}

/// Remove a slot's child after it has exited.
pub fn unregister_child(role: &str, idx: usize) {
    CHILDREN.lock().unwrap().remove(&(role.to_string(), idx));
}

fn kill_children() {
    let children = CHILDREN.lock().unwrap();
    for ((role, idx), pid) in children.iter() {
        info!(role = %role, idx = *idx, pid = *pid, "killing Firecracker process group");
        // Firecracker setsid()s into its own process group; kill the group.
        let ret = unsafe { libc::kill(-(*pid as libc::pid_t), libc::SIGKILL) };
        if ret != 0 {
            warn!(role = %role, idx = *idx, pid = *pid, "kill failed (process already gone?)");
        }
    }
}

use anyhow::Result;
use config::manager::{ManagerConfig, Role};
use opentelemetry::KeyValue;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, instrument, warn};

use crate::{instance::Instance, network::NetworkAllocation};

/// Run the per-slot loop for a given role and slot index.
/// This function is intended to run in its own OS process or thread.
#[instrument(skip(config), fields(role = %role.name, idx))]
pub fn run_slot(config: Arc<ManagerConfig>, role: &Role, idx: usize) -> Result<()> {
    let network_allocation = NetworkAllocation::new(&config.network_interface, idx as u8);
    let github = github::GitHub::new(&config.github_org, &config.github_pat);

    let mut instance = Instance::new(
        network_allocation,
        github,
        &config.run_path,
        role,
        idx as u8,
    );

    instance.setup().map_err(|e| {
        error!(role = %role.name, idx, error = %e, "slot setup failed");
        e
    })?;

    info!(role = %role.name, idx, "slot setup complete, entering boot loop");

    let host = util::hostname();
    let meter = opentelemetry::global::meter("actions-runner");
    let jobs_counter = meter
        .u64_counter("actions_runner.jobs_completed")
        .with_description("Number of Firecracker boot cycles completed")
        .build();

    let mut cycle_n: u64 = 0;

    loop {
        let pid_file = instance.pid_file_path();

        // Graceful restart: if Firecracker is already running for this slot, reattach
        if let Some(pid) = read_pid_file(&pid_file) {
            if is_process_alive(pid) {
                warn!(
                    role = %role.name,
                    idx,
                    pid,
                    "Firecracker already running, reattaching"
                );
                wait_for_pid(pid);
                remove_pid_file(&pid_file);
                // Fall through to next cycle after the job finishes
                continue;
            }
        }
        remove_pid_file(&pid_file);

        // Normal boot cycle
        cycle_n += 1;
        let span = tracing::info_span!("boot_cycle", role = %role.name, idx, cycle = cycle_n);
        let _guard = span.enter();

        info!(role = %role.name, idx, cycle = cycle_n, "starting boot cycle");

        match instance.start() {
            Ok(mut child) => {
                let pid = child.id();
                if let Err(e) = write_pid_file(&pid_file, pid) {
                    error!(role = %role.name, idx, pid, error = %e, "failed to write PID file");
                }

                let status = {
                    let _running = tracing::info_span!(
                        "vm_running",
                        role = %role.name,
                        idx,
                        cycle = cycle_n,
                        pid
                    )
                    .entered();
                    child.wait()
                };

                match status {
                    Ok(status) => {
                        let result_str = if status.success() {
                            "success"
                        } else {
                            "failure"
                        };
                        info!(
                            role = %role.name,
                            idx,
                            cycle = cycle_n,
                            exit_code = status.code(),
                            result = result_str,
                            "Firecracker exited"
                        );
                        if !status.success() {
                            for line in read_child_stderr(&mut child).lines() {
                                error!(
                                    role = %role.name,
                                    idx,
                                    cycle = cycle_n,
                                    exit_code = status.code(),
                                    "firecracker: {}", line
                                );
                            }
                        }
                        jobs_counter.add(
                            1,
                            &[
                                KeyValue::new("role", role.name.clone()),
                                KeyValue::new("host.name", host.clone()),
                                KeyValue::new("result", result_str),
                            ],
                        );
                    }
                    Err(e) => {
                        error!(role = %role.name, idx, error = %e, "error waiting for Firecracker");
                        jobs_counter.add(
                            1,
                            &[
                                KeyValue::new("role", role.name.clone()),
                                KeyValue::new("host.name", host.clone()),
                                KeyValue::new("result", "failure"),
                            ],
                        );
                    }
                }

                remove_pid_file(&pid_file);
            }
            Err(e) => {
                error!(role = %role.name, idx, cycle = cycle_n, error = %e, "boot cycle failed");
                std::thread::sleep(Duration::from_secs(20));
                instance.reset();
                continue;
            }
        }

        // Cache clear check before next cycle
        if let Err(e) = instance.try_clear_cache() {
            error!(role = %role.name, idx, error = %e, "cache clear failed");
        }
    }
}

fn read_child_stderr(child: &mut std::process::Child) -> String {
    let mut s = String::new();
    if let Some(stderr) = child.stderr.as_mut() {
        let _ = stderr.read_to_string(&mut s);
    }
    s
}

fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) does not send a signal; returns 0 if process exists
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn wait_for_pid(pid: u32) {
    loop {
        std::thread::sleep(Duration::from_secs(2));
        if !is_process_alive(pid) {
            break;
        }
    }
}

fn read_pid_file(path: &Path) -> Option<u32> {
    let contents = std::fs::read_to_string(path).ok()?;
    contents.trim().parse().ok()
}

fn write_pid_file(path: &Path, pid: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, pid.to_string())?;
    Ok(())
}

fn remove_pid_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

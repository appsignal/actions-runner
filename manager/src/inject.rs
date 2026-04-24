use anyhow::{Context, Result};
use camino::Utf8Path;
use std::fs;
use std::os::unix::fs::symlink;
use tracing::{info, instrument};

use crate::network::NetworkAllocation;

const RUNNER_SERVICE_TEMPLATE: &str = r#"[Unit]
Description=Actions Runner
After=network.target cache-mount.service

[Service]
ExecStart=/bin/bash -c '/home/runner/config.sh \
  --url https://github.com/{github_org} \
  --token {github_token} \
  --unattended --ephemeral \
  --name {runner_name} \
  --labels {labels} \
  && /home/runner/run.sh'
KillMode=control-group
KillSignal=SIGTERM
TimeoutStopSec=5min
WorkingDirectory=/home/runner
User=runner
Restart=never
ExecStopPost=+/usr/sbin/reboot
"#;

const NETWORK_CONFIG_TEMPLATE: &str = r#"[Match]
MACAddress={guest_mac}

[Network]
Address={client_ip}/30
Gateway={host_ip}
"#;

/// Inject runner.service, eth.network, and service symlink into a mounted rootfs.
#[instrument(skip(network), fields(mount_point = %mount_point, runner_name, role))]
pub fn inject_config(
    mount_point: &Utf8Path,
    github_org: &str,
    github_token: &str,
    runner_name: &str,
    labels: &str,
    network: &NetworkAllocation,
) -> Result<()> {
    inject_runner_service(mount_point, github_org, github_token, runner_name, labels)?;
    inject_network_config(mount_point, network)?;
    enable_runner_service(mount_point)?;
    Ok(())
}

fn inject_runner_service(
    mount_point: &Utf8Path,
    github_org: &str,
    github_token: &str,
    runner_name: &str,
    labels: &str,
) -> Result<()> {
    let service_content = RUNNER_SERVICE_TEMPLATE
        .replace("{github_org}", github_org)
        .replace("{github_token}", github_token)
        .replace("{runner_name}", runner_name)
        .replace("{labels}", labels);

    let service_dir = mount_point.join("etc/systemd/system");
    fs::create_dir_all(&service_dir)
        .with_context(|| format!("failed to create {}", service_dir))?;

    let service_path = service_dir.join("runner.service");
    fs::write(&service_path, service_content)
        .with_context(|| format!("failed to write {}", service_path))?;

    info!(path = %service_path, "injected runner.service");
    Ok(())
}

fn inject_network_config(mount_point: &Utf8Path, network: &NetworkAllocation) -> Result<()> {
    let network_content = NETWORK_CONFIG_TEMPLATE
        .replace("{guest_mac}", &network.guest_mac)
        .replace("{client_ip}", &network.client_ip.to_string())
        .replace("{host_ip}", &network.host_ip.to_string());

    let network_dir = mount_point.join("etc/systemd/network");
    fs::create_dir_all(&network_dir)
        .with_context(|| format!("failed to create {}", network_dir))?;

    let network_path = network_dir.join("eth.network");
    fs::write(&network_path, network_content)
        .with_context(|| format!("failed to write {}", network_path))?;

    info!(path = %network_path, "injected eth.network");
    Ok(())
}

fn enable_runner_service(mount_point: &Utf8Path) -> Result<()> {
    let wants_dir = mount_point.join("etc/systemd/system/multi-user.target.wants");
    fs::create_dir_all(&wants_dir).with_context(|| format!("failed to create {}", wants_dir))?;

    let symlink_path = wants_dir.join("runner.service");
    let _ = fs::remove_file(&symlink_path); // ignore error if doesn't exist

    symlink("/etc/systemd/system/runner.service", &symlink_path)
        .with_context(|| format!("failed to create symlink at {}", symlink_path))?;

    info!(path = %symlink_path, "enabled runner.service via symlink");
    Ok(())
}

use anyhow::Result;
use std::fs::write;
use std::os::unix::fs::symlink;

pub const SERVICE_PATH: &str = "/etc/systemd/system/runner.service";
pub const SERVICE_TEMPLATE: &str = r#"
[Unit]
Description=Actions Runner
After=network.target

[Service]
ExecStart=/sbin/actions-run
KillMode=control-group
KillSignal=SIGTERM
TimeoutStopSec=5min
WorkingDirectory=/home/runner
User=runner
Restart=never
Environment="GITHUB_ORG={github_org}"
Environment="GITHUB_TOKEN={github_token}"
Environment="GITHUB_RUNNER_NAME={github_runner_name}"
Environment="GITHUB_RUNNER_LABELS={github_runner_labels}"
ExecStopPost=+/usr/sbin/reboot
"#;

pub fn setup_service(
    github_org: &str,
    github_token: &str,
    github_runner_name: &str,
    github_runner_labels: &str,
) -> Result<()> {
    let service = SERVICE_TEMPLATE
        .replace("{github_org}", github_org)
        .replace("{github_token}", github_token)
        .replace("{github_runner_name}", github_runner_name)
        .replace("{github_runner_labels}", github_runner_labels);

    write(SERVICE_PATH, service)?;

    Ok(())
}

pub fn enable_service() -> Result<()> {
    symlink(
        SERVICE_PATH,
        "/etc/systemd/system/multi-user.target.wants/runner.service",
    )?;

    Ok(())
}

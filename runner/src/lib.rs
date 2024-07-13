use anyhow::Result;
use std::process::Command;
use util::exec;

pub struct Runner {}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner {
    pub fn new() -> Self {
        Runner {}
    }

    pub fn run(&self) -> Result<()> {
        exec(
            Command::new("/home/runner/config.sh")
                .arg("--url")
                .arg(format!(
                    "https://github.com/{}",
                    std::env::var("GITHUB_ORG").unwrap()
                ))
                .arg("--token")
                .arg(std::env::var("GITHUB_TOKEN").unwrap())
                .arg("--unattended")
                .arg("--ephemeral")
                .arg("--name")
                .arg(std::env::var("GITHUB_RUNNER_NAME").unwrap())
                .arg("--labels")
                .arg(std::env::var("GITHUB_RUNNER_LABELS").unwrap()),
        )?;

        exec(&mut Command::new("/home/runner/run.sh"))?;
        Ok(())
    }
}

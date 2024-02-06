use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use log::*;
use std::env;
use std::fs::copy;
use std::os::unix::process::CommandExt;
use std::process::Command;

mod cache;
mod network;
mod service;

pub struct Initialiser {
    pub own_path: Utf8PathBuf,
}

impl Initialiser {
    pub fn new(path: impl AsRef<Utf8Path>) -> Self {
        let path = path.as_ref();

        Initialiser {
            own_path: Utf8PathBuf::from(path),
        }
    }

    pub fn run(&self) -> Result<()> {
        debug!("Setup network");
        match network::setup_network() {
            Ok(Some(interface)) => info!(
                "Network setup complete: {} {} > {}",
                interface.ifname, interface.own_address, interface.host_address
            ),
            Ok(None) => info!("No magic address found, skipping network setup"),
            Err(e) => {
                error!("Network setup failed: {}\n\n", e);
                return Err(e.into());
            }
        }

        debug!("Setup dns");
        match network::setup_dns() {
            Ok(_) => info!("DNS setup complete"),
            Err(e) => {
                error!("DNS setup failed: {}", e);
                return Err(e.into());
            }
        }

        debug!("Setup cache");
        match env::var("cache_paths") {
            Ok(cache_paths) => {
                match cache::setup_cache(&cache_paths) {
                    Ok(_) => info!("Cache setup complete"),
                    Err(e) => {
                        error!("Cache setup failed: {}", e);
                        return Err(e.into());
                    }
                };
            }
            Err(_) => {
                info!("No 'cache' kernel arg found, skipping cache setup");
            }
        }

        debug!("Setup actions-runner");
        match (
            env::var("github_org"),
            env::var("github_token"),
            env::var("github_runner_name"),
            env::var("github_runner_labels"),
        ) {
            (
                Ok(github_org),
                Ok(github_token),
                Ok(github_runner_name),
                Ok(github_runner_labels),
            ) => {
                debug!("Copy self to actions-runner");
                copy(&self.own_path, &Utf8PathBuf::from("/sbin/actions-run"))?;

                debug!("Set runner init script");
                service::setup_service(
                    &github_org,
                    &github_token,
                    &github_runner_name,
                    &github_runner_labels,
                )?;

                debug!("Symlink init script to start at boot");
                service::enable_service()?;
            }
            _ => {
                info!("No 'github_org', 'github_token' or 'github_runner_name' kernel arg found, skipping actions-runner setup");
            }
        }

        Command::new("/sbin/init").exec();
        Ok(())
    }
}

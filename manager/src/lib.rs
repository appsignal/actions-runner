use crate::{
    instance::{Instance, InstanceState},
    network::{Forwarding, NetworkAllocation},
};
use anyhow::Result;
use config::manager::ManagerConfig;
use github::GitHub;
use log::*;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub mod disk;
pub mod instance;
pub mod network;

pub struct Manager {
    pub config: ManagerConfig,
    pub instances: Vec<Instance>,
    pub shutdown: Arc<AtomicBool>,
}

impl Manager {
    pub fn new(config: ManagerConfig) -> Self {
        let mut signals = Signals::new([SIGINT]).unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let cloned_shutdown = shutdown.clone();

        thread::spawn(move || {
            for sig in signals.forever() {
                info!("Received signal {:?}", sig);
                shutdown.store(true, Ordering::Relaxed);
            }
        });

        Self {
            config,
            instances: Vec::new(),
            shutdown: cloned_shutdown,
        }
    }

    pub fn setup(&mut self) -> Result<()> {
        let network_forwarding = Forwarding::new(&self.config.network_interface);
        network_forwarding.setup()?;

        let github = GitHub::new(&self.config.github_org, &self.config.github_pat);

        for role in &self.config.roles {
            for _ in 0..role.instance_count {
                let idx = self.instances.len() as u8 + 1;

                let network_allocation =
                    NetworkAllocation::new(&self.config.network_interface, idx);

                let mut instance = Instance::new(
                    network_allocation,
                    github.clone(),
                    &self.config.run_path,
                    role,
                    idx,
                );
                instance.setup()?;
                self.instances.push(instance);
            }
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                info!("Shutting down.");
                for instance in &mut self.instances {
                    info!("{} Stopping instance", instance.log_prefix());

                    if let Err(e) = instance.stop() {
                        error!("{} Failed to stop instance: {}", instance.log_prefix(), e);
                    }
                    let _ = instance.cleanup();
                }
                break;
            }

            for instance in &mut self.instances {
                match instance.state() {
                    InstanceState::Running => (),
                    InstanceState::NotStarted | InstanceState::NotRunning => {
                        info!("{} Starting instance", instance.log_prefix());
                        if let Err(e) = instance.start() {
                            error!("{} Failed to start instance: {}", instance.log_prefix(), e);
                        }
                    }
                    InstanceState::Errorred => {
                        error!("{} Instance has errored.", instance.log_prefix());
                        thread::sleep(Duration::from_secs(20));
                        instance.reset();
                    }
                }
            }
            thread::sleep(Duration::from_secs(1));
        }

        Ok(())
    }

    pub fn debug(&mut self, role: &str, idx: u8) -> Result<()> {
        let network_forwarding = Forwarding::new(&self.config.network_interface);
        let network_allocation = NetworkAllocation::new(&self.config.network_interface, idx);
        let github = GitHub::new(&self.config.github_org, &self.config.github_pat);
        let mut role = self
            .config
            .roles
            .iter()
            .find(|r| r.name == role)
            .expect("Could not find role.")
            .clone();

        // Set output to console
        role.kernel_cmdline = match role.kernel_cmdline {
            Some(ref cmdline) => Some(format!("console=ttyS0 {}", cmdline)),
            None => Some("console=ttyS0".to_string()),
        };

        let mut instance = Instance::new(
            network_allocation,
            github,
            &self.config.run_path,
            &role,
            idx,
        );
        network_forwarding.setup()?;
        instance.setup()?;

        instance.run_once()?;

        instance.cleanup()?;
        Ok(())
    }
}

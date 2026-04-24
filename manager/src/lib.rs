use anyhow::Result;
use config::manager::ManagerConfig;
use std::sync::Arc;
use std::thread;
use tracing::{error, info, instrument, warn};

pub mod disk;
pub mod inject;
pub mod instance;
pub mod lvm;
pub mod network;
pub mod slot;

use network::Forwarding;

pub struct Manager {
    pub config: ManagerConfig,
}

impl Manager {
    pub fn new(config: ManagerConfig) -> Self {
        Self { config }
    }

    /// Run the manager: set up forwarding, reconcile LVM, then spawn one process per slot.
    #[instrument(skip(self))]
    pub fn run(&self) -> Result<()> {
        info!("starting manager");

        // Set up IP forwarding
        let forwarding = Forwarding::new(&self.config.network_interface);
        forwarding.setup()?;

        // Reconcile LVM state
        {
            let _span = tracing::info_span!("startup.reconcile").entered();
            info!("reconciling LVM state");
            lvm::reconcile(&self.config)?;
        }

        let config = Arc::new(self.config.clone());
        let mut handles = Vec::new();

        // Spawn one thread per slot
        for role in &config.roles {
            for idx in 0..role.instance_count as usize {
                let config_clone = Arc::clone(&config);
                let role_clone = role.clone();

                info!(role = %role.name, idx, "spawning slot thread");

                let handle = thread::Builder::new()
                    .name(format!("slot-{}-{}", role.slug(), idx))
                    .spawn(move || {
                        if let Err(e) = slot::run_slot(config_clone, &role_clone, idx) {
                            error!(role = %role_clone.name, idx, error = %e, "slot thread exited with error");
                        }
                    })?;

                handles.push(handle);
            }
        }

        info!(slots = handles.len(), "all slot threads started");

        // Install SIGINT/SIGTERM handler
        setup_signal_handler()?;

        // Join all slot threads (they run forever unless killed)
        for handle in handles {
            if let Err(e) = handle.join() {
                warn!("slot thread panicked: {:?}", e);
            }
        }

        info!("manager shutting down");
        Ok(())
    }
}

fn setup_signal_handler() -> Result<()> {
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};

    // Install a no-op handler for SIGTERM and SIGINT so the process can shut down
    // gracefully. Slot processes receive the signal via their own process group.
    let action = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());
    unsafe {
        sigaction(Signal::SIGTERM, &action)?;
        sigaction(Signal::SIGINT, &action)?;
    }
    Ok(())
}

use anyhow::Result;
use config::manager::{ManagerConfig, Role};
use config::NETWORK_MAX_ALLOCATIONS;
use std::sync::Arc;
use std::thread;
use tracing::{error, info, instrument, warn};

pub mod disk;
pub mod inject;
pub mod instance;
pub mod lvm;
pub mod network;
pub mod shutdown;
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

        // Install the SIGTERM/SIGINT handler and VM-killing watcher before any
        // slot exists, so an early stop is handled gracefully too.
        shutdown::install()?;

        let config = Arc::new(self.config.clone());
        let mut handles = Vec::new();

        // Spawn one thread per slot. Each slot's network (tap device and /30
        // subnet) is numbered across all roles on this host: numbering per role
        // gave the first slot of every role tap0, so the second role's slot 0
        // failed to boot with "Resource busy".
        let net_indices = network_indices(&config.roles)?;
        for (role_pos, role) in config.roles.iter().enumerate() {
            for (idx, &net_idx) in net_indices[role_pos].iter().enumerate() {
                let config_clone = Arc::clone(&config);
                let role_clone = role.clone();

                info!(role = %role.name, idx, net_idx, "spawning slot thread");

                let handle = thread::Builder::new()
                    .name(format!("slot-{}-{}", role.slug(), idx))
                    .spawn(move || {
                        if let Err(e) = slot::run_slot(config_clone, &role_clone, idx, net_idx) {
                            error!(role = %role_clone.name, idx, error = %e, "slot thread exited with error");
                        }
                    })?;

                handles.push(handle);
            }
        }

        info!(slots = handles.len(), "all slot threads started");

        // Join all slot threads. They run until a shutdown signal is received,
        // at which point each slot tears down (unmount, PID file) and exits.
        for handle in handles {
            if let Err(e) = handle.join() {
                warn!("slot thread panicked: {:?}", e);
            }
        }

        info!("all slots stopped, manager shutting down");
        Ok(())
    }
}

/// The host-wide network index of every slot, per role in config order: the
/// first role's slots get 0..n, the next role's continue from n, and so on.
/// Errors when there are more slots than NETWORK_MAX_ALLOCATIONS tap devices and
/// 172.16.x.0/30 subnets.
pub fn network_indices(roles: &[Role]) -> Result<Vec<Vec<u8>>> {
    let total: usize = roles.iter().map(|r| r.instance_count as usize).sum();
    if total > NETWORK_MAX_ALLOCATIONS as usize {
        anyhow::bail!(
            "{total} slots configured, but a host supports at most {NETWORK_MAX_ALLOCATIONS}"
        );
    }
    let mut next = 0usize;
    Ok(roles
        .iter()
        .map(|role| {
            let indices = (next..next + role.instance_count as usize)
                .map(|i| i as u8)
                .collect();
            next += role.instance_count as usize;
            indices
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(name: &str, instance_count: u8) -> Role {
        Role {
            name: name.to_string(),
            rootfs_image: "/images/rootfs.img".into(),
            kernel_image: "/kernels/vmlinux".into(),
            kernel_cmdline: None,
            cpus: 4,
            memory_size: 12,
            cache_size: 100,
            instance_count,
            cache_paths: vec![],
            max_cache_pct: 90,
            labels: vec![],
        }
    }

    #[test]
    fn single_role_keeps_its_slot_numbers() {
        let indices = network_indices(&[role("appsignal-server", 8)]).unwrap();
        assert_eq!(indices, vec![(0..8).collect::<Vec<u8>>()]);
    }

    #[test]
    fn later_roles_continue_after_earlier_ones() {
        let indices =
            network_indices(&[role("appsignal-server", 7), role("ai-review", 1)]).unwrap();
        assert_eq!(indices, vec![(0..7).collect::<Vec<u8>>(), vec![7]]);
    }

    #[test]
    fn rejects_more_slots_than_the_host_has_networks_for() {
        assert!(network_indices(&[role("a", 150), role("b", 50)]).is_ok());
        assert!(network_indices(&[role("a", 150), role("b", 51)]).is_err());
    }
}

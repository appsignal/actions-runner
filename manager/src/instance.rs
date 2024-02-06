use crate::{
    disk::{Disk, DiskFormat},
    network::NetworkAllocation,
};
use anyhow::Result;
use camino::Utf8PathBuf;
use config::{
    firecracker::{BootSource, Drive, FirecrackerConfig, MachineConfig, NetworkInterface},
    manager::Role,
    DEFAULT_BOOT_ARGS,
};
use github::GitHub;
use log::*;
use rand::distributions::{Alphanumeric, DistString};
use serde_json;
use std::{fs, process::Command};
use util::fs::{copy_sparse, rm_rf};

pub enum InstanceState {
    NotStarted,
    Running,
    NotRunning,
    Errorred,
}

#[derive(Debug)]
pub struct Instance {
    network_allocation: NetworkAllocation,
    work_dir: Utf8PathBuf,
    kernel_image: Utf8PathBuf,
    kernel_cmdline: Option<String>,
    rootfs_image: Utf8PathBuf,
    cpus: u32,
    memory_size: u32,
    cache_paths: Vec<Utf8PathBuf>,
    cache: Disk,
    idx: u8,
    role: String,
    github: GitHub,
    labels: Vec<String>,
    child: Option<std::process::Child>,
}

impl Instance {
    pub fn new(
        network_allocation: NetworkAllocation,
        github: GitHub,
        work_dir: &Utf8PathBuf,
        role: &Role,
        idx: u8,
    ) -> Self {
        let instance_dir: Utf8PathBuf = work_dir.join(&role.slug()).join(format!("{}", idx));
        let cache = Disk::new(&instance_dir, "cache", role.cache_size, DiskFormat::Ext4);

        Self {
            network_allocation,
            work_dir: instance_dir.clone(),
            kernel_image: role.kernel_image.clone(),
            kernel_cmdline: role.kernel_cmdline.clone(),
            rootfs_image: role.rootfs_image.clone(),
            cpus: role.cpus,
            memory_size: role.memory_size,
            cache_paths: role.cache_paths.clone(),
            role: role.slug(),
            labels: role.labels.clone(),
            github,
            cache,
            idx,
            child: None,
        }
    }

    pub fn log_prefix(&self) -> String {
        format!("[{} {}]", self.role, self.idx)
    }

    pub fn name(&self) -> String {
        format!(
            "{}-{}-{}",
            self.role,
            self.idx,
            Alphanumeric.sample_string(&mut rand::thread_rng(), 4),
        )
    }

    pub fn setup(&mut self) -> Result<()> {
        info!("Running instance with: {:?}", self);

        debug!(
            "{} Creating work dir: '{}'",
            self.log_prefix(),
            self.work_dir
        );
        fs::create_dir_all(&self.work_dir)?;

        debug!(
            "{} Setup network with tap: '{}', host address: '{}'",
            self.log_prefix(),
            self.network_allocation.tap_name,
            self.network_allocation.host_ip,
        );
        self.network_allocation.setup()?;

        debug!(
            "{} Initialize shared cache on path: '{}' (size: {}GB)",
            self.log_prefix(),
            self.cache.path_with_filename(),
            self.cache.size,
        );
        self.cache.setup()?;

        Ok(())
    }

    pub fn boot_args(&self) -> Result<String> {
        let mut boot_args = vec![DEFAULT_BOOT_ARGS.to_string()];

        // Add GitHub token
        boot_args.push(format!(
            "github_token={}",
            &self.github.registration_token()?
        ));
        boot_args.push(format!("github_org={}", &self.github.org));

        // Add cache paths
        if !self.cache_paths.is_empty() {
            boot_args.push(format!(
                "cache_paths=\"{}\"",
                self.cache_paths
                    .iter()
                    .map(|cp| cp.to_string())
                    .collect::<Vec<String>>()
                    .join(",")
            ));
        }

        // Add overridden boot args
        if let Some(ref cmdline) = &self.kernel_cmdline {
            boot_args.push(cmdline.to_string());
        }

        boot_args.push(format!("github_runner_name={}", self.name()));
        boot_args.push(format!("github_runner_labels={}", self.labels()));

        Ok(boot_args.join(" "))
    }

    pub fn labels(&self) -> String {
        let mut labels = self.labels.clone();
        labels.push(self.role.to_string());
        labels.join(",")
    }

    pub fn config(&self) -> Result<FirecrackerConfig> {
        let boot_source = BootSource {
            kernel_image_path: self.kernel_image.to_string(),
            boot_args: self.boot_args()?,
        };

        let mut drives = Vec::new();
        drives.push(Drive {
            drive_id: "rootfs".to_string(),
            path_on_host: self.work_dir.join("rootfs.ext4"),
            is_root_device: true,
            is_read_only: false,
            cache_type: None,
        });

        drives.push(Drive {
            drive_id: "cache".to_string(),
            path_on_host: self.cache.path_with_filename(),
            is_root_device: false,
            is_read_only: false,
            cache_type: None,
        });

        let mut network_interfaces = Vec::new();
        network_interfaces.push(NetworkInterface {
            iface_id: "eth0".to_string(),
            guest_mac: self.network_allocation.guest_mac.clone(),
            host_dev_name: self.network_allocation.tap_name.clone(),
        });

        let machine_config = MachineConfig {
            vcpu_count: self.cpus,
            mem_size_mib: self.memory_size * 1024,
        };

        Ok(FirecrackerConfig {
            boot_source,
            drives,
            network_interfaces,
            machine_config,
        })
    }

    pub fn setup_run(&mut self) -> Result<()> {
        debug!(
            "{} Copy rootfs from: '{}'to '{}'",
            self.log_prefix(),
            self.rootfs_image,
            self.work_dir.join("rootfs.ext4"),
        );
        let _ = rm_rf(&self.work_dir.join("rootfs.ext4"));
        copy_sparse(&self.rootfs_image, &self.work_dir.join("rootfs.ext4"))?;

        debug!(
            "{} Generate config: '{}'",
            self.log_prefix(),
            self.work_dir.join("config.json")
        );

        fs::write(
            self.work_dir.join("config.json"),
            serde_json::to_string(&self.config()?)?,
        )?;
        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        let _ = rm_rf(&self.work_dir);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.child = None;
    }

    pub fn stop(&mut self) -> Result<()> {
        info!("{} Shutting down instance", self.log_prefix());

        match self.child.as_mut() {
            Some(child) => {
                child.kill()?;
                child.wait()?;
            }
            None => {
                info!("{} No instance to shut down", self.log_prefix());
            }
        }
        Ok(())
    }

    pub fn start(&mut self) -> Result<()> {
        self.setup_run()?;

        debug!("{} Running firecracker", self.log_prefix());
        let child = Command::new("firecracker")
            .args(["--config-file", "config.json", "--no-api"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .current_dir(&self.work_dir)
            .spawn()?;
        self.child = Some(child);
        Ok(())
    }

    pub fn run_once(&mut self) -> Result<()> {
        self.setup_run()?;

        debug!("{} Running firecracker", self.log_prefix());
        Command::new("firecracker")
            .args(["--config-file", "config.json", "--no-api"])
            .current_dir(&self.work_dir)
            .status()
            .expect("Failed to start process");
        Ok(())
    }

    pub fn state(&mut self) -> InstanceState {
        match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        InstanceState::NotRunning
                    } else {
                        InstanceState::Errorred
                    }
                }
                Ok(None) => InstanceState::Running,
                Err(_) => InstanceState::Errorred,
            },
            None => InstanceState::NotStarted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    #[test]
    fn test_instance_setup() {
        let workdir: Utf8PathBuf = "/tmp/test_instance_setup".into();
        let github = GitHub::new("test", "test");
        let network_allocation = NetworkAllocation::new("eth0", 1);
        let role = Role {
            name: "test".to_string(),
            kernel_image: Utf8PathBuf::from("kernel"),
            kernel_cmdline: None,
            rootfs_image: Utf8PathBuf::from("rootfs"),
            cpus: 1,
            memory_size: 1,
            cache_size: 1,
            overlay_size: 1,
            instance_count: 1,
            cache_paths: Vec::new(),
            labels: Vec::new(),
        };

        let mut _instance = Instance::new(network_allocation, github.clone(), &workdir, &role, 1);
        //instance.setup().expect("Could not setup instance");
    }
}

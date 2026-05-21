use crate::{disk::CacheDisk, inject, lvm, network::NetworkAllocation};
use anyhow::Result;
use camino::Utf8PathBuf;
use config::{
    firecracker::{BootSource, Drive, FirecrackerConfig, MachineConfig, NetworkInterface},
    manager::{LvmConfig, Role},
    DEFAULT_BOOT_ARGS,
};
use github::GitHub;
use rand::distributions::{Alphanumeric, DistString};
use std::{fs, io::Read, os::unix::process::CommandExt, path::PathBuf, process::Command};
use tracing::{debug, info, instrument, warn};
use util::mount;

#[derive(Debug)]
pub struct Instance {
    network_allocation: NetworkAllocation,
    work_dir: Utf8PathBuf,
    kernel_image: Utf8PathBuf,
    kernel_cmdline: Option<String>,
    cpus: u32,
    memory_size: u32,
    max_cache_pct: u8,
    idx: u8,
    role: String,
    github: GitHub,
    labels: Vec<String>,
    lvm: LvmConfig,
    cache: CacheDisk,
}

impl Instance {
    pub fn new(
        network_allocation: NetworkAllocation,
        github: GitHub,
        work_dir: &Utf8PathBuf,
        role: &Role,
        idx: u8,
    ) -> Self {
        let instance_dir = work_dir.join(role.slug()).join(format!("{}", idx));
        let cache_lv_name = format!("cache-{}-{}", role.slug(), idx);
        let lvm = LvmConfig {
            volume_group: String::new(), // filled in from ManagerConfig; see setup_with_lvm
            thin_pool: String::new(),
        };
        let cache = CacheDisk::new("", &cache_lv_name, role.cache_size);

        Self {
            network_allocation,
            work_dir: instance_dir,
            kernel_image: role.kernel_image.clone(),
            kernel_cmdline: role.kernel_cmdline.clone(),
            cpus: role.cpus,
            memory_size: role.memory_size,
            max_cache_pct: role.max_cache_pct,
            role: role.slug(),
            labels: role.labels.clone(),
            github,
            lvm,
            cache,
            idx,
        }
    }

    /// Initialise with LVM config (called after construction when ManagerConfig is available).
    pub fn with_lvm(mut self, lvm: &LvmConfig) -> Self {
        let cache_lv_name = format!("cache-{}-{}", self.role, self.idx);
        self.lvm = lvm.clone();
        self.cache = CacheDisk::new(&lvm.volume_group, &cache_lv_name, self.cache.size_gib);
        self
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

    pub fn labels(&self) -> String {
        let mut labels = self.labels.clone();
        labels.push(self.role.to_string());
        labels.join(",")
    }

    pub fn pid_file_path(&self) -> PathBuf {
        self.work_dir.as_std_path().join("firecracker.pid")
    }

    pub fn console_log_path(&self) -> Utf8PathBuf {
        self.work_dir.join("console.log")
    }

    /// Return the last `n` lines of the VM serial console log, or an empty string if unavailable.
    pub fn read_console_tail(&self, n: usize) -> String {
        let path = self.console_log_path();
        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return String::new(),
        };
        let mut content = String::new();
        if file.read_to_string(&mut content).is_err() {
            return String::new();
        }
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(n);
        lines[start..].join("\n")
    }

    #[instrument(skip(self), fields(role = %self.role, idx = self.idx))]
    pub fn setup(&mut self) -> Result<()> {
        debug!(work_dir = %self.work_dir, "creating work directory");
        fs::create_dir_all(&self.work_dir)?;

        debug!(
            tap = %self.network_allocation.tap_name,
            host_ip = %self.network_allocation.host_ip,
            "setting up network"
        );
        self.network_allocation.setup()?;

        info!(role = %self.role, idx = self.idx, "slot setup complete");
        Ok(())
    }

    fn rootfs_device_path(&self) -> String {
        format!(
            "/dev/{}/rootfs-{}-{}",
            self.lvm.volume_group, self.role, self.idx
        )
    }

    fn firecracker_config(&self) -> Result<FirecrackerConfig> {
        let mut boot_args = vec![DEFAULT_BOOT_ARGS.to_string()];
        if let Some(ref extra) = self.kernel_cmdline {
            boot_args.push(extra.clone());
        }

        let boot_source = BootSource {
            kernel_image_path: self.kernel_image.to_string(),
            boot_args: boot_args.join(" "),
        };

        let drives = vec![
            Drive {
                drive_id: "rootfs".to_string(),
                path_on_host: self.rootfs_device_path().into(),
                is_root_device: true,
                is_read_only: false,
                cache_type: None,
            },
            Drive {
                drive_id: "cache".to_string(),
                path_on_host: self.cache.device_path().into(),
                is_root_device: false,
                is_read_only: false,
                cache_type: None,
            },
        ];

        let network_interfaces = vec![NetworkInterface {
            iface_id: "eth0".to_string(),
            guest_mac: self.network_allocation.guest_mac.clone(),
            host_dev_name: self.network_allocation.tap_name.clone(),
        }];

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

    #[instrument(skip(self), fields(role = %self.role, idx = self.idx))]
    pub fn setup_run(&mut self) -> Result<()> {
        let rootfs_lv = format!("rootfs-{}-{}", self.role, self.idx);
        let base_lv = format!("base-{}", self.role);

        // Remove stale snapshot and create fresh one from base
        let _ = lvm::lvremove(&self.lvm.volume_group, &rootfs_lv);
        lvm::lvcreate_snapshot(&self.lvm.volume_group, &base_lv, &rootfs_lv)?;

        // Mount rootfs, inject config, unmount
        let mnt = Utf8PathBuf::from(self.work_dir.join("mnt").as_str());
        fs::create_dir_all(&mnt)?;

        let rootfs_device = self.rootfs_device_path();
        mount::mount_image(&rootfs_device, &mnt)?;

        let token = self.github.registration_token()?;
        let runner_name = self.name();
        let labels = self.labels();

        inject::inject_config(
            &mnt,
            &self.github.org.clone(),
            &token,
            &runner_name,
            &labels,
            &self.network_allocation,
        )?;

        mount::unmount(&mnt)?;
        let _ = fs::remove_dir(&mnt);

        // Write Firecracker config.json
        let config_json = serde_json::to_string(&self.firecracker_config()?)?;
        fs::write(self.work_dir.join("config.json"), config_json)?;

        debug!(role = %self.role, idx = self.idx, runner_name, "setup_run complete");
        Ok(())
    }

    #[instrument(skip(self), fields(role = %self.role, idx = self.idx))]
    pub fn start(&mut self) -> Result<std::process::Child> {
        self.setup_run()?;

        debug!(role = %self.role, idx = self.idx, "spawning Firecracker");
        let console_file = fs::File::create(self.console_log_path())?;
        let child = unsafe {
            Command::new("firecracker")
                .args(["--config-file", "config.json", "--no-api"])
                .current_dir(&self.work_dir)
                .stdin(std::process::Stdio::null())
                .stdout(console_file)
                .stderr(std::process::Stdio::piped())
                .pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
                .spawn()?
        };

        info!(role = %self.role, idx = self.idx, pid = child.id(), "Firecracker spawned");
        Ok(child)
    }

    pub fn reset(&mut self) {
        // Nothing to reset in new model; slot loop handles retry
    }

    #[instrument(skip(self), fields(role = %self.role, idx = self.idx))]
    pub fn try_clear_cache(&mut self) -> Result<()> {
        match self.cache.usage_pct() {
            Ok(pct) if pct > self.max_cache_pct => {
                info!(
                    role = %self.role,
                    idx = self.idx,
                    usage_pct = pct,
                    threshold = self.max_cache_pct,
                    "cache over threshold, clearing"
                );
                self.cache.clear()?;
            }
            Ok(pct) => {
                debug!(
                    role = %self.role,
                    idx = self.idx,
                    usage_pct = pct,
                    "cache usage OK"
                );
            }
            Err(e) => {
                warn!(role = %self.role, idx = self.idx, error = %e, "failed to check cache usage");
            }
        }
        Ok(())
    }
}

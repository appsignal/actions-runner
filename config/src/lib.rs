use thiserror::*;
use toml;

pub mod firecracker;
pub mod manager;

pub const DEFAULT_BOOT_ARGS: &str =
    "random.trust_cpu=on reboot=k panic=1 pci=off overlay_root=vdb init=/sbin/actions-init";
pub const NETWORK_MAGIC_MAC_START: &str = "06:00";
pub const NETWORK_MASK_SHORT: u8 = 30;
pub const NETWORK_MAX_ALLOCATIONS: u8 = 200;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {:?}", self)]
    Io(#[from] std::io::Error),
    #[error("Config TOML error: {:?}", self)]
    Toml(#[from] toml::de::Error),
}

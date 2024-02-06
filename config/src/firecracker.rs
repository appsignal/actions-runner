use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct FirecrackerConfig {
    pub boot_source: BootSource,
    pub drives: Vec<Drive>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub machine_config: MachineConfig,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BootSource {
    pub kernel_image_path: String,
    pub boot_args: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Drive {
    pub drive_id: String,
    pub path_on_host: Utf8PathBuf,
    pub is_root_device: bool,
    pub is_read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub guest_mac: String,
    pub host_dev_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MachineConfig {
    pub vcpu_count: u32,
    pub mem_size_mib: u32,
}

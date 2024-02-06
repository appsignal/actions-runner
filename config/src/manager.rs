use anyhow::Result;
use camino::Utf8PathBuf;
use serde::Deserialize;
use toml;

#[derive(Deserialize, Debug, Clone)]
pub struct ManagerConfig {
    pub network_interface: String,
    pub run_path: Utf8PathBuf,
    pub roles: Vec<Role>,
    pub github_org: String,
    pub github_pat: String,
}

impl ManagerConfig {
    pub fn from_file(path: &Utf8PathBuf) -> Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let config = toml::from_str(&config_str)?;
        Ok(config)
    }
}

const fn _default_overlay_size() -> u32 {
    10 // 10GB
}

#[derive(Deserialize, Debug, Clone)]
pub struct Role {
    pub name: String,
    pub rootfs_image: Utf8PathBuf,
    pub kernel_image: Utf8PathBuf,
    pub kernel_cmdline: Option<String>,
    pub cpus: u32,
    pub memory_size: u32,
    pub cache_size: u32,
    #[serde(default = "_default_overlay_size")]
    pub overlay_size: u32,
    pub instance_count: u8,
    #[serde(default)]
    pub cache_paths: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub labels: Vec<String>,
}

impl Role {
    pub fn slug(&self) -> String {
        self.name.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_file() {
        let config = ManagerConfig::from_file(&helpers::test_fixtures_file("config.toml"))
            .expect("Could not load config");

        assert_eq!(&config.network_interface, "eth0");
    }

    mod helpers {
        use camino::Utf8PathBuf;
        use std::env::current_dir;

        pub fn test_fixtures_path() -> Utf8PathBuf {
            let current_dir: Utf8PathBuf = current_dir()
                .expect("Could not get current dir")
                .try_into()
                .expect("Invalid path");

            // Check for two levels of nesting
            if current_dir.join("../test_fixtures").exists() {
                current_dir.join("../test_fixtures")
            } else {
                current_dir.join("../../test_fixtures")
            }
        }

        pub fn test_fixtures_file(file: &str) -> Utf8PathBuf {
            let path = test_fixtures_path().join(file);
            if !path.exists() {
                panic!("Test fixture file {:?} does not exist", path);
            }
            path
        }
    }
}

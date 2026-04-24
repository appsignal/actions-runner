use anyhow::Result;
use tracing::{info, instrument};

use crate::lvm;

#[derive(Debug)]
pub struct CacheDisk {
    pub volume_group: String,
    pub lv_name: String,
    pub size_gib: u32,
}

impl CacheDisk {
    pub fn new(volume_group: &str, lv_name: &str, size_gib: u32) -> Self {
        Self {
            volume_group: volume_group.to_string(),
            lv_name: lv_name.to_string(),
            size_gib,
        }
    }

    /// Device path for use in Firecracker config.
    pub fn device_path(&self) -> String {
        format!("/dev/{}/{}", self.volume_group, self.lv_name)
    }

    /// Create a snapshot from cache-empty for this slot's cache LV.
    #[instrument(skip(self), fields(lv = %self.lv_name))]
    pub fn setup(&self) -> Result<()> {
        lvm::lvcreate_snapshot(&self.volume_group, "cache-empty", &self.lv_name)?;
        info!(lv = %self.lv_name, "cache LV snapshot created");
        Ok(())
    }

    /// Check cache usage percentage. Uses df on the mounted LV device.
    pub fn usage_pct(&self) -> Result<u8> {
        let output = std::process::Command::new("df")
            .args(["--output=pcent", &self.device_path()])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output: "Use%\n  42%\n"
        let pct_str = stdout
            .lines()
            .nth(1)
            .unwrap_or("0")
            .trim()
            .trim_end_matches('%');
        Ok(pct_str.parse().unwrap_or(0))
    }

    /// Clear cache by removing and re-snapshotting from cache-empty.
    #[instrument(skip(self), fields(lv = %self.lv_name))]
    pub fn clear(&self) -> Result<()> {
        info!(lv = %self.lv_name, "clearing cache LV");
        lvm::lvremove(&self.volume_group, &self.lv_name)?;
        lvm::lvcreate_snapshot(&self.volume_group, "cache-empty", &self.lv_name)?;
        info!(lv = %self.lv_name, "cache LV cleared and re-created");
        Ok(())
    }
}

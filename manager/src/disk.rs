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

    /// Check cache usage as a percentage of the LV's virtual size.
    ///
    /// The cache LV is only ever mounted inside the guest VM (as /dev/vdb),
    /// never on the host — so `df` on the host cannot see its filesystem and
    /// reports the filesystem containing the device *node* (devtmpfs, ~0%)
    /// instead, meaning the clear threshold never fires. Read the thin LV's
    /// `data_percent` via lvs, which the host can always see. It slightly
    /// overestimates after guest-side deletes (thin blocks aren't returned
    /// without discard), which errs on the side of clearing — fine for a cache.
    pub fn usage_pct(&self) -> Result<u8> {
        let output = std::process::Command::new("lvs")
            .args([
                "--noheadings",
                "--options",
                "data_percent",
                &format!("{}/{}", self.volume_group, self.lv_name),
            ])
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "lvs failed for {}/{}: {}",
                self.volume_group,
                self.lv_name,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        // Output: "  92.98" (some locales emit a comma decimal separator)
        let raw = String::from_utf8_lossy(&output.stdout)
            .trim()
            .replace(',', ".");
        let pct: f64 = raw.parse().map_err(|e| {
            anyhow::anyhow!("failed to parse lvs data_percent {:?}: {}", raw, e)
        })?;
        Ok(pct.round().clamp(0.0, 100.0) as u8)
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

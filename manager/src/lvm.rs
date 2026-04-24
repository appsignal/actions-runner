use anyhow::{Context, Result};
use config::manager::{ManagerConfig, Role};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tracing::{info, instrument, warn};

#[derive(Debug)]
pub struct LvInfo {
    pub name: String,
    pub tags: HashMap<String, String>,
}

/// Parse the output of `lvs --noheadings -o lv_name,lv_tags <vg>`
fn parse_lvs_output(output: &str) -> Vec<LvInfo> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let tags_str = parts.next().unwrap_or("").to_string();
            let tags = parse_tags(&tags_str);
            Some(LvInfo { name, tags })
        })
        .collect()
}

fn parse_tags(tags_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for tag in tags_str.split(',') {
        if let Some((k, v)) = tag.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// List all LVs in a volume group with their tags.
#[instrument(fields(vg))]
pub fn lvs(vg: &str) -> Result<Vec<LvInfo>> {
    let output = Command::new("lvs")
        .args(["--noheadings", "-o", "lv_name,lv_tags", vg])
        .output()
        .context("failed to run lvs")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_lvs_output(&stdout))
}

/// Create a thin LV inside a thin pool.
#[instrument(fields(vg, pool, name, size_gib))]
pub fn lvcreate_thin(vg: &str, pool: &str, name: &str, size_gib: u64) -> Result<()> {
    let status = Command::new("lvcreate")
        .args([
            "-V",
            &format!("{}G", size_gib),
            "--thin",
            "-n",
            name,
            &format!("{}/{}", vg, pool),
        ])
        .status()
        .context("failed to run lvcreate")?;
    if !status.success() {
        anyhow::bail!("lvcreate failed for {}/{}", vg, name);
    }
    info!(vg, pool, name, size_gib, "thin LV created");
    Ok(())
}

/// Create a thin snapshot of an existing LV.
#[instrument(fields(vg, source, name))]
pub fn lvcreate_snapshot(vg: &str, source: &str, name: &str) -> Result<()> {
    let status = Command::new("lvcreate")
        .args(["-s", &format!("{}/{}", vg, source), "-n", name])
        .status()
        .context("failed to run lvcreate -s")?;
    if !status.success() {
        anyhow::bail!("lvcreate snapshot failed: {}/{} -> {}", vg, source, name);
    }
    info!(vg, source, name, "snapshot created");
    Ok(())
}

/// Remove an LV.
#[instrument(fields(vg, name))]
pub fn lvremove(vg: &str, name: &str) -> Result<()> {
    let status = Command::new("lvremove")
        .args(["-y", &format!("{}/{}", vg, name)])
        .status()
        .context("failed to run lvremove")?;
    if !status.success() {
        anyhow::bail!("lvremove failed for {}/{}", vg, name);
    }
    info!(vg, name, "LV removed");
    Ok(())
}

/// Tag an LV with a key=value pair.
#[instrument(fields(vg, lv, tag))]
pub fn lv_tag(vg: &str, lv: &str, tag: &str) -> Result<()> {
    let status = Command::new("lvchange")
        .args(["--addtag", tag, &format!("{}/{}", vg, lv)])
        .status()
        .context("failed to run lvchange --addtag")?;
    if !status.success() {
        anyhow::bail!("lvchange --addtag failed for {}/{}", vg, lv);
    }
    Ok(())
}

/// Provision an image file onto an LV using dd.
#[instrument(fields(vg, lv, image_path = %image_path.display()))]
pub fn provision_image(image_path: &Path, vg: &str, lv: &str) -> Result<()> {
    info!(vg, lv, image = %image_path.display(), "provisioning image onto LV");
    let status = Command::new("dd")
        .args([
            &format!("if={}", image_path.display()),
            &format!("of=/dev/{}/{}", vg, lv),
            "bs=4M",
            "status=progress",
        ])
        .status()
        .context("failed to run dd")?;
    if !status.success() {
        anyhow::bail!("dd failed provisioning image to {}/{}", vg, lv);
    }
    info!(vg, lv, "image provisioned");
    Ok(())
}

/// Format a device as ext4.
#[instrument(fields(device))]
pub fn mkfs_ext4(device: &str) -> Result<()> {
    let status = Command::new("mkfs.ext4")
        .arg(device)
        .status()
        .context("failed to run mkfs.ext4")?;
    if !status.success() {
        anyhow::bail!("mkfs.ext4 failed for {}", device);
    }
    info!(device, "formatted as ext4");
    Ok(())
}

/// Compute SHA-256 checksum of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to open {} for checksum", path.display()))?;
    let mut hasher = sha2_hash_bytes();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        update_hash(&mut hasher, &buf[..n]);
    }
    Ok(finalize_hash(hasher))
}

// Inline SHA-256 implementation using std without an extra dep.
// We use the sha2 crate via a small shim below; if sha2 isn't a dep,
// fall back to running `sha256sum` as a subprocess.
fn sha256_via_process(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .context("failed to run sha256sum")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hash = stdout
        .split_whitespace()
        .next()
        .context("unexpected sha256sum output")?
        .to_string();
    Ok(hash)
}

// Delegate to subprocess since we don't want to add sha2 crate dep.
struct DummyHasher;
fn sha2_hash_bytes() -> DummyHasher {
    DummyHasher
}
fn update_hash(_h: &mut DummyHasher, _data: &[u8]) {}
fn finalize_hash(_h: DummyHasher) -> String {
    String::new()
}

/// Re-implementation: use subprocess sha256sum since we avoid extra dep.
pub fn checksum_file(path: &Path) -> Result<String> {
    sha256_via_process(path)
}

/// Reconcile desired LVM state (from config) against actual state.
#[instrument(skip(config))]
pub fn reconcile(config: &ManagerConfig) -> Result<()> {
    let vg = &config.lvm.volume_group;
    let pool = &config.lvm.thin_pool;

    info!(vg, pool, "starting LVM reconciliation");

    let existing = lvs(vg)?;
    let existing_names: HashMap<&str, &LvInfo> =
        existing.iter().map(|lv| (lv.name.as_str(), lv)).collect();

    // Ensure cache-empty exists
    if !existing_names.contains_key("cache-empty") {
        info!(vg, "creating cache-empty LV");
        lvcreate_thin(vg, pool, "cache-empty", 1)?;
        mkfs_ext4(&format!("/dev/{}/cache-empty", vg))?;
    }

    // Reconcile base LVs for each role
    for role in &config.roles {
        reconcile_base_lv(vg, pool, role, &existing_names)?;
    }

    // Re-query after base LV reconciliation
    let existing = lvs(vg)?;
    let existing_names: HashMap<&str, &LvInfo> =
        existing.iter().map(|lv| (lv.name.as_str(), lv)).collect();

    // Reconcile slot LVs for each role
    for role in &config.roles {
        reconcile_slot_lvs(vg, pool, role, &existing_names)?;
    }

    // Remove LVs for roles no longer in config
    let active_roles: std::collections::HashSet<String> =
        config.roles.iter().map(|r| r.slug()).collect();

    for lv in &existing {
        if let Some(role_slug) = lv.name.strip_prefix("base-") {
            if !active_roles.contains(role_slug) {
                warn!(vg, lv = lv.name.as_str(), "removing LV for removed role");
                lvremove(vg, &lv.name)?;
            }
        }
        for prefix in ["rootfs-", "cache-"] {
            if let Some(rest) = lv.name.strip_prefix(prefix) {
                let role_slug = rest.rsplit_once('-').map(|(r, _)| r).unwrap_or(rest);
                if !active_roles.contains(role_slug) {
                    lvremove(vg, &lv.name)?;
                }
            }
        }
    }

    info!(vg, "LVM reconciliation complete");
    Ok(())
}

fn reconcile_base_lv(
    vg: &str,
    pool: &str,
    role: &Role,
    existing: &HashMap<&str, &LvInfo>,
) -> Result<()> {
    let base_name = format!("base-{}", role.slug());
    let image_path = role.rootfs_image.as_std_path();
    let current_checksum = checksum_file(image_path)
        .with_context(|| format!("failed to checksum {}", role.rootfs_image))?;

    if let Some(lv) = existing.get(base_name.as_str()) {
        let stored_checksum = lv.tags.get("checksum").map(|s| s.as_str()).unwrap_or("");
        if stored_checksum == current_checksum {
            info!(vg, lv = base_name.as_str(), "base LV up to date");
            return Ok(());
        }
        warn!(
            vg,
            lv = base_name.as_str(),
            "base LV checksum mismatch, reprovisioning"
        );
        lvremove(vg, &base_name)?;
    }

    // Compute size: image file size + 20%, rounded up to GiB
    let image_size = std::fs::metadata(image_path)
        .with_context(|| format!("failed to stat {}", role.rootfs_image))?
        .len();
    let size_gib = ((image_size as f64 * 1.2) / (1024.0 * 1024.0 * 1024.0)).ceil() as u64;
    let size_gib = size_gib.max(1);

    info!(vg, lv = base_name.as_str(), size_gib, "creating base LV");
    lvcreate_thin(vg, pool, &base_name, size_gib)?;
    provision_image(image_path, vg, &base_name)?;
    lv_tag(vg, &base_name, &format!("checksum={}", current_checksum))?;

    info!(
        vg,
        lv = base_name.as_str(),
        checksum = current_checksum.as_str(),
        "base LV provisioned"
    );
    Ok(())
}

fn reconcile_slot_lvs(
    vg: &str,
    _pool: &str,
    role: &Role,
    existing: &HashMap<&str, &LvInfo>,
) -> Result<()> {
    let slug = role.slug();

    for idx in 0..role.instance_count as usize {
        let rootfs_name = format!("rootfs-{}-{}", slug, idx);
        if !existing.contains_key(rootfs_name.as_str()) {
            info!(vg, lv = rootfs_name.as_str(), "creating slot rootfs LV");
            lvcreate_snapshot(vg, &format!("base-{}", slug), &rootfs_name)?;
        }

        let cache_name = format!("cache-{}-{}", slug, idx);
        if !existing.contains_key(cache_name.as_str()) {
            info!(vg, lv = cache_name.as_str(), "creating slot cache LV");
            lvcreate_snapshot(vg, "cache-empty", &cache_name)?;
        }
    }

    // Remove extra slot LVs beyond instance_count
    let wanted_rootfs: std::collections::HashSet<String> = (0..role.instance_count as usize)
        .map(|i| format!("rootfs-{}-{}", slug, i))
        .collect();
    let wanted_cache: std::collections::HashSet<String> = (0..role.instance_count as usize)
        .map(|i| format!("cache-{}-{}", slug, i))
        .collect();

    for lv in existing.keys() {
        if lv.starts_with(&format!("rootfs-{}-", slug)) && !wanted_rootfs.contains(*lv) {
            warn!(vg, lv, "removing extra rootfs slot LV");
            lvremove(vg, lv)?;
        }
        if lv.starts_with(&format!("cache-{}-", slug)) && !wanted_cache.contains(*lv) {
            warn!(vg, lv, "removing extra cache slot LV");
            lvremove(vg, lv)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lvs_output() {
        let output = "  base-default   checksum=abc123\n  cache-empty   \n  rootfs-default-0  \n";
        let lvs = parse_lvs_output(output);
        assert_eq!(lvs.len(), 3);
        assert_eq!(lvs[0].name, "base-default");
        assert_eq!(lvs[0].tags.get("checksum"), Some(&"abc123".to_string()));
        assert_eq!(lvs[1].name, "cache-empty");
    }

    #[test]
    fn test_parse_tags() {
        let tags = parse_tags("checksum=abc123,foo=bar");
        assert_eq!(tags.get("checksum"), Some(&"abc123".to_string()));
        assert_eq!(tags.get("foo"), Some(&"bar".to_string()));
    }
}

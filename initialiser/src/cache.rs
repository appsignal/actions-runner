use anyhow::Result;
use std::fs::{set_permissions, Permissions};
use std::os::unix::fs::{symlink, PermissionsExt};
use thiserror::Error;
use util::{fs, mount, CommandExecutionError};

const CACHE_PATH: &str = "/cache";

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("IO error: {:?}", self)]
    Io(#[from] std::io::Error),
    #[error("Could not mount: {:?}", self)]
    Mount(#[from] CommandExecutionError),
}

pub fn setup_cache(cache_str: &str) -> Result<(), CacheError> {
    fs::mkdir_p(CACHE_PATH)?;
    mount::mount_ext4("/dev/vdb", CACHE_PATH)?;
    set_permissions(CACHE_PATH, Permissions::from_mode(0o777))?;

    let cache_links = cache_str.split(',');
    for cache_link in cache_links {
        let cache_link = cache_link.trim();
        let cache_parts: Vec<&str> = cache_link.split(':').collect();
        if cache_parts.len() != 2 {
            return Err(CacheError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid cache link: {}", cache_link),
            )));
        }
        let cache_root = format!("{}/{}", CACHE_PATH, cache_parts[0]);
        let cache_path = cache_parts[1];

        fs::mkdir_p(&cache_root)?;
        symlink(&cache_root, cache_path)?;
    }

    Ok(())
}

use camino::Utf8PathBuf;
use log::*;
use std::env;
use thiserror::Error;
use util::{fs, mount};

pub mod docker;
pub mod qemu;

const MOUNT_PATH: &str = "/tmp/actions-runner/mnt";
const WORK_PATH: &str = "/tmp/actions-runner";
const DEFAULT_IMAGE_SIZE_GB: u8 = 10;

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("IO error: {:?}", .0)]
    IO(#[from] std::io::Error),
    #[error("Command execution error: {:?}", self)]
    Command(#[from] util::CommandExecutionError),
    #[error("Utf8 conversion error: {:?}", self)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Utf8 conversion error: {:?}", self)]
    PathBuf(#[from] camino::FromPathBufError),
    #[error("Docker build error: {:?}", .0)]
    DockerBuild(String),
    #[error("Qemu build error: {stderr}", stderr = "0.stderr")]
    QemuBuild(util::CommandExecutionError),
    #[error("Could not find our own binary: {:?}", .0)]
    SelfNotFound(std::io::Error),
}

pub struct Builder {
    own_path: Utf8PathBuf,
    source_path: Utf8PathBuf,
    output_path: Utf8PathBuf,
    work_path: Utf8PathBuf,
    mount_path: Utf8PathBuf,
    size_gb: u8,
}

impl Builder {
    pub fn new(
        source_path: &Utf8PathBuf,
        output_path: &Utf8PathBuf,
        size_gb: Option<u8>,
    ) -> Result<Self, BuildError> {
        let current_dir: Utf8PathBuf = env::current_dir()?.try_into()?;
        let own_path: Utf8PathBuf = env::current_exe()?.try_into()?;

        Ok(Self {
            own_path,
            source_path: [&current_dir, source_path].iter().collect(),
            output_path: [&current_dir, output_path].iter().collect(),
            size_gb: size_gb.unwrap_or(DEFAULT_IMAGE_SIZE_GB),
            work_path: WORK_PATH.into(),
            mount_path: MOUNT_PATH.into(),
        })
    }

    pub fn build_inner(&self) -> Result<(), BuildError> {
        // Build the given Dockerfile into an image, and get the image ID
        debug!("Building image from: '{}'", self.source_path);
        let image_id = docker::build_image(&self.source_path)?;

        // Get the container ID from the image ID
        let container_id = docker::create_container(&image_id)?;

        // Create the mount directory, we use this to copy the data into an image
        debug!("Creating directory on: '{}'", &self.mount_path);
        fs::mkdir_p(&self.mount_path)?;

        // Create the rootfs image, and mount it.
        debug!(
            "Creating rootfs in: '{}' with size: {}GB",
            &self.work_path, self.size_gb
        );
        let image_path = qemu::create_fs(&self.work_path, self.size_gb)?;

        // Create a filesystem on the image
        debug!("Creating ext4 filesystem on: {}", &image_path);
        fs::mkfs_ext4(&image_path)?;

        // Mount the image so we can add files to it
        debug!(
            "Mounting root image: '{}' on: {}",
            &image_path, self.mount_path
        );
        mount::mount_image(&image_path, &self.mount_path)?;

        // Copy the data from the container into the image
        debug!(
            "Exporting container: '{}' to: {}",
            &container_id, self.mount_path
        );
        docker::export_container(&container_id, &self.mount_path)?;

        // Copy our own binary into the image
        debug!(
            "Copy ourselves from '{}' to '{}'",
            &self.own_path,
            self.mount_path.join("sbin/actions-init")
        );
        fs::copy_sparse(&self.own_path, self.mount_path.join("sbin/actions-init"))?;

        debug!("Unmounting the image from: '{}'", &self.mount_path);
        // Unmount the image
        mount::unmount(&self.mount_path)?;

        debug!(
            "Copying image from: '{}' to: '{}'",
            &image_path, &self.output_path
        );
        fs::copy_sparse(&image_path, &self.output_path)?;

        // Cleanup
        fs::rm_rf(&self.work_path)?;

        debug!("Done!");
        Ok(())
    }

    // Runs the inner build, and cleans up after failure
    pub fn build(&self) -> Result<(), BuildError> {
        match self.build_inner() {
            Ok(res) => Ok(res),
            Err(e) => {
                // let _ = mount::unmount(&self.mount_path);
                //let _ = fs::rm_rf(&self.work_path);
                Err(e)
            }
        }
    }
}

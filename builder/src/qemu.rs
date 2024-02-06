use crate::BuildError;
use camino::Utf8PathBuf;
use std::process::Command;
use util::exec;

pub const IMAGE_NAME: &str = "image.ext4";

pub fn create_fs(path: &Utf8PathBuf, size_gb: u8) -> Result<Utf8PathBuf, BuildError> {
    let image_path = path.join(IMAGE_NAME);

    exec(Command::new("qemu-img").args([
        "create",
        "-f",
        "raw",
        image_path.as_str(),
        &format!("{}G", size_gb),
    ]))?;

    Ok(image_path)
}

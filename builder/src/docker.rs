use crate::BuildError;
use anyhow::Result;
use camino::Utf8PathBuf;
use std::process::Command;
use util::{exec, exec_spawn};

pub fn build_image(source_path: &Utf8PathBuf) -> Result<String, BuildError> {
    let output =
        exec(Command::new("docker").args(["build", "-q", "--file", source_path.as_str(), "."]))?;

    let trimmed_line = output
        .stdout
        .into_iter()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<Vec<u8>>();

    Ok(String::from_utf8_lossy(&trimmed_line).to_string())
}

pub fn create_container(image_id: &str) -> Result<String, BuildError> {
    let output = exec(Command::new("docker").args(["run", "-td", image_id]))?;

    // Get the container id from the output, ignoring whitespace and newlines
    let trimmed_line = output
        .stdout
        .into_iter()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<Vec<u8>>();

    Ok(String::from_utf8_lossy(&trimmed_line).to_string())
}

pub fn export_container(container_id: &str, mount_path: &Utf8PathBuf) -> Result<(), BuildError> {
    let output = exec_spawn(
        Command::new("docker")
            .args(["cp", &format!("{}:/", container_id), "-"])
            .stdout(std::process::Stdio::piped()),
    )?;

    let _ = exec(
        Command::new("tar")
            .args(["xf", "-", "-C", mount_path.as_str()])
            .stdin(output.stdout.unwrap()),
    )?;

    Ok(())
}

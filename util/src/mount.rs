use super::*;
use camino::Utf8Path;
use std::process::Command;

pub fn mount_image(
    from: impl AsRef<Utf8Path>,
    to: impl AsRef<Utf8Path>,
) -> Result<(), CommandExecutionError> {
    let from = from.as_ref();
    let to = to.as_ref();

    let _ = exec(Command::new("mount").args([from.as_str(), to.as_str()]))?;
    Ok(())
}

pub fn mount_ext4(
    from: impl AsRef<Utf8Path>,
    to: impl AsRef<Utf8Path>,
) -> Result<(), CommandExecutionError> {
    let from = from.as_ref();
    let to = to.as_ref();

    let _ = exec(Command::new("mount").args(["-t", "ext4", from.as_str(), to.as_str()]))?;
    Ok(())
}

pub fn unmount(path: impl AsRef<Utf8Path>) -> Result<(), CommandExecutionError> {
    let path = path.as_ref();
    let _ = exec(Command::new("umount").arg(path.as_str()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn test_mount_image() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "mount /dev/sda1 /mnt")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = mount_image("/dev/sda1", "/mnt");
        assert!(result.is_ok());
        ctx.checkpoint();
    }

    #[test]
    fn test_mount_ext4() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "mount -t ext4 /dev/sda1 /mnt")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = mount_ext4("/dev/sda1", "/mnt");
        assert!(result.is_ok());
        ctx.checkpoint();
    }

    #[test]
    fn test_unmount() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "umount /dev/sda1")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = unmount("/dev/sda1");
        assert!(result.is_ok());
        ctx.checkpoint();
    }
}

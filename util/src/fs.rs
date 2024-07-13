use super::*;
use camino::Utf8Path;
use std::process::Command;

pub fn copy_sparse(from: impl AsRef<Utf8Path>, to: impl AsRef<Utf8Path>) -> std::io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    exec(Command::new("cp").args(["--sparse=always", from.as_str(), to.as_str()]))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(())
}

pub fn rm_rf(path: impl AsRef<Utf8Path>) -> std::io::Result<()> {
    let path = path.as_ref();

    exec(Command::new("rm").args(["-rf", path.as_str()]))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(())
}

pub fn mkdir_p(path: impl AsRef<Utf8Path>) -> std::io::Result<()> {
    let path = path.as_ref();

    exec(Command::new("mkdir").args(["-p", path.as_str()]))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(())
}

pub fn mkfs_ext4(path: impl AsRef<Utf8Path>) -> std::io::Result<()> {
    let path = path.as_ref();

    exec(Command::new("mkfs.ext4").arg(path.as_str()))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(())
}

pub fn dd(path: impl AsRef<Utf8Path>, size_in_mb: u64) -> std::io::Result<()> {
    let path = path.as_ref();

    exec(Command::new("dd").args([
        "if=/dev/zero",
        &format!("of={}", &path),
        "conv=sparse",
        "bs=1M",
        &format!("count={}", size_in_mb),
    ]))
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(())
}

pub fn du(path: impl AsRef<Utf8Path>) -> std::io::Result<u64> {
    let path = path.as_ref();

    let du_output = exec(Command::new("du").args([&path.as_str()]))
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let size = du_output
        .split_whitespace()
        .next()
        .ok_or(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Couldn not split '{:?}' into number and rest", du_output),
        ))?;

    size.parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Could not parse '{:?}' to number: {}", size, e),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn test_copy_sparse() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "cp --sparse=always /foo.txt /bar.txt")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = copy_sparse("/foo.txt", "/bar.txt");
        assert!(result.is_ok());
        ctx.checkpoint();
    }

    #[test]
    fn test_rm_rf() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "rm -rf /foo.txt")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = rm_rf("/foo.txt");
        assert!(result.is_ok());
        ctx.checkpoint();
    }

    #[test]
    fn test_mkdir_p() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "mkdir -p /foo")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = mkdir_p("/foo");
        assert!(result.is_ok());
        ctx.checkpoint();
    }

    #[test]
    fn test_mkfs_ext4() {
        let _m = MTX.lock();

        let ctx = mock_inner::internal_exec_context();
        ctx.expect()
            .withf(|c| inner::to_string(c) == "mkfs.ext4 /dev/sda1")
            .returning(|_| {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let result = mkfs_ext4("/dev/sda1");
        assert!(result.is_ok());
        ctx.checkpoint();
    }
}

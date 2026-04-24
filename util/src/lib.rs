#[cfg(any(test, feature = "testing"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[allow(unused_imports)]
use std::sync::Mutex;
use std::time::Instant;

pub mod fs;
pub mod mount;
pub mod network;

pub fn hostname() -> String {
    nix::unistd::gethostname()
        .ok()
        .and_then(|h: std::ffi::OsString| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string())
}

pub static MTX: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub struct CommandResult {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

impl std::fmt::Display for CommandResult {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Command '{}' executed and failed with status: {}",
            self.command, self.status
        )?;
        write!(f, "  stdout: {}", self.stdout)?;
        write!(f, "  stderr: {}", self.stderr)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CommandExecutionError {
    #[error("Failed to start execution of '{command}': {err}")]
    ExecutionStart {
        command: String,
        err: std::io::Error,
    },

    #[error("{0}")]
    CommandFailure(Box<CommandResult>),
}

#[cfg_attr(
    any(test, automock, feature = "testing"),
    mockall::automock,
    allow(dead_code)
)]
pub mod inner {
    use super::*;

    pub fn to_string(command: &std::process::Command) -> String {
        format!(
            "{} {}",
            command.get_program().to_string_lossy(),
            command
                .get_args()
                .map(|s| s.to_string_lossy().into())
                .collect::<Vec<String>>()
                .join(" ")
        )
    }

    pub fn output_to_exec_error(
        command: &std::process::Command,
        output: &std::process::Output,
    ) -> CommandExecutionError {
        CommandExecutionError::CommandFailure(Box::new(CommandResult {
            command: to_string(command),
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }))
    }

    pub fn internal_exec(
        cmd: &mut std::process::Command,
    ) -> Result<std::process::Output, CommandExecutionError> {
        let start = Instant::now();

        let output = cmd
            .output()
            .map_err(|err| CommandExecutionError::ExecutionStart {
                command: to_string(cmd),
                err,
            })?;

        if !output.status.success() {
            return Err(output_to_exec_error(cmd, &output));
        }

        let duration = start.elapsed();
        tracing::trace!(cmd = ?cmd, duration_ms = duration.as_millis(), "command executed");

        Ok(output)
    }

    pub fn internal_exec_spawn(
        cmd: &mut std::process::Command,
    ) -> Result<std::process::Child, CommandExecutionError> {
        let output = cmd
            .spawn()
            .map_err(|err| CommandExecutionError::ExecutionStart {
                command: to_string(cmd),
                err,
            })?;

        Ok(output)
    }
}

#[cfg(any(test, feature = "testing"))]
pub static USE_MOCKS: AtomicBool = AtomicBool::new(true);

pub fn exec(
    cmd: &mut std::process::Command,
) -> Result<std::process::Output, CommandExecutionError> {
    tracing::trace!(
        program = ?cmd.get_program(),
        args = ?cmd.get_args().collect::<Vec<_>>(),
        "executing command"
    );
    #[cfg(any(test, feature = "testing"))]
    {
        if USE_MOCKS.load(Ordering::SeqCst) {
            return mock_inner::internal_exec(cmd);
        }
    }
    inner::internal_exec(cmd)
}

pub fn exec_spawn(
    cmd: &mut std::process::Command,
) -> Result<std::process::Child, CommandExecutionError> {
    tracing::trace!(
        program = ?cmd.get_program(),
        args = ?cmd.get_args().collect::<Vec<_>>(),
        "spawning command"
    );
    #[cfg(any(test, feature = "testing"))]
    {
        if USE_MOCKS.load(Ordering::SeqCst) {
            return mock_inner::internal_exec_spawn(cmd);
        }
    }
    inner::internal_exec_spawn(cmd)
}

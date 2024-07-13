use cfg_if::cfg_if;
#[allow(unused)]
use lazy_static::lazy_static;

#[allow(unused)]
use std::sync::atomic::{AtomicBool, Ordering};
#[allow(unused)]
use std::sync::Mutex;
use std::time::Instant;

pub mod fs;
pub mod mount;
pub mod network;

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
        log::trace!("Command {:?} executed in {}ms", cmd, duration.as_millis());

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
    log::trace!(
        "Executing command {:?} with args {:?}",
        cmd.get_program(),
        cmd.get_args()
    );
    cfg_if! {
        if #[cfg(any(test, feature = "testing"))] {
            if USE_MOCKS.load(Ordering::SeqCst) {
                mock_inner::internal_exec(cmd)
            } else {
                inner::internal_exec(cmd)
            }
        } else {
            inner::internal_exec(cmd)
        }
    }
}

pub fn exec_spawn(
    cmd: &mut std::process::Command,
) -> Result<std::process::Child, CommandExecutionError> {
    log::trace!(
        "Executing command {:?} with args {:?}",
        cmd.get_program(),
        cmd.get_args()
    );
    cfg_if! {
        if #[cfg(any(test, feature = "testing"))] {
            if USE_MOCKS.load(Ordering::SeqCst) {
                mock_inner::internal_exec_spawn(cmd)
            } else {
                inner::internal_exec_spawn(cmd)
            }
        } else {
            inner::internal_exec_spawn(cmd)
        }
    }
}

lazy_static! {
    pub static ref MTX: Mutex<()> = Mutex::new(());
}

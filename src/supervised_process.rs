//! Bounded child-process lifecycle. A cancelled command is killed and reaped.
//! Only commands explicitly authorized by a caller should be passed here.
use anyhow::{Context, Result};
use std::{ffi::OsStr, path::Path, time::Duration};
use tokio::{process::Command, time::timeout};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProcessOutcome {
    Exited { code: Option<i32>, stdout: Vec<u8>, stderr: Vec<u8> },
    TimedOut,
    Cancelled,
}

/// Spawn a direct child with piped output, then terminate and reap on timeout
/// or cancellation. This does not terminate grandchildren/process groups.
pub(crate) async fn run_supervised(
    executable: &OsStr,
    args: &[&OsStr],
    directory: &Path,
    deadline: Duration,
    cancellation: &crate::async_agent_loop::Cancellation,
) -> Result<ProcessOutcome> {
    if cancellation.is_cancelled() {
        return Ok(ProcessOutcome::Cancelled);
    }
    let mut command = Command::new(executable);
    command.args(args).current_dir(directory).kill_on_drop(true);
    let child = command
        .spawn()
        .with_context(|| format!("cannot start supervised process {executable:?}"))?;
    // wait_with_output consumes the child. kill_on_drop ensures cancellation
    // closes the direct child even when this future is dropped.
    let wait = child.wait_with_output();
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(ProcessOutcome::Cancelled),
        result = timeout(deadline, wait) => match result {
            Ok(Ok(output)) => Ok(ProcessOutcome::Exited {
                code: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
            }),
            Ok(Err(error)) => Err(error).context("supervised child wait failed"),
            Err(_) => Ok(ProcessOutcome::TimedOut),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_agent_loop::Cancellation;
    use std::ffi::OsStr;

    #[tokio::test]
    async fn cancellation_before_spawn_is_side_effect_free() {
        let cancellation = Cancellation::new();
        cancellation.cancel();
        let result = run_supervised(
            OsStr::new("nonexistent-ferrum-process"),
            &[],
            Path::new("."),
            Duration::from_secs(1),
            &cancellation,
        ).await.unwrap();
        assert_eq!(result, ProcessOutcome::Cancelled);
    }

    #[tokio::test]
    async fn missing_executable_reports_spawn_failure() {
        let result = run_supervised(
            OsStr::new("nonexistent-ferrum-process-123456"),
            &[],
            Path::new("."),
            Duration::from_secs(1),
            &Cancellation::new(),
        ).await;
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_terminates_direct_child() {
        let result = run_supervised(
            OsStr::new("/bin/sh"),
            &[OsStr::new("-c"), OsStr::new("sleep 30")],
            Path::new("."),
            Duration::from_millis(20),
            &Cancellation::new(),
        ).await.unwrap();
        assert_eq!(result, ProcessOutcome::TimedOut);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_child_returns_output() {
        let result = run_supervised(
            OsStr::new("/bin/sh"),
            &[OsStr::new("-c"), OsStr::new("printf hello")],
            Path::new("."),
            Duration::from_secs(2),
            &Cancellation::new(),
        ).await.unwrap();
        assert!(matches!(result, ProcessOutcome::Exited { code: Some(0), stdout, .. } if stdout == b"hello"));
    }
}

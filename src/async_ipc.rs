//! Bounded Tokio framed transport used by the production extension-worker supervisor.
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::time::{timeout_at, Instant};

const MAX_FRAME: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOutcome {
    NotStarted,
    Unknown,
}

#[derive(Debug)]
pub struct IpcError {
    pub outcome: IpcOutcome,
    pub kind: io::ErrorKind,
}

pub struct FramedIpc {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl FramedIpc {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
        }
    }

    pub async fn exchange(
        &mut self,
        request: &[u8],
        deadline: Instant,
    ) -> Result<Vec<u8>, IpcError> {
        let error = |outcome, kind| IpcError { outcome, kind };
        if request.len() > MAX_FRAME || request.contains(&b'\n') {
            return Err(error(IpcOutcome::NotStarted, io::ErrorKind::InvalidInput));
        }
        if Instant::now() >= deadline {
            return Err(error(IpcOutcome::NotStarted, io::ErrorKind::TimedOut));
        }
        let write = async {
            self.stdin.write_all(request).await?;
            self.stdin.write_all(b"\n").await?;
            self.stdin.flush().await
        };
        timeout_at(deadline, write)
            .await
            .map_err(|_| error(IpcOutcome::Unknown, io::ErrorKind::TimedOut))?
            .map_err(|e| error(IpcOutcome::Unknown, e.kind()))?;

        let mut frame = Vec::new();
        loop {
            let available = timeout_at(deadline, self.stdout.fill_buf())
                .await
                .map_err(|_| error(IpcOutcome::Unknown, io::ErrorKind::TimedOut))?
                .map_err(|e| error(IpcOutcome::Unknown, e.kind()))?;
            if available.is_empty() {
                return Err(error(IpcOutcome::Unknown, io::ErrorKind::UnexpectedEof));
            }
            let count = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |i| i + 1);
            if frame.len().saturating_add(count) > MAX_FRAME + 1 {
                return Err(error(IpcOutcome::Unknown, io::ErrorKind::InvalidData));
            }
            let complete = available[count - 1] == b'\n';
            frame.extend_from_slice(&available[..count]);
            self.stdout.consume(count);
            if complete {
                frame.pop();
                if frame.len() > MAX_FRAME {
                    return Err(error(IpcOutcome::Unknown, io::ErrorKind::InvalidData));
                }
                return Ok(frame);
            }
            if frame.len() > MAX_FRAME {
                return Err(error(IpcOutcome::Unknown, io::ErrorKind::InvalidData));
            }
        }
    }
}

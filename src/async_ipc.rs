//! Bounded Tokio framed transport used by the production extension-worker supervisor.
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
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

        read_frame(&mut self.stdout, deadline).await
    }
}

async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    deadline: Instant,
) -> Result<Vec<u8>, IpcError> {
    let error = |kind| IpcError {
        outcome: IpcOutcome::Unknown,
        kind,
    };
    let mut frame = Vec::new();
    loop {
        let available = timeout_at(deadline, reader.fill_buf())
            .await
            .map_err(|_| error(io::ErrorKind::TimedOut))?
            .map_err(|e| error(e.kind()))?;
        if available.is_empty() {
            return Err(error(io::ErrorKind::UnexpectedEof));
        }
        let count = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if frame.len().saturating_add(count) > MAX_FRAME + 1 {
            return Err(error(io::ErrorKind::InvalidData));
        }
        let complete = available[count - 1] == b'\n';
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            frame.pop();
            if frame.len() > MAX_FRAME {
                return Err(error(io::ErrorKind::InvalidData));
            }
            return Ok(frame);
        }
        if frame.len() > MAX_FRAME {
            return Err(error(io::ErrorKind::InvalidData));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncWriteExt};

    #[tokio::test]
    async fn truncated_response_is_unknown_eof() {
        let (mut writer, reader) = duplex(64);
        writer.write_all(br#"{"partial":true}"#).await.unwrap();
        drop(writer);
        let result = read_frame(
            &mut BufReader::new(reader),
            Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
        assert!(matches!(
            result,
            Err(IpcError {
                outcome: IpcOutcome::Unknown,
                kind: io::ErrorKind::UnexpectedEof
            })
        ));
    }

    #[tokio::test]
    async fn blocked_response_is_unknown_timeout() {
        let (_writer, reader) = duplex(64);
        let result = read_frame(
            &mut BufReader::new(reader),
            Instant::now() + std::time::Duration::from_millis(30),
        )
        .await;
        assert!(matches!(
            result,
            Err(IpcError {
                outcome: IpcOutcome::Unknown,
                kind: io::ErrorKind::TimedOut
            })
        ));
    }

    #[tokio::test]
    async fn oversized_response_is_unknown_invalid_data() {
        let (mut writer, reader) = duplex(MAX_FRAME + 128);
        writer.write_all(&vec![b'x'; MAX_FRAME + 1]).await.unwrap();
        drop(writer);
        let result = read_frame(
            &mut BufReader::new(reader),
            Instant::now() + std::time::Duration::from_secs(1),
        )
        .await;
        assert!(matches!(
            result,
            Err(IpcError {
                outcome: IpcOutcome::Unknown,
                kind: io::ErrorKind::InvalidData
            })
        ));
    }
}

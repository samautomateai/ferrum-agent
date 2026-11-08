//! Experimental Tokio actor: not yet integrated with the synchronous worker facade.
use crate::async_ipc::{FramedIpc, IpcError, IpcOutcome};
use crate::worker_lifecycle::{WorkerEvent, WorkerLifecycle};
use std::{io, path::Path, process::Stdio, time::Duration};
use tokio::{
    process::{Child, Command},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{timeout_at, Instant},
};

const COMMAND_CAPACITY: usize = 16;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum SupervisorError {
    NotStarted(io::ErrorKind),
    Unknown(io::ErrorKind),
    Closed,
}

impl From<IpcError> for SupervisorError {
    fn from(value: IpcError) -> Self {
        match value.outcome {
            IpcOutcome::NotStarted => Self::NotStarted(value.kind),
            IpcOutcome::Unknown => Self::Unknown(value.kind),
        }
    }
}

enum CommandMessage {
    Exchange {
        payload: Vec<u8>,
        deadline: Instant,
        reply: oneshot::Sender<Result<Vec<u8>, SupervisorError>>,
    },
    Shutdown { reply: oneshot::Sender<()> },
}

pub struct WorkerSupervisor {
    sender: mpsc::Sender<CommandMessage>,
    actor: JoinHandle<()>,
}

impl WorkerSupervisor {
    /// Must be called from within the shared Tokio runtime.
    pub fn spawn(executable: &Path, source: &Path) -> Result<Self, SupervisorError> {
        let mut child = Command::new(executable)
            .arg("--extension-worker")
            .arg("--worker-source")
            .arg(source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SupervisorError::NotStarted(e.kind()))?;
        let stdin = child.stdin.take().ok_or(SupervisorError::NotStarted(io::ErrorKind::BrokenPipe))?;
        let stdout = child.stdout.take().ok_or(SupervisorError::NotStarted(io::ErrorKind::BrokenPipe))?;
        let (sender, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let actor = tokio::spawn(actor_loop(child, FramedIpc::new(stdin, stdout), receiver));
        Ok(Self { sender, actor })
    }

    pub async fn exchange(&self, payload: Vec<u8>, deadline: Instant) -> Result<Vec<u8>, SupervisorError> {
        if Instant::now() >= deadline {
            return Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut));
        }
        let (reply, response) = oneshot::channel();
        timeout_at(deadline, self.sender.send(CommandMessage::Exchange { payload, deadline, reply }))
            .await
            .map_err(|_| SupervisorError::NotStarted(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Closed)?;
        timeout_at(deadline, response)
            .await
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::BrokenPipe))?
    }

    pub async fn shutdown(self) -> Result<(), SupervisorError> {
        let (reply, response) = oneshot::channel();
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        timeout_at(deadline, self.sender.send(CommandMessage::Shutdown { reply }))
            .await
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Closed)?;
        timeout_at(deadline, response)
            .await
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Closed)?;
        timeout_at(deadline, self.actor)
            .await
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Closed)?;
        Ok(())
    }
}

async fn actor_loop(mut child: Child, mut ipc: FramedIpc, mut receiver: mpsc::Receiver<CommandMessage>) {
    let mut lifecycle = WorkerLifecycle::new();
    if lifecycle.transition(WorkerEvent::Started).is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return;
    }
    while let Some(command) = receiver.recv().await {
        match command {
            CommandMessage::Exchange { payload, deadline, reply } => {
                let result = if Instant::now() >= deadline {
                    Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut))
                } else if lifecycle.transition(WorkerEvent::RequestAccepted).is_err() {
                    Err(SupervisorError::Closed)
                } else {
                    let result = ipc.exchange(&payload, deadline).await.map_err(Into::into);
                    if result.is_ok() {
                        let _ = lifecycle.transition(WorkerEvent::RequestCompleted);
                    } else {
                        let _ = lifecycle.transition(WorkerEvent::Fault);
                    }
                    result
                };
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed { break; }
            }
            CommandMessage::Shutdown { reply } => {
                let _ = lifecycle.transition(WorkerEvent::StopRequested);
                let _ = reply.send(());
                break;
            }
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    if lifecycle.state() == crate::worker_lifecycle::WorkerState::Stopping {
        let _ = lifecycle.transition(WorkerEvent::Stopped);
    }
}

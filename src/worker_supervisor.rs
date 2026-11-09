//! Tokio worker supervisor used by the synchronous extension-worker facade.\n//! Direct-child cleanup is best-effort during runtime teardown; OS process-tree\n//! containment and guaranteed reaping are tracked in the readiness checkpoint.
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
    Shutdown {
        deadline: Instant,
        reply: oneshot::Sender<Result<(), SupervisorError>>,
    },
}

pub struct WorkerSupervisor {
    sender: mpsc::Sender<CommandMessage>,
    actor: Option<JoinHandle<()>>,
    runtime: tokio::runtime::Handle,
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
        let stdin = child
            .stdin
            .take()
            .ok_or(SupervisorError::NotStarted(io::ErrorKind::BrokenPipe))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(SupervisorError::NotStarted(io::ErrorKind::BrokenPipe))?;
        let (sender, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let actor = tokio::spawn(actor_loop(child, FramedIpc::new(stdin, stdout), receiver));
        Ok(Self {
            sender,
            actor: Some(actor),
            runtime: tokio::runtime::Handle::current(),
        })
    }

    pub async fn exchange(
        &self,
        payload: Vec<u8>,
        deadline: Instant,
    ) -> Result<Vec<u8>, SupervisorError> {
        if Instant::now() >= deadline {
            return Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut));
        }
        // Reject malformed frames before queueing: no worker side effects can occur.
        if payload.len() > 2 * 1024 * 1024 || payload.contains(&b'\n') {
            return Err(SupervisorError::NotStarted(io::ErrorKind::InvalidInput));
        }
        let (reply, response) = oneshot::channel();
        timeout_at(
            deadline,
            self.sender.send(CommandMessage::Exchange {
                payload,
                deadline,
                reply,
            }),
        )
        .await
        .map_err(|_| SupervisorError::NotStarted(io::ErrorKind::TimedOut))?
        .map_err(|_| SupervisorError::Closed)?;
        timeout_at(deadline, response)
            .await
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::BrokenPipe))?
    }

    pub async fn shutdown(mut self) -> Result<(), SupervisorError> {
        let (reply, response) = oneshot::channel();
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        timeout_at(
            deadline,
            self.sender
                .send(CommandMessage::Shutdown { deadline, reply }),
        )
        .await
        .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
        .map_err(|_| SupervisorError::Closed)?;
        let shutdown_result = timeout_at(deadline, response)
            .await
            .map_err(|_| SupervisorError::Unknown(io::ErrorKind::TimedOut))?
            .map_err(|_| SupervisorError::Closed)?;
        let actor = self.actor.take().ok_or(SupervisorError::Closed)?;
        // Preserve ownership on deadline expiry: dropping a JoinHandle detaches it.
        // The reaper owns the task until it exits, even if the caller times out.
        let mut actor = actor;
        match timeout_at(deadline, &mut actor).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(SupervisorError::Closed),
            Err(_) => {
                self.runtime.spawn(async move {
                    let _ = actor.await;
                });
                return Err(SupervisorError::Unknown(io::ErrorKind::TimedOut));
            }
        }
        shutdown_result
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        // Closing the channel lets the actor kill and reap its child on exit.
        // Keep the JoinHandle owned by a reaper rather than detaching it.
        if let Some(actor) = self.actor.take() {
            self.runtime.spawn(async move {
                let _ = actor.await;
            });
        }
    }
}

async fn actor_loop(
    mut child: Child,
    mut ipc: FramedIpc,
    mut receiver: mpsc::Receiver<CommandMessage>,
) {
    let mut lifecycle = WorkerLifecycle::new();
    if lifecycle.transition(WorkerEvent::Started).is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return;
    }
    while let Some(command) = receiver.recv().await {
        match command {
            CommandMessage::Exchange {
                payload,
                deadline,
                reply,
            } => {
                let result = if reply.is_closed() || Instant::now() >= deadline {
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
                // A caller that abandons an in-flight request cannot trigger replay.
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    break;
                }
            }
            CommandMessage::Shutdown { deadline, reply } => {
                let _ = lifecycle.transition(WorkerEvent::StopRequested);
                // Acknowledgement must follow the shutdown handshake, not precede it.
                let handshake = ipc
                    .exchange(
                        br#"{"name":null,"arguments":null,"shutdown":true}"#,
                        deadline,
                    )
                    .await;
                drop(ipc);
                let exited = timeout_at(deadline, child.wait()).await;
                if !matches!(exited, Ok(Ok(_))) {
                    let _ = child.start_kill();
                    // Reap on the actor task; the caller deadline is not extended.
                    let _ = child.wait().await;
                }
                let result = match (handshake, exited) {
                    (Ok(response), Ok(Ok(status)))
                        if status.success() && shutdown_acknowledged(&response) =>
                    {
                        let _ = lifecycle.transition(WorkerEvent::Stopped);
                        Ok(())
                    }
                    (Err(error), _) => Err(SupervisorError::from(error)),
                    (_, Err(_)) => Err(SupervisorError::Unknown(io::ErrorKind::TimedOut)),
                    _ => Err(SupervisorError::Unknown(io::ErrorKind::InvalidData)),
                };
                if result.is_err() {
                    let _ = lifecycle.transition(WorkerEvent::Fault);
                }
                let _ = reply.send(result);
                return;
            }
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    if lifecycle.state() == crate::worker_lifecycle::WorkerState::Stopping {
        let _ = lifecycle.transition(WorkerEvent::Stopped);
    }
}

fn shutdown_acknowledged(response: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(response)
        .ok()
        .is_some_and(|value| {
            value.is_object()
                && value.get("error").is_none_or(serde_json::Value::is_null)
                && value.get("result").is_some()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_reply_is_detected_before_dispatch() {
        let (reply, receiver) = oneshot::channel::<Result<Vec<u8>, SupervisorError>>();
        assert!(!reply.is_closed());
        drop(receiver);
        assert!(reply.is_closed());
    }

    #[test]
    fn shutdown_ack_requires_valid_response() {
        assert!(shutdown_acknowledged(br#"{"result":null,"error":null}"#));
        assert!(!shutdown_acknowledged(
            br#"{"error":"failed","result":null}"#
        ));
        assert!(!shutdown_acknowledged(b"invalid"));
        assert!(!shutdown_acknowledged(br#"{"manifest":{}}"#));
        assert!(!shutdown_acknowledged(br#"{"result":null,"error":true}"#));
    }

    #[test]
    fn ipc_errors_preserve_side_effect_uncertainty() {
        assert!(matches!(
            SupervisorError::from(IpcError {
                outcome: IpcOutcome::Unknown,
                kind: io::ErrorKind::TimedOut,
            }),
            SupervisorError::Unknown(io::ErrorKind::TimedOut)
        ));
        assert!(matches!(
            SupervisorError::from(IpcError {
                outcome: IpcOutcome::NotStarted,
                kind: io::ErrorKind::InvalidInput,
            }),
            SupervisorError::NotStarted(io::ErrorKind::InvalidInput)
        ));
    }
}

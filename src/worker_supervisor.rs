//! Tokio worker supervisor used by the synchronous extension-worker facade.
//! A dedicated process-owner thread survives the shared Tokio runtime and
//! owns direct-child termination and wait/reaping. Descendants are out of scope.
use crate::async_ipc::{FramedIpc, IpcError, IpcOutcome};
use crate::worker_lifecycle::{WorkerEvent, WorkerLifecycle};
#[cfg(unix)]
#[path = "worker_process_group.rs"]
mod worker_process_group;
use std::{
    io,
    path::Path,
    process::{ExitStatus, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::{timeout_at, Instant},
};
#[cfg(unix)]
use worker_process_group::{native_kill_and_reap, terminate_worker_group};

const COMMAND_CAPACITY: usize = 16;
const MAX_PROCESS_OWNERS: usize = 64;
const OWNER_READY_TIMEOUT: Duration = Duration::from_secs(3);

struct OwnerSlots {
    active: Mutex<usize>,
}

fn owner_slots() -> &'static OwnerSlots {
    static SLOTS: OnceLock<OwnerSlots> = OnceLock::new();
    SLOTS.get_or_init(|| OwnerSlots {
        active: Mutex::new(0),
    })
}

struct OwnerPermit;

impl OwnerPermit {
    fn acquire() -> Result<Self, SupervisorError> {
        let slots = owner_slots();
        let mut active = slots.active.lock().unwrap_or_else(|e| e.into_inner());
        if *active >= MAX_PROCESS_OWNERS {
            return Err(SupervisorError::NotStarted(io::ErrorKind::WouldBlock));
        }
        *active += 1;
        Ok(Self)
    }
}

impl Drop for OwnerPermit {
    fn drop(&mut self) {
        let slots = owner_slots();
        let mut active = slots.active.lock().unwrap_or_else(|e| e.into_inner());
        *active -= 1;
    }
}

/// Emit only fixed lifecycle labels and error categories. Never serialize IPC payloads.
fn lifecycle_record(
    event: &'static str,
    pid: Option<u32>,
    error: Option<io::ErrorKind>,
) -> serde_json::Value {
    serde_json::json!({
        "component": "worker_supervisor",
        "event": event,
        "pid": pid,
        "error_kind": error.map(|kind| format!("{kind:?}")),
    })
}

fn lifecycle_event(event: &'static str, pid: Option<u32>, error: Option<io::ErrorKind>) {
    eprintln!("{}", lifecycle_record(event, pid, error));
}

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

// Last-resort OS wait path when the independent Tokio runtime cannot start.
// The process owner still holds the Child, so the PID cannot be reused before
// this thread completes its termination and wait attempt.
#[cfg(windows)]
fn native_kill_and_reap(pid: u32) -> io::Result<()> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn TerminateProcess(handle: *mut c_void, code: u32) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, millis: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const PROCESS_TERMINATE: u32 = 0x0001;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    const INFINITE: u32 = 0xffff_ffff;
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let _ = TerminateProcess(handle, 1);
        let result = WaitForSingleObject(handle, INFINITE);
        let error = if result == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        };
        let _ = CloseHandle(handle);
        error
    }
}

struct ProcessOwner {
    terminate: watch::Sender<bool>,
    status: watch::Receiver<Option<ExitStatus>>,
}

impl ProcessOwner {
    fn start(mut child: Child) -> Result<Arc<Self>, SupervisorError> {
        let permit = OwnerPermit::acquire()?;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (terminate, mut requested) = watch::channel(false);
        let (finished, status) = watch::channel(None);
        std::thread::Builder::new()
            .name("ferrum-worker-reaper".into())
            .spawn(move || {
                let _permit = permit;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let _ = ready_tx.send(runtime.as_ref().map(|_| ()).map_err(|e| e.kind()));
                if let Ok(runtime) = runtime {
                    runtime.block_on(async {
                        let outcome = tokio::select! {
                            result = child.wait() => result,
                            _ = async {
                                while !*requested.borrow_and_update() {
                                    if requested.changed().await.is_err() {
                                        break;
                                    }
                                }
                            } => {
                                #[cfg(unix)]
                                if let Some(pid) = child.id() {
                                    if let Err(error) = terminate_worker_group(pid) {
                                        lifecycle_event("group_terminate_failed", Some(pid), Some(error.kind()));
                                    }
                                }
                                let _ = child.start_kill();
                                child.wait().await
                            }
                        };
                        if let Ok(exit) = outcome {
                            let _ = finished.send(Some(exit));
                        }
                    });
                } else {
                    // Runtime-free OS termination/wait, including when Tokio cannot initialize.
                    if let Some(pid) = child.id() {
                        if let Err(error) = native_kill_and_reap(pid) {
                            lifecycle_event("native_reap_failed", Some(pid), Some(error.kind()));
                        }
                    }
                }
            })
            .map_err(|error| SupervisorError::NotStarted(error.kind()))?;
        match ready_rx.recv_timeout(OWNER_READY_TIMEOUT) {
            Ok(Ok(())) => Ok(Arc::new(Self { terminate, status })),
            Ok(Err(kind)) => Err(SupervisorError::NotStarted(kind)),
            Err(_) => Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut)),
        }
    }

    fn kill(&self) {
        let _ = self.terminate.send(true);
    }

    async fn wait(&self) -> io::Result<ExitStatus> {
        let mut status = self.status.clone();
        loop {
            if let Some(exit) = *status.borrow_and_update() {
                return Ok(exit);
            }
            status
                .changed()
                .await
                .map_err(|_| io::ErrorKind::BrokenPipe)?;
        }
    }
}

impl Drop for ProcessOwner {
    fn drop(&mut self) {
        self.kill();
    }
}

pub struct WorkerSupervisor {
    sender: mpsc::Sender<CommandMessage>,
    actor: Option<JoinHandle<()>>,
    runtime: tokio::runtime::Handle,
    #[allow(dead_code)]
    child_id: Option<u32>,
    #[allow(dead_code)]
    exited: watch::Receiver<bool>,
    #[allow(dead_code)]
    reaped: watch::Receiver<Option<ExitStatus>>,
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
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            // Establish containment before exec; process groups are not a sandbox.
            child.process_group(0);
        }
        let mut child = child
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
        let child_id = child.id();
        let owner = ProcessOwner::start(child)?;
        let reaped = owner.status.clone();
        lifecycle_event("spawned", child_id, None);
        let (sender, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (exit_sender, exited) = watch::channel(false);
        let actor = tokio::spawn(async move {
            actor_loop(owner, FramedIpc::new(stdin, stdout), receiver).await;
            lifecycle_event("actor_exited", child_id, None);
            let _ = exit_sender.send(true);
        });
        Ok(Self {
            sender,
            actor: Some(actor),
            runtime: tokio::runtime::Handle::current(),
            child_id,
            exited,
            reaped,
        })
    }

    /// PID of the direct worker process, for diagnostics and reaping tests.
    #[allow(dead_code)]
    pub fn child_id(&self) -> Option<u32> {
        self.child_id
    }

    /// Completes when the actor finishes its direct-child cleanup path.
    /// This does not establish descendant process-tree containment.
    #[allow(dead_code)]
    pub fn exit_observer(&self) -> watch::Receiver<bool> {
        self.exited.clone()
    }

    /// Reports an OS wait completion, independent of the shared Tokio runtime.
    #[allow(dead_code)]
    pub fn reap_observer(&self) -> watch::Receiver<Option<ExitStatus>> {
        self.reaped.clone()
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

    /// Submit a request without waiting for queue capacity. A saturated queue
    /// rejects before dispatch, so the caller can safely retry later.
    /// Once enqueued, a timeout is conservative: the worker may have run it.
    // The binary compiles this module separately; the nonblocking API is
    // exercised through the library's integration tests until CLI wiring lands.
    #[allow(dead_code)]
    pub async fn try_exchange(
        &self,
        payload: Vec<u8>,
        deadline: Instant,
    ) -> Result<Vec<u8>, SupervisorError> {
        if Instant::now() >= deadline {
            return Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut));
        }
        if payload.len() > 2 * 1024 * 1024 || payload.contains(&b'\n') {
            return Err(SupervisorError::NotStarted(io::ErrorKind::InvalidInput));
        }
        let (reply, response) = oneshot::channel();
        self.sender
            .try_send(CommandMessage::Exchange {
                payload,
                deadline,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    SupervisorError::NotStarted(io::ErrorKind::WouldBlock)
                }
                mpsc::error::TrySendError::Closed(_) => SupervisorError::Closed,
            })?;
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
        if shutdown_result.is_ok() {
            lifecycle_event("shutdown_acknowledged", self.child_id, None);
        }
        shutdown_result
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        // Closing the channel lets the actor kill and reap its child on exit.
        // Keep the JoinHandle owned by a reaper rather than detaching it.
        if let Some(actor) = self.actor.take() {
            lifecycle_event("supervisor_dropped", self.child_id, None);
            self.runtime.spawn(async move {
                let _ = actor.await;
            });
        }
    }
}

async fn actor_loop(
    child: Arc<ProcessOwner>,
    mut ipc: FramedIpc,
    mut receiver: mpsc::Receiver<CommandMessage>,
) {
    let mut lifecycle = WorkerLifecycle::new();
    if lifecycle.transition(WorkerEvent::Started).is_err() {
        child.kill();
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
                // Expired or abandoned queued requests were never dispatched.
                // They must not fault or terminate an otherwise healthy worker.
                if reply.is_closed() || Instant::now() >= deadline {
                    let _ = reply.send(Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut)));
                    continue;
                }
                if lifecycle.transition(WorkerEvent::RequestAccepted).is_err() {
                    let _ = reply.send(Err(SupervisorError::Closed));
                    break;
                }
                let result = ipc.exchange(&payload, deadline).await.map_err(Into::into);
                if result.is_ok() {
                    let _ = lifecycle.transition(WorkerEvent::RequestCompleted);
                } else {
                    let _ = lifecycle.transition(WorkerEvent::Fault);
                }
                // A dispatched request may have side effects: never replay it.
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    lifecycle_event("exchange_fault", None, None);
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
                    child.kill();
                    // The independent owner thread completes the OS wait/reap.
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
                    lifecycle_event("shutdown_fault", None, None);
                    let _ = lifecycle.transition(WorkerEvent::Fault);
                }
                let _ = reply.send(result);
                return;
            }
        }
    }
    child.kill();
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

    #[cfg(unix)]
    #[test]
    fn native_fallback_terminates_and_reaps_without_tokio_runtime() {
        let mut command = std::process::Command::new("sleep");
        command.arg("30");
        let child = command.spawn().expect("spawn direct child");
        let pid = child.id();
        native_kill_and_reap(pid).expect("native fallback must reap direct child");
        let mut status = 0_i32;
        unsafe extern "C" {
            fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        }
        assert_eq!(unsafe { waitpid(pid as i32, &mut status, 1) }, -1);
        drop(child);
    }

    #[cfg(windows)]
    #[test]
    fn native_fallback_terminates_and_waits_without_tokio_runtime() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .spawn()
            .expect("spawn direct child");
        native_kill_and_reap(child.id()).expect("native fallback must wait for child");
        assert!(child.try_wait().expect("read child status").is_some());
    }

    #[test]
    fn lifecycle_record_has_fixed_safe_fields() {
        let event = lifecycle_record("exchange_fault", Some(1234), Some(io::ErrorKind::TimedOut));
        assert_eq!(event["event"], "exchange_fault");
        assert_eq!(event["error_kind"], "TimedOut");
        assert_eq!(event.as_object().unwrap().len(), 4);
        assert!(event.get("payload").is_none());
        assert!(event.get("source").is_none());
    }

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

//! Additional real-worker fault-contract and process-liveness regression tests.
use ferrum::worker_supervisor::{SupervisorError, WorkerSupervisor};
use std::{fs, io, path::Path, time::Duration};
use tokio::time::Instant;

fn extension() -> tempfile::NamedTempFile {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        "pi.registerTool({name:'echo',execute:(args)=>args});",
    )
    .unwrap();
    source
}

fn worker_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_ferrum"))
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[cfg(target_os = "linux")]
fn assert_linux_child_reaped(pid: u32) {
    assert!(
        !Path::new("/proc").join(pid.to_string()).exists(),
        "worker PID {pid} remains visible after actor cleanup"
    );
}

#[tokio::test]
async fn missing_worker_binary_is_not_started_and_does_not_spawn_actor() {
    let source = extension();
    let missing = source.path().with_extension("missing-worker-executable");
    let error = match WorkerSupervisor::spawn(&missing, source.path()) {
        Ok(_) => panic!("missing executable unexpectedly spawned"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        SupervisorError::NotStarted(io::ErrorKind::NotFound)
    ));
}

#[tokio::test]
async fn worker_crash_rejects_followup_without_restarting_child() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(source.path(), "not valid javascript !").unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let pid = worker.child_id().expect("worker PID");
    let mut exited = worker.exit_observer();
    let result = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(result, Err(SupervisorError::Unknown(_))));
    tokio::time::timeout(Duration::from_secs(5), exited.wait_for(|done| *done))
        .await
        .expect("crashed child cleanup timed out")
        .expect("cleanup observer closed");
    assert_eq!(worker.child_id(), Some(pid), "worker must not be restarted");
    let followup = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(followup, Err(SupervisorError::Closed)));
    #[cfg(target_os = "linux")]
    assert_linux_child_reaped(pid);
}

#[tokio::test]
async fn rejected_request_keeps_original_child_alive_for_valid_followup() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let pid = worker.child_id().expect("worker PID");
    let invalid = worker.exchange(b"two\nframes".to_vec(), deadline()).await;
    assert!(matches!(
        invalid,
        Err(SupervisorError::NotStarted(io::ErrorKind::InvalidInput))
    ));
    let valid = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&valid).unwrap();
    assert!(decoded["manifest"].is_object());
    assert_eq!(worker.child_id(), Some(pid));
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelled_queued_request_does_not_terminate_healthy_worker() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        concat!(
            "pi.registerTool({name:'pause',execute:()=>{",
            "const end=Date.now()+250;while(Date.now()<end){};",
            "return 'done';}});"
        ),
    )
    .unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let first = worker.exchange(br#"{"name":"pause","arguments":{}}"#.to_vec(), deadline());
    let queued = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        worker
            .exchange(
                br#"{"name":null,"arguments":null}"#.to_vec(),
                Instant::now() + Duration::from_millis(30),
            )
            .await
    };
    let (first_result, queued_result) = tokio::join!(first, queued);
    assert!(
        first_result.is_ok(),
        "first dispatched request should succeed"
    );
    assert!(matches!(
        queued_result,
        Err(SupervisorError::Unknown(io::ErrorKind::TimedOut))
    ));
    // The queued request timed out before actor dispatch. Its stale command
    // must be skipped without poisoning the worker.
    let response = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert!(decoded["manifest"].is_object());
    worker.shutdown().await.unwrap();
}

#[cfg(target_os = "windows")]
fn assert_windows_child_reaped(pid: u32) {
    // tasklist is a native Windows OS query; an actor completion flag alone
    // does not establish that the direct child has disappeared.
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist must be available on Windows CI");
    assert!(output.status.success(), "tasklist failed");
    let listing = String::from_utf8_lossy(&output.stdout);
    let present = listing.lines().any(|line| {
        let fields: Vec<_> = line.trim().trim_matches('"').split("\",\"").collect();
        fields
            .get(1)
            .is_some_and(|field| field.trim_matches('"') == pid.to_string())
    });
    assert!(
        !present,
        "worker PID {pid} remains in Windows process table"
    );
}

#[cfg(target_os = "windows")]
#[tokio::test]
async fn windows_direct_child_disappears_after_graceful_shutdown() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let pid = worker.child_id().expect("worker PID");
    worker.shutdown().await.unwrap();
    assert_windows_child_reaped(pid);
}

#[cfg(target_os = "windows")]
#[tokio::test]
async fn windows_direct_child_disappears_after_drop() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let pid = worker.child_id().expect("worker PID");
    let mut exited = worker.exit_observer();
    drop(worker);
    tokio::time::timeout(Duration::from_secs(5), exited.wait_for(|done| *done))
        .await
        .expect("drop cleanup timed out")
        .expect("drop cleanup observer closed");
    assert_windows_child_reaped(pid);
}

#[tokio::test]
async fn busy_worker_shutdown_is_bounded_and_cleanup_completes() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        "pi.registerTool({name:'hang',execute:()=>{while(true){}}});",
    )
    .unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let mut exited = worker.exit_observer();
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    // Drive the in-flight request to completion before consuming the supervisor.
    // A concurrent shutdown(self) would move a value still borrowed by exchange(&self).
    let request_result = worker
        .exchange(
            br#"{"name":"hang","arguments":{}}"#.to_vec(),
            Instant::now() + Duration::from_millis(250),
        )
        .await;
    assert!(matches!(
        request_result,
        Err(SupervisorError::Unknown(io::ErrorKind::TimedOut))
    ));
    let shutdown_result = tokio::time::timeout(Duration::from_secs(5), worker.shutdown())
        .await
        .expect("shutdown must remain bounded after a hung invocation");
    assert!(
        shutdown_result.is_err(),
        "hung worker cannot acknowledge shutdown"
    );
    tokio::time::timeout(Duration::from_secs(5), exited.wait_for(|done| *done))
        .await
        .expect("busy worker cleanup timed out")
        .expect("cleanup observer closed");
}

#[tokio::test]
async fn try_exchange_processes_real_worker_request_without_waiting_for_queue_slot() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let response = worker
        .try_exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert!(decoded["manifest"].is_object());
    assert!(matches!(
        worker
            .try_exchange(b"invalid\nframe".to_vec(), deadline())
            .await,
        Err(SupervisorError::NotStarted(io::ErrorKind::InvalidInput))
    ));
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn saturated_queue_rejects_without_dispatch_and_recovers() {
    use std::sync::Arc;

    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        concat!(
            "pi.registerTool({name:'pause',execute:()=>{",
            "const end=Date.now()+900;while(Date.now()<end){};",
            "return 'done';}});"
        ),
    )
    .unwrap();
    let worker = Arc::new(WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap());
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();

    let active_worker = Arc::clone(&worker);
    let active = tokio::spawn(async move {
        active_worker
            .exchange(br#"{"name":"pause","arguments":{}}"#.to_vec(), deadline())
            .await
    });
    tokio::time::sleep(Duration::from_millis(80)).await;

    // The actor is executing pause; exactly sixteen waiting requests fill
    // the bounded command channel. Spawn each to poll exchange immediately.
    let mut pending = Vec::new();
    for _ in 0..16 {
        let queued_worker = Arc::clone(&worker);
        pending.push(tokio::spawn(async move {
            queued_worker
                .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
                .await
        }));
    }
    tokio::time::sleep(Duration::from_millis(80)).await;
    let rejected = worker
        .try_exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(
        rejected,
        Err(SupervisorError::NotStarted(io::ErrorKind::WouldBlock))
    ));
    assert!(active.await.unwrap().is_ok());
    for queued in pending {
        assert!(queued.await.unwrap().is_ok());
    }
    let recovered = worker
        .try_exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&recovered).is_ok());
    Arc::try_unwrap(worker)
        .unwrap_or_else(|_| panic!("all exchange tasks must release the supervisor"))
        .shutdown()
        .await
        .unwrap();
}

#[tokio::test]
async fn aborted_queued_request_is_skipped_without_poisoning_worker() {
    use std::sync::Arc;

    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        concat!(
            "pi.registerTool({name:'pause',execute:()=>{",
            "const end=Date.now()+450;while(Date.now()<end){};",
            "return 'done';}});"
        ),
    )
    .unwrap();
    let worker = Arc::new(WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap());
    let pid = worker.child_id();
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();

    let active_worker = Arc::clone(&worker);
    let active = tokio::spawn(async move {
        active_worker
            .exchange(br#"{"name":"pause","arguments":{}}"#.to_vec(), deadline())
            .await
    });
    tokio::time::sleep(Duration::from_millis(70)).await;

    let cancelled_worker = Arc::clone(&worker);
    let queued = tokio::spawn(async move {
        cancelled_worker
            .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());

    assert!(active.await.unwrap().is_ok());
    let followup = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&followup).unwrap();
    assert!(parsed["manifest"].is_object());
    assert_eq!(worker.child_id(), pid);
    Arc::try_unwrap(worker)
        .unwrap_or_else(|_| panic!("exchange tasks must release supervisor"))
        .shutdown()
        .await
        .unwrap();
}

#[tokio::test]
async fn blocked_response_times_out_and_reaps_direct_child() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        "pi.registerTool({name:'block',execute:()=>{while(true){}}});",
    )
    .unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let pid = worker.child_id().expect("worker PID");
    let mut exited = worker.exit_observer();
    let started = Instant::now();
    let result = worker
        .exchange(
            br#"{"name":"block","arguments":{}}"#.to_vec(),
            started + Duration::from_millis(200),
        )
        .await;
    assert!(matches!(
        result,
        Err(SupervisorError::Unknown(io::ErrorKind::TimedOut))
    ));
    assert!(started.elapsed() < Duration::from_secs(5));
    tokio::time::timeout(Duration::from_secs(5), exited.wait_for(|done| *done))
        .await
        .expect("blocked-response actor cleanup timed out")
        .expect("exit observer closed");
    #[cfg(target_os = "linux")]
    assert_linux_child_reaped(pid);
    #[cfg(target_os = "windows")]
    assert_windows_child_reaped(pid);
    let followup = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(followup, Err(SupervisorError::Closed)));
}

#[tokio::test]
async fn dropping_supervisor_while_response_blocked_reaps_child() {
    use std::sync::Arc;

    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        "pi.registerTool({name:'block',execute:()=>{while(true){}}});",
    )
    .unwrap();
    let worker = Arc::new(WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap());
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let pid = worker.child_id().expect("worker PID");
    let mut exited = worker.exit_observer();
    let request_worker = Arc::clone(&worker);
    let request = tokio::spawn(async move {
        request_worker
            .exchange(
                br#"{"name":"block","arguments":{}}"#.to_vec(),
                Instant::now() + Duration::from_millis(300),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(80)).await;
    request.abort();
    let _ = request.await;
    drop(worker);
    tokio::time::timeout(Duration::from_secs(5), exited.wait_for(|done| *done))
        .await
        .expect("drop must terminate blocked child")
        .expect("exit observer closed");
    #[cfg(target_os = "linux")]
    assert_linux_child_reaped(pid);
    #[cfg(target_os = "windows")]
    assert_windows_child_reaped(pid);
}

#[tokio::test]
async fn cancelled_dispatched_request_is_never_replayed() {
    use std::sync::Arc;

    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        concat!(
            "let calls=0;",
            "pi.registerTool({name:'count',execute:()=>{",
            "calls++;const end=Date.now()+350;while(Date.now()<end){};",
            "return {calls};}});",
            "pi.registerTool({name:'readCount',execute:()=>({calls})});"
        ),
    )
    .unwrap();
    let worker = Arc::new(WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap());
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let active_worker = Arc::clone(&worker);
    let request = tokio::spawn(async move {
        active_worker
            .exchange(br#"{"name":"count","arguments":{}}"#.to_vec(), deadline())
            .await
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    tokio::time::sleep(Duration::from_millis(350)).await;
    let response = worker
        .exchange(
            br#"{"name":"readCount","arguments":{}}"#.to_vec(),
            deadline(),
        )
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(
        parsed["result"]["calls"], 1,
        "cancelled invocation must not replay"
    );
    Arc::try_unwrap(worker)
        .unwrap_or_else(|_| panic!("request must release worker"))
        .shutdown()
        .await
        .unwrap();
}

#[test]
fn runtime_teardown_terminates_direct_worker_process() {
    let source = extension();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let worker = runtime.block_on(async {
        let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
        worker
            .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
            .await
            .unwrap();
        worker
    });
    let pid = worker.child_id().expect("worker PID");
    // The supervisor is deliberately still alive when the runtime stops.
    // This exercises Tokio's kill-on-drop fallback, not normal actor cleanup.
    runtime.shutdown_timeout(Duration::from_millis(200));
    drop(worker);
    let started = std::time::Instant::now();
    loop {
        #[cfg(target_os = "linux")]
        let gone = !Path::new("/proc").join(pid.to_string()).exists();
        #[cfg(target_os = "windows")]
        let gone = {
            let output = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output()
                .expect("tasklist");
            let listing = String::from_utf8_lossy(&output.stdout);
            !listing.lines().any(|line| {
                let fields: Vec<_> = line.trim().trim_matches('"').split("\",\"").collect();
                fields.get(1).is_some_and(|field| field.trim_matches('"') == pid.to_string())
            })
        };
        if gone {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "worker {pid} survived runtime teardown"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

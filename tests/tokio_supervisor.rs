//! End-to-end tests for the staged Tokio supervisor using the real Ferrum worker binary.
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

#[tokio::test]
async fn supervisor_exchanges_multiple_frames_and_shuts_down() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let hello = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&hello).unwrap();
    assert!(manifest["manifest"].is_object());
    for value in ["first", "second", "third"] {
        let request = serde_json::json!({"name":"echo","arguments":{"value":value}});
        let response = worker
            .exchange(serde_json::to_vec(&request).unwrap(), deadline())
            .await
            .unwrap();
        let decoded: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(decoded["result"]["value"], value);
    }
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn supervisor_rejects_oversized_and_multiline_requests_before_dispatch() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    for payload in [b"invalid\nframe".to_vec(), vec![b'x'; 2 * 1024 * 1024 + 1]] {
        assert!(matches!(
            worker.exchange(payload, deadline()).await,
            Err(SupervisorError::NotStarted(io::ErrorKind::InvalidInput))
        ));
    }
    // A rejected request does not poison the worker.
    let response = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&response).is_ok());
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn expired_deadline_is_not_started_and_worker_remains_usable() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    assert!(matches!(
        worker.exchange(b"{}".to_vec(), Instant::now()).await,
        Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut))
    ));
    assert!(worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .is_ok());
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn worker_crash_closes_actor_without_replaying_request() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(source.path(), "not valid javascript !").unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let first = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(first, Err(SupervisorError::Unknown(_))));
    let second = worker.exchange(b"{}".to_vec(), deadline()).await;
    // The actor may be closing while the second request enters its bounded queue.
    // Both Closed and Unknown(BrokenPipe) are conservative, non-replayed outcomes.
    assert!(matches!(
        second,
        Err(SupervisorError::Closed) | Err(SupervisorError::Unknown(io::ErrorKind::BrokenPipe))
    ));
}

#[tokio::test]
async fn nonexistent_worker_executable_is_not_started() {
    let directory = tempfile::tempdir().unwrap();
    let source = extension();
    let result = WorkerSupervisor::spawn(&directory.path().join("absent-ferrum"), source.path());
    assert!(matches!(result, Err(SupervisorError::NotStarted(_))));
}

#[tokio::test]
async fn worker_error_response_does_not_corrupt_transport() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let reply = worker
        .exchange(br#"{"name":"missing","arguments":{}}"#.to_vec(), deadline())
        .await
        .unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    assert!(decoded["error"].is_string());
    assert!(worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .is_ok());
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn independent_workers_exchange_concurrently_and_reap_cleanly() {
    let first_source = extension();
    let second_source = extension();
    let first = WorkerSupervisor::spawn(worker_binary(), first_source.path()).unwrap();
    let second = WorkerSupervisor::spawn(worker_binary(), second_source.path()).unwrap();

    let (first_hello, second_hello) = tokio::join!(
        first.exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline()),
        second.exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
    );
    for response in [first_hello.unwrap(), second_hello.unwrap()] {
        let decoded: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert!(decoded["manifest"].is_object());
    }

    let (first_stop, second_stop) = tokio::join!(first.shutdown(), second.shutdown());
    first_stop.unwrap();
    second_stop.unwrap();
}

#[tokio::test]
async fn repeated_worker_start_exchange_and_shutdown_cycles() {
    let source = extension();
    for cycle in 0..5 {
        let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
        let hello = worker
            .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
            .await
            .unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&hello).unwrap()["manifest"].is_object(),
            "missing manifest on cycle {cycle}"
        );
        worker.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn timed_out_inflight_request_has_unknown_outcome_and_is_not_replayed() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        source.path(),
        "pi.registerTool({name:'hang',execute:()=>{while(true){}}});",
    )
    .unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    let request = br#"{"name":"hang","arguments":{}}"#.to_vec();
    let result = worker
        .exchange(request, Instant::now() + Duration::from_millis(250))
        .await;
    assert!(matches!(
        result,
        Err(SupervisorError::Unknown(io::ErrorKind::TimedOut))
    ));
    // The supervisor must not retry an invocation whose side effects are uncertain.
    // Dropping the sender lets the actor finish its failed exchange and reap the child.
    drop(worker);
}

#[tokio::test]
async fn cancelled_queued_caller_does_not_poison_healthy_worker() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let cancelled = worker.exchange(
        br#"{"name":null,"arguments":null}"#.to_vec(),
        Instant::now(),
    );
    assert!(matches!(
        cancelled.await,
        Err(SupervisorError::NotStarted(io::ErrorKind::TimedOut))
    ));
    let reply = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&reply).is_ok());
    worker.shutdown().await.unwrap();
}

#[test]
fn supervisor_drop_outside_async_context_does_not_panic() {
    let source = extension();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let worker = runtime
        .block_on(async { WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap() });
    // A synchronous facade drops the supervisor without an entered Tokio context.
    // The supervisor must use its captured runtime handle for cleanup.
    drop(worker);
    runtime.block_on(async {
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
}

#[tokio::test]
async fn cancelling_an_inflight_exchange_does_not_replay_or_poison_next_request() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await
        .unwrap();

    // Drop an exchange future after dispatch. The actor must retain ownership
    // of its IPC operation and must not retry it on behalf of the next caller.
    let request = serde_json::json!({"name":"echo","arguments":{"value":"cancelled"}});
    let mut inflight = Box::pin(worker.exchange(serde_json::to_vec(&request).unwrap(), deadline()));
    tokio::select! {
        _ = &mut inflight => {}
        _ = tokio::task::yield_now() => {}
    }
    drop(inflight);

    let next = serde_json::json!({"name":"echo","arguments":{"value":"next"}});
    let response = worker
        .exchange(serde_json::to_vec(&next).unwrap(), deadline())
        .await
        .unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(decoded["result"]["value"], "next");
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_after_worker_crash_is_bounded_and_reports_failure() {
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(source.path(), "not valid javascript !").unwrap();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let first = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(first, Err(SupervisorError::Unknown(_))));

    // A crashed actor must not hang shutdown while trying to enqueue a
    // handshake that can no longer be processed.
    let shutdown = tokio::time::timeout(Duration::from_secs(2), worker.shutdown())
        .await
        .expect("shutdown hung after worker crash");
    assert!(
        shutdown.is_err(),
        "crashed worker cannot acknowledge shutdown"
    );
}

#[tokio::test]
async fn malformed_json_request_closes_worker_without_replay() {
    let source = extension();
    let worker = WorkerSupervisor::spawn(worker_binary(), source.path()).unwrap();
    let first = worker
        .exchange(b"{invalid-json}".to_vec(), deadline())
        .await;
    // The worker rejects invalid protocol JSON and exits without a reply.
    // The supervisor must conservatively report an unknown outcome.
    assert!(matches!(first, Err(SupervisorError::Unknown(_))));
    let second = worker
        .exchange(br#"{"name":null,"arguments":null}"#.to_vec(), deadline())
        .await;
    assert!(matches!(
        second,
        Err(SupervisorError::Closed) | Err(SupervisorError::Unknown(_))
    ));
}

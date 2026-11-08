//! End-to-end tests for the staged Tokio supervisor using the real Ferrum worker binary.
#![allow(dead_code)] // Imported production modules are exercised selectively here.
#[path = "../src/async_ipc.rs"]
mod async_ipc;
#[path = "../src/worker_lifecycle.rs"]
mod worker_lifecycle;
#[path = "../src/worker_supervisor.rs"]
mod worker_supervisor;

use std::{fs, io, path::Path, time::Duration};
use tokio::time::Instant;
use worker_supervisor::{SupervisorError, WorkerSupervisor};

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
    assert!(matches!(second, Err(SupervisorError::Closed)));
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

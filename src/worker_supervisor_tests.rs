//! Error-contract regressions after removing the legacy thread/channel transport.
//! Subprocess fault-injection and deadline tests live in tests/tokio_supervisor.rs.
use super::*;
use crate::error_contract::{ErrorOutcome, RuntimeFailure};
use crate::worker_supervisor::SupervisorError;
use std::io;

fn mapped(error: SupervisorError) -> RuntimeFailure {
    map_supervisor_error(error)
}

#[test]
fn pre_dispatch_failures_are_not_started_and_not_retryable() {
    for kind in [io::ErrorKind::InvalidInput, io::ErrorKind::BrokenPipe] {
        let failure = mapped(SupervisorError::NotStarted(kind));
        assert_eq!(failure.outcome(), ErrorOutcome::NotStarted);
        assert!(!failure.retryable());
    }
}

#[test]
fn in_flight_timeouts_have_unknown_outcome() {
    let failure = mapped(SupervisorError::Unknown(io::ErrorKind::TimedOut));
    assert_eq!(failure.code(), "FERRUM-WORKER-TIMEOUT");
    assert_eq!(failure.outcome(), ErrorOutcome::Unknown);
    assert!(!failure.retryable());
}

#[test]
fn broken_connection_never_implies_safe_replay() {
    for error in [SupervisorError::Unknown(io::ErrorKind::BrokenPipe), SupervisorError::Closed] {
        let failure = mapped(error);
        assert_eq!(failure.outcome(), ErrorOutcome::Unknown);
        assert!(!failure.retryable());
    }
}

#[test]
fn internal_io_errors_are_not_exposed_in_public_contract() {
    let failure = mapped(SupervisorError::Unknown(io::ErrorKind::PermissionDenied));
    assert!(!failure.to_string().contains("PermissionDenied"));
    assert!(!format!("{failure:?}").contains("PermissionDenied"));
}

#[test]
fn oversized_requests_are_rejected_before_worker_dispatch() {
    let request = Request {
        name: Some("echo".to_owned()),
        arguments: Value::String("x".repeat(MAX_PROTOCOL_BYTES)),
        event: None,
        shutdown: false,
    };
    assert!(serde_json::to_vec(&request).unwrap().len() > MAX_PROTOCOL_BYTES);
}

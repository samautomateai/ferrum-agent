use super::*;
use crate::error_contract::{ErrorOutcome, RuntimeFailure};

fn dummy_request() -> Request {
    Request {
        name: Some("echo".to_owned()),
        arguments: Value::Null,
        event: None,
        shutdown: false,
    }
}

#[test]
fn disconnected_writer_does_not_start_request() {
    let (writes, reader) = mpsc::sync_channel(1);
    drop(reader);
    let (_result_sender, write_results) = mpsc::sync_channel(1);
    let (_reply_sender, replies) = mpsc::sync_channel(1);
    let mut process = Process {
        child: Command::new(std::env::current_exe().unwrap())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        writes,
        write_results,
        replies,
    };
    let failure = process
        .request(&dummy_request(), Duration::from_secs(1))
        .err()
        .expect("expected worker failure");
    let typed = failure.downcast_ref::<RuntimeFailure>().unwrap();
    assert_eq!(typed.outcome(), ErrorOutcome::NotStarted);
    assert!(!typed.retryable());
}

#[test]
fn writer_acknowledgement_timeout_is_bounded() {
    let (writes, _reader) = mpsc::sync_channel(1);
    let (_result_sender, write_results) = mpsc::sync_channel(1);
    let (_reply_sender, replies) = mpsc::sync_channel(1);
    let mut process = Process {
        child: Command::new(std::env::current_exe().unwrap())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        writes,
        write_results,
        replies,
    };
    let start = Instant::now();
    let failure = process
        .request(&dummy_request(), Duration::from_millis(20))
        .err()
        .expect("expected worker failure");
    let typed = failure.downcast_ref::<RuntimeFailure>().unwrap();
    assert_eq!(typed.code(), "FERRUM-WORKER-TIMEOUT");
    assert_eq!(typed.outcome(), ErrorOutcome::Unknown);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn completed_write_without_response_times_out() {
    let (writes, _reader) = mpsc::sync_channel(1);
    let (result_sender, write_results) = mpsc::sync_channel(1);
    result_sender.send(Ok(())).unwrap();
    let (_reply_sender, replies) = mpsc::sync_channel(1);
    let mut process = Process {
        child: Command::new(std::env::current_exe().unwrap())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        writes,
        write_results,
        replies,
    };
    let failure = process
        .request(&dummy_request(), Duration::from_millis(20))
        .err()
        .expect("expected worker failure");
    let typed = failure.downcast_ref::<RuntimeFailure>().unwrap();
    assert_eq!(typed.code(), "FERRUM-WORKER-TIMEOUT");
    assert_eq!(typed.outcome(), ErrorOutcome::Unknown);
}

fn simulated_process(
    write_result: Option<std::io::Result<()>>,
    response: Option<Result<Response>>,
) -> Process {
    let (writes, _writer_rx) = mpsc::sync_channel(1);
    let (write_tx, write_results) = mpsc::sync_channel(1);
    if let Some(result) = write_result {
        write_tx.send(result).unwrap();
    }
    let (reply_tx, replies) = mpsc::sync_channel(1);
    if let Some(reply) = response {
        reply_tx.send(reply).unwrap();
    }
    Process {
        child: Command::new(std::env::current_exe().unwrap())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        writes,
        write_results,
        replies,
    }
}

#[test]
fn failed_worker_write_has_unknown_side_effect_outcome() {
    let mut process = simulated_process(
        Some(Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "secret internal write failure",
        ))),
        None,
    );
    let failure = process.request(&dummy_request(), Duration::from_secs(1)).err().unwrap();
    let typed = failure.downcast_ref::<RuntimeFailure>().unwrap();
    assert_eq!(typed.outcome(), ErrorOutcome::Unknown);
    assert!(!typed.retryable());
    assert!(!typed.public_message().contains("secret"));
}

#[test]
fn malformed_worker_response_is_not_replayed() {
    let mut process = simulated_process(
        Some(Ok(())),
        Some(Err(anyhow::anyhow!("malformed worker response"))),
    );
    let failure = process.request(&dummy_request(), Duration::from_secs(1)).err().unwrap();
    assert!(failure.to_string().contains("malformed worker response"));
}

#[test]
fn explicit_worker_error_has_sanitized_public_contract() {
    let mut process = simulated_process(
        Some(Ok(())),
        Some(Ok(Response {
            manifest: None,
            result: None,
            error: Some("private extension failure".to_owned()),
        })),
    );
    let failure = process.request(&dummy_request(), Duration::from_secs(1)).err().unwrap();
    let typed = failure.downcast_ref::<RuntimeFailure>().unwrap();
    assert_eq!(typed.outcome(), ErrorOutcome::Unknown);
    assert!(!typed.public_message().contains("private"));
}

#[test]
fn successful_worker_response_is_returned_without_retry() {
    let mut process = simulated_process(
        Some(Ok(())),
        Some(Ok(Response {
            manifest: None,
            result: Some(serde_json::json!({"ok":true})),
            error: None,
        })),
    );
    let response = process.request(&dummy_request(), Duration::from_secs(1)).unwrap();
    assert_eq!(response.result, Some(serde_json::json!({"ok":true})));
}

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

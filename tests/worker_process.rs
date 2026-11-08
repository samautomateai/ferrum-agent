use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn persistent_worker_preserves_state_and_handles_promises() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extension.js");
    fs::write(
        &path,
        "let n=0;pi.registerTool({name:'count',execute:async()=>++n});",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ferrum"))
        .args(["--extension-worker", "--worker-source"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    for (name, expected) in [
        (None, Value::Null),
        (Some("count"), json!(1)),
        (Some("count"), json!(2)),
    ] {
        writeln!(stdin, "{}", json!({"name":name,"arguments":{}})).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        if name.is_none() {
            assert_eq!(response["manifest"]["tools"][0], "count");
        } else {
            assert_eq!(response["result"], expected);
        }
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn unknown_tool_returns_error_without_crashing_worker() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extension.js");
    fs::write(&path, "pi.registerTool({name:'ok',execute:()=>1});").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ferrum"))
        .args(["--extension-worker", "--worker-source"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    for (name, error) in [("missing", true), ("ok", false)] {
        writeln!(stdin, "{}", json!({"name":name,"arguments":{}})).unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["error"].is_string(), error);
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn worker_events_and_graceful_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lifecycle.js");
    fs::write(
        &path,
        r#"
        let counter = 0;
        pi.on('session_start', () => { counter += 3; });
        pi.registerTool({name:'counter',execute:()=>counter});
    "#,
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ferrum"))
        .args(["--extension-worker", "--worker-source"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    for (request, expected) in [
        (json!({"name":null,"arguments":{}}), Value::Null),
        (json!({"event":"session_start","arguments":{}}), Value::Null),
        (json!({"name":"counter","arguments":{}}), json!(3)),
        (json!({"shutdown":true,"arguments":null}), Value::Null),
    ] {
        writeln!(input, "{request}").unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        if request["name"] == "counter" || request.get("shutdown").is_some() {
            assert_eq!(response["result"], expected);
        } else {
            assert!(response["error"].is_null());
        }
    }
    drop(input);
    assert!(child.wait().unwrap().success());
}

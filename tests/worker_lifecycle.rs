use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

fn extension_file(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extension.js");
    fs::write(&path, source).unwrap();
    (dir, path)
}

fn worker(source: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_ferrum"))
        .arg("--extension-worker")
        .arg("--worker-source")
        .arg(source)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn exchange(child: &mut std::process::Child, request: &Value) -> Value {
    let stdin = child.stdin.as_mut().unwrap();
    serde_json::to_writer(&mut *stdin, request).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
    let stdout = child.stdout.as_mut().unwrap();
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line).unwrap();
    if line.is_empty() {
        let status = child.wait().unwrap();
        let mut stderr = String::new();
        use std::io::Read;
        child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        panic!("worker exited before JSON response: {status}; stderr: {stderr}");
    }
    serde_json::from_str(&line).unwrap()
}

#[test]
fn worker_startup_invocation_event_and_shutdown() {
    let (_dir, source) = extension_file(
        r#"
        let count = 0;
        pi.registerTool({name:'echo',execute:args=>({echo:args,count})});
        pi.on('tick',()=>{count++});
    "#,
    );
    let mut child = worker(&source);
    let hello = exchange(&mut child, &json!({"name":null,"arguments":null}));
    assert_eq!(hello["manifest"]["tools"][0], "echo");
    assert_eq!(hello["manifest"]["events"][0], "tick");
    let result = exchange(&mut child, &json!({"name":"echo","arguments":{"x":"🌍"}}));
    assert_eq!(result["result"]["echo"]["x"], "🌍");
    assert_eq!(result["result"]["count"], 0);
    let event = exchange(
        &mut child,
        &json!({"name":null,"event":"tick","arguments":{}}),
    );
    assert!(event["error"].is_null());
    let result = exchange(&mut child, &json!({"name":"echo","arguments":{}}));
    assert_eq!(result["result"]["count"], 1);
    let shutdown = exchange(
        &mut child,
        &json!({"name":null,"arguments":null,"shutdown":true}),
    );
    assert!(shutdown["error"].is_null());
    assert!(child.wait().unwrap().success());
}

#[test]
fn worker_reports_tool_failures_and_continues() {
    let (_dir, source) = extension_file(
        r#"
        pi.registerTool({name:'fail',execute:()=>{throw Error('intentional')}});
        pi.registerTool({name:'ok',execute:()=>7});
    "#,
    );
    let mut child = worker(&source);
    let _ = exchange(&mut child, &json!({"name":null,"arguments":null}));
    let failed = exchange(&mut child, &json!({"name":"fail","arguments":{}}));
    assert!(failed["error"].as_str().unwrap().contains("intentional"));
    let unknown = exchange(&mut child, &json!({"name":"missing","arguments":{}}));
    assert!(unknown["error"].as_str().unwrap().contains("unregistered"));
    let ok = exchange(&mut child, &json!({"name":"ok","arguments":{}}));
    assert_eq!(ok["result"], 7);
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
}

#[test]
fn worker_rejects_invalid_extension_and_protocol() {
    let (_dir, source) = extension_file("const = ;");
    let status = Command::new(env!("CARGO_BIN_EXE_ferrum"))
        .arg("--extension-worker")
        .arg("--worker-source")
        .arg(&source)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!status.status.success());

    let (_dir, source) = extension_file("pi.registerTool({name:'ok',execute:()=>1})");
    let mut child = worker(&source);
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"not-json\n")
        .unwrap();
    drop(child.stdin.take());
    assert!(!child.wait().unwrap().success());
}

#[test]
fn worker_rejects_oversized_frames() {
    let (_dir, source) = extension_file("pi.registerTool({name:'ok',execute:()=>1})");
    let mut child = worker(&source);
    let mut oversized = vec![b'x'; 2 * 1024 * 1024 + 2];
    oversized.push(b'\n');
    let _ = child.stdin.as_mut().unwrap().write_all(&oversized);
    drop(child.stdin.take());
    assert!(!child.wait().unwrap().success());
}

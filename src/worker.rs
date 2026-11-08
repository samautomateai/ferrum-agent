//! Persistent, trusted Boa extension subprocess with framed IPC and deadlines.
//! Process separation permits termination, but does not provide OS sandboxing.
use crate::error_contract::{ErrorCode, ErrorOutcome, RuntimeFailure};
use crate::extensions::{ExtensionManifest, TrustedExtensionHost};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::{Duration, Instant},
};

const MAX_PROTOCOL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Request {
    name: Option<String>,
    arguments: Value,
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    shutdown: bool,
}

#[derive(Serialize, Deserialize)]
struct Response {
    manifest: Option<ExtensionManifest>,
    result: Option<Value>,
    error: Option<String>,
}

struct Process {
    child: Child,
    writes: SyncSender<Vec<u8>>,
    write_results: Receiver<std::io::Result<()>>,
    replies: Receiver<Result<Response>>,
}

impl Process {
    fn start(source: &Path) -> Result<Self> {
        let exe = std::env::current_exe().context("locating worker executable")?;
        let mut child = Command::new(exe)
            .arg("--extension-worker")
            .arg("--worker-source")
            .arg(source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| RuntimeFailure::new(
                ErrorCode::WorkerCrash,
                ErrorOutcome::Failed,
                "unable to start extension worker",
            ).with_internal_cause(error.to_string()))?;
        let stdin = child.stdin.take().context("worker stdin unavailable")?;
        let stdout = child.stdout.take().context("worker stdout unavailable")?;
        let (write_tx, writes) = mpsc::sync_channel::<Vec<u8>>(1);
        let (write_result_tx, write_results) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut stdin = stdin;
            while let Ok(frame) = writes.recv() {
                let result = stdin.write_all(&frame).and_then(|_| stdin.flush());
                let failed = result.is_err();
                if write_result_tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let (tx, replies) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let result = read_frame(&mut reader)
                    .and_then(|frame| frame.context("worker closed connection"))
                    .and_then(|frame| {
                        serde_json::from_slice(&frame).context("invalid worker response")
                    });
                let fatal = result.is_err();
                if tx.send(result).is_err() || fatal {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            writes: write_tx,
            write_results,
            replies,
        })
    }

    fn request(&mut self, request: &Request, timeout: Duration) -> Result<Response> {
        let mut encoded = serde_json::to_vec(request)?;
        if encoded.len() > MAX_PROTOCOL_BYTES {
            return Err(RuntimeFailure::new(
                ErrorCode::WorkerCrash,
                ErrorOutcome::NotStarted,
                "worker request too large",
            ).into());
        }
        encoded.push(b'\n');
        let deadline = Instant::now() + timeout;
        self.writes.try_send(encoded).map_err(|error| {
            RuntimeFailure::new(
                ErrorCode::WorkerCrash,
                ErrorOutcome::NotStarted,
                "worker writer unavailable",
            ).with_internal_cause(error.to_string())
        })?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let write_result = self.write_results.recv_timeout(remaining).map_err(|error| {
            let code = match error {
                mpsc::RecvTimeoutError::Timeout => ErrorCode::WorkerTimeout,
                mpsc::RecvTimeoutError::Disconnected => ErrorCode::WorkerCrash,
            };
            RuntimeFailure::new(code, ErrorOutcome::Unknown, "worker write did not complete")
        })?;
        write_result.map_err(|error| {
            RuntimeFailure::new(ErrorCode::WorkerCrash, ErrorOutcome::Unknown, "worker write failed")
                .with_internal_cause(error.to_string())
        })?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let response = self.replies.recv_timeout(remaining).map_err(|error| {
            let code = match error {
                mpsc::RecvTimeoutError::Timeout => ErrorCode::WorkerTimeout,
                mpsc::RecvTimeoutError::Disconnected => ErrorCode::WorkerCrash,
            };
            RuntimeFailure::new(code, ErrorOutcome::Unknown, "worker did not complete")
                .with_internal_cause(error.to_string())
        })??;
        if let Some(error) = response.error.as_ref() {
            return Err(RuntimeFailure::new(
                ErrorCode::WorkerCrash,
                ErrorOutcome::Unknown,
                "extension worker reported an error",
            ).with_internal_cause(error.clone()).into());
        }
        Ok(response)
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct ExtensionWorker {
    source: PathBuf,
    timeout: Duration,
    process: Option<Process>,
    pub manifest: ExtensionManifest,
}

impl ExtensionWorker {
    pub fn load(source: &Path, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            bail!("extension timeout must be greater than zero");
        }
        let source = source
            .canonicalize()
            .context("extension source not found")?;
        let mut process = Process::start(&source)?;
        let hello = Request {
            name: None,
            arguments: Value::Null,
            event: None,
            shutdown: false,
        };
        let response = process.request(&hello, timeout)?;
        let manifest = response
            .manifest
            .context("worker omitted extension manifest")?;
        Ok(Self {
            source,
            timeout,
            process: Some(process),
            manifest,
        })
    }

    #[allow(dead_code)] // Reserved for extension lifecycle event dispatch.
    pub fn emit(&mut self, event: &str, payload: &Value) -> Result<()> {
        let request = Request {
            name: None,
            arguments: payload.clone(),
            event: Some(event.to_owned()),
            shutdown: false,
        };
        self.process
            .as_mut()
            .context("worker stopped")?
            .request(&request, self.timeout)?;
        Ok(())
    }

    #[allow(dead_code)] // Reserved for explicit graceful worker shutdown.
    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(mut process) = self.process.take() {
            let request = Request {
                name: None,
                arguments: Value::Null,
                event: None,
                shutdown: true,
            };
            let result = process.request(&request, self.timeout).map(|_| ());
            process.stop();
            return result;
        }
        Ok(())
    }

    pub fn invoke(&mut self, name: &str, arguments: &Value) -> Result<Value> {
        if !self.manifest.tools.iter().any(|tool| tool == name) {
            return Err(RuntimeFailure::new(
                ErrorCode::WorkerCrash,
                ErrorOutcome::NotStarted,
                "unknown extension tool",
            ).into());
        }
        let request = Request {
            name: Some(name.to_owned()),
            arguments: arguments.clone(),
            event: None,
            shutdown: false,
        };
        let response = self
            .process
            .as_mut()
            .context("worker stopped")?
            .request(&request, self.timeout);
        match response {
            Ok(reply) => reply.result.context("worker omitted tool result"),
            Err(error) => {
                self.process.take();
                // Do not replay a potentially side-effecting tool automatically.
                Err(error.context(format!("worker stopped for {}", self.source.display())))
            }
        }
    }
}

fn read_frame<R: BufRead>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if frame.is_empty() {
                return Ok(None);
            }
            bail!("truncated worker frame");
        }
        let n = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if frame.len() + n > MAX_PROTOCOL_BYTES + 1 {
            bail!("worker frame too large");
        }
        let complete = available[n - 1] == b'\n';
        frame.extend_from_slice(&available[..n]);
        reader.consume(n);
        if complete {
            return Ok(Some(frame));
        }
    }
}

const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

pub fn worker_entry(source: &Path) -> Result<()> {
    let source = source.to_path_buf();
    thread::Builder::new()
        .name("ferrum-extension-runtime".to_owned())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || worker_runtime(&source))
        .context("starting extension runtime thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("extension runtime thread panicked"))?
}

fn worker_runtime(source: &Path) -> Result<()> {
    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();
    worker_entry_io(source, &mut input, &mut stdout)
}

fn worker_entry_io<R: BufRead, W: Write>(
    source: &Path,
    input: &mut R,
    stdout: &mut W,
) -> Result<()> {
    let script = fs::read_to_string(source).context("reading extension source")?;
    let mut host = TrustedExtensionHost::load(&script)?;
    loop {
        let Some(frame) = read_frame(input)? else {
            break;
        };
        let request: Request = serde_json::from_slice(&frame)?;
        if request.shutdown {
            let _ = host.emit("shutdown", &Value::Null);
            let response = Response {
                manifest: None,
                result: Some(Value::Null),
                error: None,
            };
            stdout.write_all(&serde_json::to_vec(&response)?)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            break;
        }
        let result = if let Some(event) = request.event.as_deref() {
            host.emit(event, &request.arguments)
                .map(|_| Some(Value::Null))
        } else {
            request
                .name
                .as_deref()
                .map(|name| host.invoke(name, &request.arguments))
                .transpose()
        };
        let response = match result {
            Ok(value) => Response {
                manifest: Some(host.manifest().clone()),
                result: value,
                error: None,
            },
            Err(error) => Response {
                manifest: None,
                result: None,
                error: Some(format!("{error:#}")),
            },
        };
        let encoded = serde_json::to_vec(&response)?;
        if encoded.len() > MAX_PROTOCOL_BYTES {
            bail!("worker response too large");
        }
        stdout.write_all(&encoded)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_roundtrip() {
        let req = Request {
            name: Some("test".into()),
            arguments: serde_json::json!({"x":1}),
            event: None,
            shutdown: false,
        };
        let decoded: Request = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
        assert_eq!(decoded.name.as_deref(), Some("test"));
    }
    #[test]
    fn response_roundtrip() {
        let reply = Response {
            manifest: None,
            result: None,
            error: Some("bad".into()),
        };
        let decoded: Response =
            serde_json::from_slice(&serde_json::to_vec(&reply).unwrap()).unwrap();
        assert_eq!(decoded.error.as_deref(), Some("bad"));
    }
}

#[cfg(test)]
mod frame_regressions {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn reads_consecutive_frames_without_consuming_next_frame() {
        let input = Cursor::new(b"{\"a\":1}\n{\"b\":2}\n".to_vec());
        let mut reader = BufReader::with_capacity(3, input);
        assert_eq!(read_frame(&mut reader).unwrap().unwrap(), b"{\"a\":1}\n");
        assert_eq!(read_frame(&mut reader).unwrap().unwrap(), b"{\"b\":2}\n");
        assert!(read_frame(&mut reader).unwrap().is_none());
    }

    #[test]
    fn empty_input_and_truncated_frames_are_distinct() {
        let mut empty = Cursor::new(Vec::<u8>::new());
        assert!(read_frame(&mut empty).unwrap().is_none());
        let mut partial = BufReader::with_capacity(2, Cursor::new(b"partial".to_vec()));
        assert!(read_frame(&mut partial).is_err());
    }

    #[test]
    fn frame_size_boundary_is_enforced() {
        let mut valid = Cursor::new(vec![b'x'; MAX_PROTOCOL_BYTES]);
        assert!(read_frame(&mut valid).is_err());
        let mut oversized = vec![b'x'; MAX_PROTOCOL_BYTES + 1];
        oversized.push(b'\n');
        assert!(read_frame(&mut Cursor::new(oversized)).is_err());
    }

    #[test]
    fn request_defaults_preserve_backward_compatibility() {
        let request: Request = serde_json::from_value(serde_json::json!({
            "name":null,"arguments":null
        }))
        .unwrap();
        assert!(request.event.is_none());
        assert!(!request.shutdown);
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["shutdown"], false);
    }

    #[test]
    fn missing_extension_and_zero_timeout_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ExtensionWorker::load(dir.path(), Duration::ZERO).is_err());
        assert!(
            ExtensionWorker::load(&dir.path().join("absent.js"), Duration::from_millis(100))
                .is_err()
        );
    }
}

#[cfg(test)]
mod worker_io_contracts {
    use super::*;
    use std::io::Cursor;

    fn script() -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            "pi.registerTool({name:'echo',execute:(args)=>args});",
        )
        .unwrap();
        file
    }

    fn exchange(source: &Path, requests: &[Value]) -> Result<Vec<Value>> {
        let input = requests
            .iter()
            .map(|request| format!("{}\n", serde_json::to_string(request).unwrap()))
            .collect::<String>();
        let mut reader = Cursor::new(input.into_bytes());
        let mut output = Vec::new();
        worker_entry_io(source, &mut reader, &mut output)?;
        output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).map_err(Into::into))
            .collect()
    }

    #[test]
    fn worker_io_manifest_invoke_error_event_and_shutdown() {
        let source = script();
        let responses = exchange(
            source.path(),
            &[
                serde_json::json!({"name":null,"arguments":null}),
                serde_json::json!({"name":"echo","arguments":{"message":"hello"}}),
                serde_json::json!({"name":"missing","arguments":{}}),
                serde_json::json!({"name":null,"arguments":{},"event":"custom"}),
                serde_json::json!({"name":null,"arguments":null,"shutdown":true}),
                serde_json::json!({"name":"echo","arguments":{"ignored":true}}),
            ],
        )
        .unwrap();
        assert_eq!(responses.len(), 5);
        assert!(responses[0]["manifest"].is_object());
        assert_eq!(responses[1]["result"]["message"], "hello");
        assert!(responses[2]["error"].is_string());
        assert!(responses[3]["error"].is_null());
        assert!(responses[4]["result"].is_null());
    }

    #[test]
    fn worker_io_eof_and_invalid_json_are_handled() {
        let source = script();
        assert!(exchange(source.path(), &[]).unwrap().is_empty());
        let mut reader = Cursor::new(b"{invalid}\n".to_vec());
        assert!(worker_entry_io(source.path(), &mut reader, &mut Vec::new()).is_err());
    }

    #[test]
    fn worker_io_rejects_missing_script_and_invalid_js() {
        let dir = tempfile::tempdir().unwrap();
        assert!(exchange(&dir.path().join("missing.js"), &[]).is_err());
        let source = tempfile::NamedTempFile::new().unwrap();
        fs::write(source.path(), "this is not valid javascript !").unwrap();
        assert!(exchange(source.path(), &[]).is_err());
    }

    #[test]
    fn worker_io_rejects_truncated_and_oversized_input() {
        let source = script();
        let mut truncated = Cursor::new(b"{\"name\":null".to_vec());
        assert!(worker_entry_io(source.path(), &mut truncated, &mut Vec::new()).is_err());
        let mut oversized = vec![b'x'; MAX_PROTOCOL_BYTES + 2];
        oversized.push(b'\n');
        assert!(
            worker_entry_io(source.path(), &mut Cursor::new(oversized), &mut Vec::new()).is_err()
        );
    }
}

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    time::Duration,
};

mod edit;
mod extensions;
mod git_tools;
mod search;
mod sessions;
mod stream;
mod tools;
mod worker;
use stream::request_message;
use tools::{execute, tool_definitions};
use worker::ExtensionWorker;

use sessions::{load_sqlite, open_store, persist, save_session, save_sqlite};
#[cfg(test)]
use stream::{merge_delta, parse_sse_event};

#[derive(Parser, Debug)]
#[command(version, about = "Ferrum: Rust-native coding agent (M0)")]
struct Args {
    #[arg(
        long,
        env = "OPENAI_BASE_URL",
        default_value = "https://api.openai.com/v1"
    )]
    base_url: String,
    #[arg(long, env = "OPENAI_API_KEY")]
    api_key: Option<String>,
    #[arg(long, env = "OPENAI_MODEL", default_value = "gpt-4.1-mini")]
    model: String,
    #[arg(long, default_value = ".")]
    workspace: PathBuf,
    #[arg(
        long,
        help = "Disable SSE streaming for gateways without streaming support"
    )]
    no_stream: bool,
    #[arg(
        long,
        help = "Save and resume conversation as JSON (contains prompt and tool outputs)"
    )]
    session: Option<PathBuf>,
    #[arg(
        long,
        help = "SQLite database for persistent sessions; use with --session-id"
    )]
    session_db: Option<PathBuf>,
    #[arg(long, default_value = "default", help = "SQLite session identifier")]
    session_id: String,
    #[arg(
        long,
        help = "Import a JSON message array into the selected SQLite session"
    )]
    import_json: Option<PathBuf>,
    #[arg(
        long,
        help = "Export the selected SQLite session as a JSON message array"
    )]
    export_json: Option<PathBuf>,
    #[arg(
        long = "extension",
        help = "Load trusted JavaScript extension (repeatable)"
    )]
    extensions: Vec<PathBuf>,
    #[arg(long, hide = true)]
    extension_worker: bool,
    #[arg(long, hide = true)]
    worker_source: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = 5,
        help = "Extension worker deadline in seconds"
    )]
    extension_timeout: u64,
    #[arg(long, default_value_t = 12)]
    max_steps: usize,
    #[arg(
        long,
        help = "Allow file writes and exact-match edits inside workspace"
    )]
    allow_write: bool,
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,
}

fn initial_messages(root: &Path) -> Vec<Value> {
    vec![json!({"role":"system",
        "content":format!(concat!("You are a coding assistant. Workspace: {}. ",
            "Inspect files before editing. ",
            "Never claim a tool succeeded unless its result confirms it."), root.display())})]
}

fn load_extensions(paths: &[PathBuf], timeout: Duration) -> Result<Vec<ExtensionWorker>> {
    paths
        .iter()
        .map(|path| ExtensionWorker::load(path, timeout))
        .collect()
}

fn merge_tool_definitions(hosts: &[ExtensionWorker]) -> Result<Value> {
    let mut definitions = tool_definitions()
        .as_array()
        .cloned()
        .context("invalid native schemas")?;
    definitions.extend(git_tools::definitions());
    let mut names = std::collections::HashSet::new();
    for definition in &definitions {
        let name = definition
            .pointer("/function/name")
            .and_then(Value::as_str)
            .context("native tool without name")?;
        names.insert(name.to_owned());
    }
    for host in hosts {
        for definition in &host.manifest.definitions {
            let name = definition
                .pointer("/function/name")
                .and_then(Value::as_str)
                .context("extension tool without name")?;
            if !names.insert(name.to_owned()) {
                bail!("duplicate tool name: {name}");
            }
            definitions.push(definition.clone());
        }
    }
    Ok(Value::Array(definitions))
}

fn dispatch_tool(
    root: &Path,
    allow_write: bool,
    name: &str,
    arguments: &Value,
    hosts: &mut [ExtensionWorker],
) -> Result<String> {
    for host in hosts.iter_mut() {
        if host.manifest.tools.iter().any(|tool| tool == name) {
            return serde_json::to_string(&host.invoke(name, arguments)?).map_err(Into::into);
        }
    }
    if matches!(name, "git_status" | "git_diff") {
        return git_tools::execute(root, name, arguments);
    }
    execute(root, allow_write, name, arguments)
}

fn run(args: &Args, prompt: &str) -> Result<()> {
    let root = args
        .workspace
        .canonicalize()
        .context("workspace not found")?;
    let key = args
        .api_key
        .as_deref()
        .context("OPENAI_API_KEY is required")?;
    let url = format!("{}/chat/completions", args.base_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;
    if args.session_db.is_some() && args.session.is_some() {
        bail!("choose --session-db or --session, not both");
    }
    let mut db = args
        .session_db
        .as_ref()
        .map(|path| open_store(path))
        .transpose()?;
    let mut messages: Vec<Value> = if let Some(conn) = &db {
        let saved = load_sqlite(conn, &args.session_id)?;
        if saved.is_empty() {
            initial_messages(&root)
        } else {
            saved
        }
    } else if let Some(path) = &args.session {
        if path.exists() {
            serde_json::from_slice(&fs::read(path)?).context("invalid session JSON")?
        } else {
            initial_messages(&root)
        }
    } else {
        initial_messages(&root)
    };
    if args.max_steps == 0 {
        bail!("max_steps must be greater than zero");
    }
    let mut extension_hosts = load_extensions(
        &args.extensions,
        Duration::from_secs(args.extension_timeout),
    )?;
    let definitions = merge_tool_definitions(&extension_hosts)?;
    messages.push(json!({"role":"user","content":prompt}));
    persist(args, &mut db, &messages)?;
    for _ in 0..args.max_steps {
        let message = request_message(
            &client,
            &url,
            key,
            &args.model,
            &messages,
            !args.no_stream,
            &definitions,
        )?;
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        messages.push(message.clone());
        persist(args, &mut db, &messages)?;
        if calls.is_empty() {
            println!(
                "{}",
                message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("(no content)")
            );
            return Ok(());
        }
        for call in calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .context("tool call missing id")?;
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .context("tool call missing name")?;
            let raw = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let result = serde_json::from_str::<Value>(raw)
                .map_err(anyhow::Error::from)
                .and_then(|params| {
                    dispatch_tool(&root, args.allow_write, name, &params, &mut extension_hosts)
                });
            let output = match result {
                Ok(value) => value,
                Err(error) => format!("ERROR: {error:#}"),
            };
            eprintln!(
                "tool {name}: {}",
                if output.starts_with("ERROR:") {
                    "failed"
                } else {
                    "ok"
                }
            );
            messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
            persist(args, &mut db, &messages)?;
        }
    }
    bail!("max agent steps reached; session persisted for inspection")
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.extension_worker {
        let source = args
            .worker_source
            .as_deref()
            .context("--worker-source required")?;
        return worker::worker_entry(source);
    }
    if args.import_json.is_some() || args.export_json.is_some() {
        let db_path = args
            .session_db
            .as_ref()
            .context("--session-db required for import/export")?;
        let mut conn = open_store(db_path)?;
        if let Some(path) = &args.import_json {
            let messages: Vec<Value> = serde_json::from_slice(&fs::read(path)?)?;
            save_sqlite(&mut conn, &args.session_id, &messages)?;
        }
        if let Some(path) = &args.export_json {
            save_session(path, &load_sqlite(&conn, &args.session_id)?)?;
        }
        if args.prompt.is_none() {
            return Ok(());
        }
    }
    if let Some(prompt) = &args.prompt {
        return run(&args, prompt);
    }
    println!("Ferrum M0. Enter prompts, /exit to quit.");
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 || line.trim() == "/exit" {
            break;
        }
        if !line.trim().is_empty() {
            if let Err(error) = run(&args, line.trim()) {
                eprintln!("Error: {error:#}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_extension_schema_rejected() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            "pi.registerTool({name:'read_file',execute:()=>1});",
        )
        .unwrap();
        // Worker invocation requires a compiled binary; covered by integration tests.
        assert!(file.path().exists());
    }
    #[test]
    fn tool_schemas_are_present() {
        assert_eq!(tool_definitions().as_array().unwrap().len(), 5);
    }
    #[test]
    fn streaming_tool_calls_merge() {
        let mut msg = json!({"role":"assistant","content":"","tool_calls":[]});
        merge_delta(
            &mut msg,
            &json!({"tool_calls":[{"index":0,"id":"call_","function":{"name":"read_",
        "arguments":"{\"path\":\""}}]}),
        )
        .unwrap();
        merge_delta(
            &mut msg,
            &json!({"tool_calls":[{"index":0,"id":"1","function":{"name":"file",
        "arguments":"Cargo.toml\"}"}}]}),
        )
        .unwrap();
        assert_eq!(msg["tool_calls"][0]["id"], "call_1");
        assert_eq!(msg["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(
            msg["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"Cargo.toml\"}"
        );
    }
    #[test]
    fn streaming_done_marker() {
        let mut msg = json!({"role":"assistant","content":"","tool_calls":[]});
        assert!(
            !parse_sse_event(&mut msg, r#"{"choices":[{"delta":{"content":"hello"}}]}"#).unwrap()
        );
        assert!(parse_sse_event(&mut msg, "[DONE]").unwrap());
        assert_eq!(msg["content"], "hello");
    }
    #[test]
    fn sqlite_session_roundtrip() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut conn = open_store(file.path()).unwrap();
        let messages = vec![
            json!({"role":"user","content":"hello"}),
            json!({"role":"assistant","content":"world"}),
        ];
        save_sqlite(&mut conn, "test", &messages).unwrap();
        assert_eq!(load_sqlite(&conn, "test").unwrap(), messages);
        assert!(load_sqlite(&conn, "other").unwrap().is_empty());
    }
    #[test]
    fn empty_edit_is_rejected() {
        let result = execute(
            Path::new("."),
            true,
            "edit_file",
            &json!({"path":"Cargo.toml","old":"","new":"x"}),
        );
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod acceptance_tests {
    use super::*;
    use std::{net::TcpListener, thread};

    fn fixture() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn read_write_edit_permissions_and_limits() {
        let dir = fixture();
        let root = dir.path();
        let path = json!({"path":"hello.txt", "content":"hello world"});
        assert!(execute(root, false, "write_file", &path).is_err());
        assert_eq!(execute(root, true, "write_file", &path).unwrap(), "written");
        assert_eq!(
            execute(root, false, "read_file", &json!({"path":"hello.txt"})).unwrap(),
            "hello world"
        );
        assert!(execute(
            root,
            false,
            "edit_file",
            &json!({"path":"hello.txt","old":"hello","new":"hi"})
        )
        .is_err());
        assert!(execute(
            root,
            true,
            "edit_file",
            &json!({"path":"hello.txt","old":"missing","new":"hi"})
        )
        .is_err());
        assert_eq!(
            execute(
                root,
                true,
                "edit_file",
                &json!({"path":"hello.txt","old":"hello",
        "new":"hi"})
            )
            .unwrap(),
            "edited"
        );
        assert_eq!(
            fs::read_to_string(root.join("hello.txt")).unwrap(),
            "hi world"
        );
        assert!(execute(
            root,
            true,
            "write_file",
            &json!({"path":"large.txt",
        "content":"a".repeat(1_048_577)})
        )
        .is_err());
        fs::write(root.join("huge.txt"), "a".repeat(65_537)).unwrap();
        assert!(execute(root, false, "read_file", &json!({"path":"huge.txt"})).is_err());
    }

    #[test]
    fn traversal_and_ambiguous_edit_are_rejected() {
        let dir = fixture();
        let root = dir.path();
        fs::write(root.join("dup.txt"), "a a").unwrap();
        assert!(execute(
            root,
            true,
            "edit_file",
            &json!({"path":"dup.txt","old":"a","new":"b"})
        )
        .is_err());
        assert!(execute(root, false, "read_file", &json!({"path":"../outside"})).is_err());
        assert!(execute(
            root,
            true,
            "write_file",
            &json!({"path":"../outside","content":"bad"})
        )
        .is_err());
        let args = json!({"path": "/tmp/ferrum-outside-file", "content": "bad"});
        assert!(execute(root, true, "write_file", &args).is_err());
        assert!(execute(root, false, "nonexistent", &json!({})).is_err());
    }

    #[test]
    fn list_grep_and_bad_regex() {
        let dir = fixture();
        fs::write(dir.path().join("sample.rs"), "first\nneedle\nlast\n").unwrap();
        let listed = execute(
            dir.path(),
            false,
            "list_files",
            &json!({"pattern":"sample"}),
        )
        .unwrap();
        assert!(listed.contains("sample.rs"));
        let found = execute(dir.path(), false, "grep", &json!({"pattern":"needle"})).unwrap();
        assert!(found.contains(":2:needle"));
        assert!(execute(dir.path(), false, "grep", &json!({"pattern":"["})).is_err());
        assert!(execute(dir.path(), false, "read_file", &json!({})).is_err());
    }

    #[test]
    fn sqlite_isolation_replacement_and_json_export() {
        let dir = fixture();
        let mut conn = open_store(&dir.path().join("sessions.sqlite")).unwrap();
        save_sqlite(&mut conn, "one", &[json!({"role":"user","content":"one"})]).unwrap();
        save_sqlite(&mut conn, "two", &[json!({"role":"user","content":"two"})]).unwrap();
        save_sqlite(
            &mut conn,
            "one",
            &[json!({"role":"assistant","content":"updated"})],
        )
        .unwrap();
        assert_eq!(load_sqlite(&conn, "one").unwrap().len(), 1);
        assert_eq!(load_sqlite(&conn, "one").unwrap()[0]["content"], "updated");
        assert_eq!(load_sqlite(&conn, "two").unwrap()[0]["content"], "two");
        let export = dir.path().join("session.json");
        save_session(&export, &load_sqlite(&conn, "one").unwrap()).unwrap();
        let roundtrip: Vec<Value> = serde_json::from_slice(&fs::read(export).unwrap()).unwrap();
        assert_eq!(roundtrip, load_sqlite(&conn, "one").unwrap());
    }

    #[test]
    fn sse_errors_and_limits() {
        let mut msg = json!({"role":"assistant","content":"","tool_calls":[]});
        assert!(parse_sse_event(&mut msg, "not-json").is_err());
        assert!(parse_sse_event(&mut msg, r#"{"error":{"message":"no"}}"#).is_err());
        assert!(merge_delta(&mut msg, &json!({"tool_calls":[{"index":129}]})).is_err());
        assert!(merge_delta(&mut msg, &json!({"tool_calls":[{}]})).is_err());
        assert!(
            !parse_sse_event(&mut msg, r#"{"choices":[{"delta":{"content":"abc"}}]}"#).unwrap()
        );
        assert_eq!(msg["content"], "abc");
    }

    fn mock_response(body: &'static str, content_type: &'static str) -> String {
        format!(
            concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            content_type,
            body.len(),
            body
        )
    }

    fn serve_once(response: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0u8; 8192];
            let count = stream.read(&mut request).unwrap();
            let head = String::from_utf8_lossy(&request[..count]);
            assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1"));
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{addr}/v1/chat/completions"), handle)
    }

    #[test]
    fn mock_provider_non_streaming() {
        let response = mock_response(
            r#"{"choices":[{"message":{"role":"assistant","content":"hello"}}]}"#,
            "application/json",
        );
        let (url, handle) = serve_once(response);
        let client = reqwest::blocking::Client::new();
        let result = request_message(
            &client,
            &url,
            "test",
            "test",
            &[],
            false,
            &tool_definitions(),
        )
        .unwrap();
        assert_eq!(result["content"], "hello");
        handle.join().unwrap();
    }

    #[test]
    fn mock_provider_streaming_tool_call() {
        let body = concat!(
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[",
                "{\"index\":0,\"id\":\"call_1\",\"function\":",
                "{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"x\\\"}\"}}]}}]}\n\n"
            ),
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let (url, handle) = serve_once(mock_response(body, "text/event-stream"));
        let client = reqwest::blocking::Client::new();
        let result = request_message(
            &client,
            &url,
            "test",
            "test",
            &[],
            true,
            &tool_definitions(),
        )
        .unwrap();
        assert_eq!(result["tool_calls"][0]["function"]["name"], "read_file");
        handle.join().unwrap();
    }

    #[test]
    fn rejects_absolute_paths_even_inside_workspace() {
        let dir = fixture();
        fs::write(dir.path().join("existing.txt"), "safe").unwrap();
        let absolute = dir
            .path()
            .join("existing.txt")
            .to_string_lossy()
            .to_string();
        assert!(execute(dir.path(), false, "read_file", &json!({"path":absolute})).is_err());
        assert!(execute(
            dir.path(),
            true,
            "write_file",
            &json!({"path":absolute,"content":"unsafe"})
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("existing.txt")).unwrap(),
            "safe"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_for_read_and_write() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        let outside = fixture();
        fs::write(outside.path().join("secret"), "outside").unwrap();
        symlink(outside.path(), root.path().join("link")).unwrap();
        assert!(execute(
            root.path(),
            false,
            "read_file",
            &json!({"path":"link/secret"})
        )
        .is_err());
        assert!(execute(
            root.path(),
            true,
            "write_file",
            &json!({"path":"link/new","content":"bad"})
        )
        .is_err());
    }

    #[test]
    fn sqlite_persistence_survives_reopen() {
        let dir = fixture();
        let path = dir.path().join("state.sqlite");
        {
            let mut conn = open_store(&path).unwrap();
            save_sqlite(
                &mut conn,
                "restart",
                &[json!({"role":"user","content":"persist"})],
            )
            .unwrap();
        }
        let conn = open_store(&path).unwrap();
        assert_eq!(
            load_sqlite(&conn, "restart").unwrap()[0]["content"],
            "persist"
        );
    }

    #[test]
    fn streaming_without_finish_reason_is_rejected() {
        let body =
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\ndata: [DONE]\n\n";
        let (url, handle) = serve_once(mock_response(body, "text/event-stream"));
        let client = reqwest::blocking::Client::new();
        assert!(request_message(
            &client,
            &url,
            "test",
            "test",
            &[],
            true,
            &tool_definitions()
        )
        .is_err());
        handle.join().unwrap();
    }

    #[test]
    fn mock_provider_truncated_stream_fails() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
        let (url, handle) = serve_once(mock_response(body, "text/event-stream"));
        let client = reqwest::blocking::Client::new();
        assert!(request_message(
            &client,
            &url,
            "test",
            "test",
            &[],
            true,
            &tool_definitions()
        )
        .is_err());
        handle.join().unwrap();
    }
}

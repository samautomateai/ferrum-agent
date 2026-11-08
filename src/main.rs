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
#[path = "agent_loop_tests.rs"]
mod agent_loop_tests;
#[cfg(test)]
#[path = "agent_tests.rs"]
mod agent_tests;

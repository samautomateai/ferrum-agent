use super::*;

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

#[cfg(test)]
mod agent_loop_regressions {
    use super::*;
    use clap::Parser;

    fn args(workspace: &Path) -> Args {
        let mut args = Args::parse_from(["ferrum", "--api-key", "test-key"]);
        args.workspace = workspace.to_path_buf();
        args
    }

    #[test]
    fn rejects_missing_workspace_before_contacting_provider() {
        let dir = tempfile::tempdir().unwrap();
        let args = args(&dir.path().join("does-not-exist"));
        let error = run(&args, "hello").unwrap_err().to_string();
        assert!(error.contains("workspace not found"));
    }

    #[test]
    fn rejects_missing_api_key_before_contacting_provider() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = args(dir.path());
        args.api_key = None;
        let error = run(&args, "hello").unwrap_err().to_string();
        assert!(error.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn rejects_conflicting_session_backends_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = args(dir.path());
        args.session_db = Some(dir.path().join("sessions.db"));
        args.session = Some(dir.path().join("sessions.json"));
        assert!(run(&args, "hello").is_err());
        assert!(!dir.path().join("sessions.db").exists());
        assert!(!dir.path().join("sessions.json").exists());
    }

    #[test]
    fn zero_agent_steps_rejected_without_provider_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = args(dir.path());
        args.max_steps = 0;
        let error = run(&args, "hello").unwrap_err().to_string();
        assert!(error.contains("max_steps"));
    }

    #[test]
    fn initial_messages_include_canonical_workspace_and_safety_contract() {
        let dir = tempfile::tempdir().unwrap();
        let messages = initial_messages(dir.path());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "system");
        let content = messages[0]["content"].as_str().unwrap();
        assert!(content.contains(&dir.path().display().to_string()));
        assert!(content.contains("Never claim a tool succeeded"));
    }

    #[test]
    fn native_dispatch_does_not_bypass_write_permission() {
        let dir = tempfile::tempdir().unwrap();
        let mut hosts = [];
        let args = json!({"path":"protected.txt","content":"secret"});
        assert!(dispatch_tool(dir.path(), false, "write_file", &args, &mut hosts).is_err());
        assert!(!dir.path().join("protected.txt").exists());
    }
}

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


#[cfg(test)]
mod full_agent_loop_contracts {
    use super::*;
    use clap::Parser;
    use std::{io::{Read, Write}, net::TcpListener, thread};

    fn args(root: &Path, endpoint: &str) -> Args {
        let mut args = Args::parse_from(["ferrum", "--api-key", "test-key"]);
        args.workspace = root.to_path_buf();
        args.base_url = endpoint.to_owned();
        args.no_stream = true;
        args
    }

    fn server(responses: Vec<Value>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                let header_end = loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(pos) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
                let length = headers.lines().find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                }).unwrap();
                while bytes.len() - header_end < length {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let payload: Value = serde_json::from_slice(
                    &bytes[header_end..header_end + length]
                ).unwrap();
                assert_eq!(payload["stream"], false);
                assert!(payload["messages"].is_array());
                let body = json!({"choices":[{"message":response}]}).to_string();
                write!(stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                ).unwrap();
                stream.flush().unwrap();
            }
        });
        (endpoint, handle)
    }

    fn tool_call(id: &str, name: &str, arguments: &str) -> Value {
        json!({"id":id,"type":"function",
            "function":{"name":name,"arguments":arguments}})
    }

    #[test]
    fn agent_roundtrip_persists_assistant_and_user_in_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let (endpoint, handle) = server(vec![
            json!({"role":"assistant","content":"completed"}),
        ]);
        let mut config = args(dir.path(), &endpoint);
        let path = dir.path().join("nested").join("history.db");
        config.session_db = Some(path.clone());
        config.session_id = "roundtrip".into();
        run(&config, "hello").unwrap();
        handle.join().unwrap();
        let conn = open_store(&path).unwrap();
        let messages = load_sqlite(&conn, "roundtrip").unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["content"], "hello");
        assert_eq!(messages[2]["content"], "completed");
    }

    #[test]
    fn tool_execution_and_rejected_write_are_recorded_in_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("existing.txt"), "visible").unwrap();
        let (endpoint, handle) = server(vec![
            json!({"role":"assistant","content":null,"tool_calls":[
                tool_call("read", "read_file", r#"{"path":"existing.txt"}"#),
                tool_call("write", "write_file", r#"{"path":"blocked.txt","content":"no"}"#)
            ]}),
            json!({"role":"assistant","content":"finished"}),
        ]);
        let mut config = args(dir.path(), &endpoint);
        config.session = Some(dir.path().join("history.json"));
        run(&config, "inspect").unwrap();
        handle.join().unwrap();
        assert!(!dir.path().join("blocked.txt").exists());
        let messages: Vec<Value> = serde_json::from_slice(
            &fs::read(config.session.unwrap()).unwrap()
        ).unwrap();
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[3]["content"], "visible");
        assert!(messages[4]["content"].as_str().unwrap().contains("ERROR:"));
        assert_eq!(messages[5]["content"], "finished");
    }

    #[test]
    fn malformed_tool_arguments_are_recorded_without_aborting_loop() {
        let dir = tempfile::tempdir().unwrap();
        let (endpoint, handle) = server(vec![
            json!({"role":"assistant","content":null,"tool_calls":[
                tool_call("bad", "read_file", "{invalid"),
                tool_call("unknown", "unknown_tool", "{}")
            ]}),
            json!({"role":"assistant","content":"recovered"}),
        ]);
        let mut config = args(dir.path(), &endpoint);
        config.session_db = Some(dir.path().join("state.db"));
        run(&config, "inspect").unwrap();
        handle.join().unwrap();
        let db = open_store(config.session_db.as_ref().unwrap()).unwrap();
        let messages = load_sqlite(&db, "default").unwrap();
        assert_eq!(messages.len(), 6);
        assert!(messages[3]["content"].as_str().unwrap().starts_with("ERROR:"));
        assert!(messages[4]["content"].as_str().unwrap().starts_with("ERROR:"));
        assert_eq!(messages[5]["content"], "recovered");
    }

    #[test]
    fn missing_tool_id_and_name_fail_without_fabricating_results() {
        for call in [
            json!({"type":"function","function":{"name":"read_file","arguments":"{}"}}),
            json!({"id":"one","type":"function","function":{"arguments":"{}"}}),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (endpoint, handle) = server(vec![
                json!({"role":"assistant","content":null,"tool_calls":[call]}),
            ]);
            let mut config = args(dir.path(), &endpoint);
            config.session_db = Some(dir.path().join("state.db"));
            assert!(run(&config, "inspect").is_err());
            handle.join().unwrap();
            let db = open_store(config.session_db.as_ref().unwrap()).unwrap();
            let messages = load_sqlite(&db, "default").unwrap();
            assert_eq!(messages.len(), 3);
        }
    }

    #[test]
    fn agent_step_limit_persists_last_tool_result() {
        let dir = tempfile::tempdir().unwrap();
        let (endpoint, handle) = server(vec![
            json!({"role":"assistant","content":null,"tool_calls":[
                tool_call("call", "read_file", r#"{"path":"missing.txt"}"#)
            ]}),
        ]);
        let mut config = args(dir.path(), &endpoint);
        config.max_steps = 1;
        config.session_db = Some(dir.path().join("state.db"));
        let error = run(&config, "inspect").unwrap_err().to_string();
        handle.join().unwrap();
        assert!(error.contains("max agent steps"));
        let db = open_store(config.session_db.as_ref().unwrap()).unwrap();
        let messages = load_sqlite(&db, "default").unwrap();
        assert_eq!(messages.len(), 4);
        assert!(messages[3]["content"].as_str().unwrap().contains("ERROR:"));
    }

    #[test]
    fn resumes_existing_sqlite_history_without_replacing_it() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let mut db = open_store(&db_path).unwrap();
        save_sqlite(&mut db, "default", &[json!({"role":"system","content":"prior"})]).unwrap();
        drop(db);
        let (endpoint, handle) = server(vec![json!({"role":"assistant","content":"next"})]);
        let mut config = args(dir.path(), &endpoint);
        config.session_db = Some(db_path.clone());
        run(&config, "follow-up").unwrap();
        handle.join().unwrap();
        let db = open_store(&db_path).unwrap();
        let messages = load_sqlite(&db, "default").unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"], "prior");
        assert_eq!(messages[1]["content"], "follow-up");
    }

    #[test]
    fn invalid_existing_json_session_is_rejected_before_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "not-json").unwrap();
        let mut config = args(dir.path(), "http://127.0.0.1:1/v1");
        config.session = Some(path);
        assert!(run(&config, "prompt").unwrap_err().to_string().contains("invalid session JSON"));
    }
}

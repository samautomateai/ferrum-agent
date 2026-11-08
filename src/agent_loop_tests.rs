use super::*;

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

use super::*;

// Chat Completions streaming: merge content and tool call deltas by their stable index.
pub(super) fn merge_delta(message: &mut Value, delta: &Value) -> Result<()> {
    if let Some(content) = delta.get("content").and_then(Value::as_str) {
        let previous = message["content"].as_str().unwrap_or("");
        message["content"] = json!(format!("{previous}{content}"));
    }
    if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let index = call.get("index").and_then(Value::as_u64).context("stream tool call missing index")? as usize;
            if index > 128 { bail!("too many tool calls in stream"); }
            let existing = message["tool_calls"].as_array_mut().context("tool_calls not an array")?;
            while existing.len() <= index {
                existing.push(json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
            }
            let target = &mut existing[index];
            for (field, path) in [("id", "id"), ("name", "function.name"), ("arguments", "function.arguments")] {
                let fragment = match path {
                    "id" => call.get("id"),
                    "function.name" => call.pointer("/function/name"),
                    _ => call.pointer("/function/arguments"),
                }.and_then(Value::as_str);
                if let Some(fragment) = fragment {
                    let slot = if field == "id" { &mut target["id"] } else { &mut target["function"][field] };
                    let previous = slot.as_str().unwrap_or("");
                    *slot = json!(format!("{previous}{fragment}"));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn parse_sse_event(message: &mut Value, payload: &str) -> Result<bool> {
    if payload == "[DONE]" { return Ok(true); }
    let event: Value = serde_json::from_str(payload).context("invalid SSE JSON")?;
    if let Some(error) = event.get("error") { bail!("provider stream error: {error}"); }
    if let Some(delta) = event.pointer("/choices/0/delta") { merge_delta(message, delta)?; }
    Ok(false)
}

pub(super) fn request_message(client: &reqwest::blocking::Client, url: &str, key: &str, model: &str,
        messages: &[Value], stream: bool, definitions: &Value) -> Result<Value> {
    let response = client.post(url).bearer_auth(key).json(&json!({
        "model": model, "messages": messages, "tools": definitions,
        "tool_choice": "auto", "stream": stream
    })).send()?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text()?;
        bail!("provider HTTP {status}: {}", body.chars().take(2000).collect::<String>());
    }
    if !stream {
        let body: Value = response.json().context("invalid JSON response")?;
        return body.pointer("/choices/0/message").cloned().context("missing assistant message");
    }
    let mut message = json!({"role":"assistant","content":"","tool_calls":[]});
    let mut done = false;
    let mut data = String::new();
    let mut bytes_received = 0usize;
    let mut finish_reason: Option<String> = None;
    for line in io::BufReader::new(response).lines() {
        let line = line?;
        bytes_received = bytes_received.saturating_add(line.len());
        if bytes_received > 8 * 1024 * 1024 { bail!("provider stream exceeded 8 MiB limit"); }
        if line.is_empty() {
            if !data.is_empty() {
                if let Ok(event) = serde_json::from_str::<Value>(data.trim_end()) {
                    if let Some(reason) = event.pointer("/choices/0/finish_reason")
                        .and_then(Value::as_str) {
                        finish_reason = Some(reason.to_owned());
                    }
                }
                done = parse_sse_event(&mut message, data.trim_end())?;
                data.clear();
                if done { break; }
            }
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() { data.push('\n'); }
            data.push_str(value.trim_start());
        }
    }
    if !done && !data.is_empty() { done = parse_sse_event(&mut message, data.trim_end())?; }
    if !done { bail!("provider stream ended without [DONE]"); }
    if !matches!(finish_reason.as_deref(), Some("stop" | "tool_calls")) {
        bail!("provider stream missing successful finish reason");
    }
    if finish_reason.as_deref() == Some("length") { bail!("provider truncated completion (finish_reason=length)"); }
    if message["tool_calls"].as_array().is_some_and(Vec::is_empty) {
        message.as_object_mut().context("message object missing")?.remove("tool_calls");
    }
    Ok(message)
}


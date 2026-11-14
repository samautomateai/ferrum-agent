//! Model-independent assistant turn processing shared by CLI and future ACP/TUI frontends.
//! Tool calls are executed in order and every result is persisted before the next call.
use anyhow::{Context, Result};
use serde_json::{json, Value};

/// Stable core events consumed by CLI, ACP, TUI and future remote transports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentEvent {
    AssistantCompleted {
        content: String,
    },
    ToolStarted {
        id: String,
        name: String,
    },
    ToolFinished {
        id: String,
        name: String,
        success: bool,
    },
}

pub(crate) enum TurnOutcome {
    Complete(String),
    Continue,
}

/// Append a provider message, execute its tool calls, and durably record each outcome.
/// A malformed tool payload is returned to the model as an error rather than
/// silently dropping the tool_call_id (which would corrupt the conversation).
pub(crate) fn process_turn(
    messages: &mut Vec<Value>,
    message: Value,
    mut persist: impl FnMut(&[Value]) -> Result<()>,
    dispatch: impl FnMut(&str, &Value) -> Result<String>,
) -> Result<TurnOutcome> {
    process_turn_with_events(messages, message, persist, dispatch, |_| {})
}

/// Event-aware execution; callbacks are observations, never authorization decisions.
pub(crate) fn process_turn_with_events(
    messages: &mut Vec<Value>,
    message: Value,
    mut persist: impl FnMut(&[Value]) -> Result<()>,
    mut dispatch: impl FnMut(&str, &Value) -> Result<String>,
    mut emit: impl FnMut(AgentEvent),
) -> Result<TurnOutcome> {
    let calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    messages.push(message.clone());
    persist(messages)?;
    if calls.is_empty() {
        let content = message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("(no content)")
            .to_owned();
        emit(AgentEvent::AssistantCompleted {
            content: content.clone(),
        });
        return Ok(TurnOutcome::Complete(content));
    }
    for call in calls {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .context("tool call missing id")?;
        let name = call.pointer("/function/name").and_then(Value::as_str);
        let event_name = name.unwrap_or("<missing>").to_owned();
        emit(AgentEvent::ToolStarted {
            id: id.to_owned(),
            name: event_name.clone(),
        });
        let result = match name {
            Some(name) => {
                let raw = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                serde_json::from_str::<Value>(raw)
                    .map_err(anyhow::Error::from)
                    .and_then(|params| dispatch(name, &params))
            }
            None => Err(anyhow::anyhow!("tool call missing name")),
        };
        let success = result.is_ok();
        let output = match result {
            Ok(value) => value,
            Err(error) => format!("ERROR: {error:#}"),
        };
        messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
        persist(messages)?;
        emit(AgentEvent::ToolFinished {
            id: id.to_owned(),
            name: event_name,
            success,
        });
    }
    Ok(TurnOutcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_order_and_status_are_observable_after_persistence() {
        let mut history = vec![];
        let mut events = vec![];
        let mut saves = 0;
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"one","function":{"name":"read_file","arguments":"{}"}},
            {"id":"two","function":{"name":"write_file","arguments":"{}"}}
        ]});
        process_turn_with_events(
            &mut history,
            message,
            |_| {
                saves += 1;
                Ok(())
            },
            |name, _| {
                if name == "write_file" {
                    anyhow::bail!("denied")
                } else {
                    Ok("ok".into())
                }
            },
            |event| events.push(event),
        )
        .unwrap();
        assert_eq!(saves, 3);
        assert_eq!(
            events,
            vec![
                AgentEvent::ToolStarted {
                    id: "one".into(),
                    name: "read_file".into()
                },
                AgentEvent::ToolFinished {
                    id: "one".into(),
                    name: "read_file".into(),
                    success: true
                },
                AgentEvent::ToolStarted {
                    id: "two".into(),
                    name: "write_file".into()
                },
                AgentEvent::ToolFinished {
                    id: "two".into(),
                    name: "write_file".into(),
                    success: false
                },
            ]
        );
    }

    #[test]
    fn completion_is_persisted_without_dispatch() {
        let mut history = vec![];
        let mut saves = 0;
        let outcome = process_turn(
            &mut history,
            json!({"role":"assistant","content":"done"}),
            |_| {
                saves += 1;
                Ok(())
            },
            |_, _| panic!("unexpected dispatch"),
        )
        .unwrap();
        assert!(matches!(outcome, TurnOutcome::Complete(ref s) if s == "done"));
        assert_eq!(saves, 1);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn multiple_calls_execute_in_order_and_persist_individually() {
        let mut history = vec![];
        let mut seen = vec![];
        let mut sizes = vec![];
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"a","function":{"name":"read_file","arguments":"{}"}},
            {"id":"b","function":{"name":"grep","arguments":"{}"}}
        ]});
        let outcome = process_turn(
            &mut history,
            message,
            |m| {
                sizes.push(m.len());
                Ok(())
            },
            |name, _| {
                seen.push(name.to_owned());
                Ok(name.to_owned())
            },
        )
        .unwrap();
        assert!(matches!(outcome, TurnOutcome::Continue));
        assert_eq!(seen, ["read_file", "grep"]);
        assert_eq!(sizes, [1, 2, 3]);
        assert_eq!(history[2]["tool_call_id"], "b");
    }

    #[test]
    fn invalid_arguments_produce_tool_result_without_execution() {
        let mut history = vec![];
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"bad","function":{"name":"read_file","arguments":"{"}}
        ]});
        process_turn(
            &mut history,
            message,
            |_| Ok(()),
            |_, _| panic!("malformed JSON must not execute"),
        )
        .unwrap();
        assert_eq!(history[1]["tool_call_id"], "bad");
        assert!(history[1]["content"]
            .as_str()
            .unwrap()
            .starts_with("ERROR:"));
    }

    #[test]
    fn persistence_failure_stops_before_tool_execution() {
        let mut history = vec![];
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"x","function":{"name":"write_file","arguments":"{}"}}
        ]});
        let error = process_turn(
            &mut history,
            message,
            |_| anyhow::bail!("storage unavailable"),
            |_, _| panic!("tool must not execute before assistant message is durable"),
        );
        assert!(error.is_err());
    }
}

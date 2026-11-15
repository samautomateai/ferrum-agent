//! Repairs a persisted assistant tool batch interrupted before all results were saved.
//! The provider must never receive unmatched tool_call IDs on the next request.
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Append explicit interruption results for outstanding tool calls in the last
/// assistant batch. Does not re-execute tools or mutate existing messages.
pub(crate) fn recover_pending_tools(history: &mut Vec<Value>) -> Result<usize> {
    let mut pending: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for message in history.iter() {
        match message.get("role").and_then(Value::as_str) {
            Some("assistant") => {
                if !pending.is_empty() {
                    bail!("unresolved tool calls before subsequent assistant message");
                }
                seen.clear();
                if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        let id = call.get("id").and_then(Value::as_str)
                            .filter(|id| !id.is_empty())
                            .ok_or_else(|| anyhow::anyhow!("invalid persisted tool call id"))?;
                        if !seen.insert(id.to_owned()) {
                            bail!("duplicate persisted tool call id: {id}");
                        }
                        pending.push(id.to_owned());
                    }
                }
            }
            Some("tool") => {
                let id = message.get("tool_call_id").and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("persisted tool result missing call id"))?;
                if let Some(position) = pending.iter().position(|value| value == id) {
                    pending.remove(position);
                } else {
                    bail!("orphan or duplicate persisted tool result: {id}");
                }
            }
            Some("user" | "system") if !pending.is_empty() => {
                bail!("unresolved tool calls before subsequent conversation message");
            }
            _ => {}
        }
    }
    let count = pending.len();
    for id in pending {
        history.push(json!({
            "role": "tool",
            "tool_call_id": id,
            "content": "ERROR: execution interrupted before a durable tool result; not retried"
        }));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str) -> Value {
        json!({"id":id,"type":"function","function":{"name":"read_file","arguments":"{}"}})
    }

    #[test]
    fn interrupted_batch_is_repaired_in_order_without_reexecution() {
        let mut history = vec![
            json!({"role":"user","content":"inspect"}),
            json!({"role":"assistant","tool_calls":[call("a"),call("b"),call("c")]}),
            json!({"role":"tool","tool_call_id":"a","content":"ok"}),
        ];
        assert_eq!(recover_pending_tools(&mut history).unwrap(), 2);
        assert_eq!(history[3]["tool_call_id"], "b");
        assert_eq!(history[4]["tool_call_id"], "c");
        assert!(history[3]["content"].as_str().unwrap().contains("not retried"));
        assert_eq!(recover_pending_tools(&mut history).unwrap(), 0);
    }

    #[test]
    fn completed_history_is_unchanged() {
        let mut history = vec![
            json!({"role":"assistant","tool_calls":[call("a")]}),
            json!({"role":"tool","tool_call_id":"a","content":"done"}),
            json!({"role":"assistant","content":"finished"}),
        ];
        let original = history.clone();
        assert_eq!(recover_pending_tools(&mut history).unwrap(), 0);
        assert_eq!(history, original);
    }

    #[test]
    fn orphan_tool_result_is_rejected() {
        let mut history = vec![json!({"role":"tool","tool_call_id":"ghost","content":"ok"})];
        assert!(recover_pending_tools(&mut history).is_err());
    }

    #[test]
    fn intervening_user_message_is_rejected_without_mutation() {
        let mut history = vec![
            json!({"role":"assistant","tool_calls":[call("a")]}),
            json!({"role":"user","content":"new prompt"}),
        ];
        let original = history.clone();
        assert!(recover_pending_tools(&mut history).is_err());
        assert_eq!(history, original);
    }

    #[test]
    fn duplicate_call_ids_are_rejected() {
        let mut history = vec![json!({"role":"assistant","tool_calls":[call("a"),call("a")]})];
        assert!(recover_pending_tools(&mut history).is_err());
    }
}

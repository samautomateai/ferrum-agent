//! Tokio-native agent orchestration independent of UI and model transport.
//! Sequential execution preserves tool-call order and persistence boundaries.
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{future::Future, pin::Pin};
use tokio::sync::watch;

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Clone)]
pub(crate) struct Cancellation {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl Cancellation {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    pub(crate) fn cancel(&self) {
        self.sender.send_replace(true);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    pub(crate) async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        let mut receiver = self.receiver.clone();
        while receiver.changed().await.is_ok() {
            if *receiver.borrow_and_update() {
                return;
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum AsyncOutcome {
    Complete(String),
    NeedsModel,
}

pub(crate) async fn process_turn<P, D>(
    history: &mut Vec<Value>,
    message: Value,
    cancellation: &Cancellation,
    mut persist: P,
    mut dispatch: D,
) -> Result<AsyncOutcome>
where
    P: for<'a> FnMut(&'a [Value]) -> BoxFuture<'a, ()>,
    D: for<'a> FnMut(&'a str, &'a Value) -> BoxFuture<'a, String>,
{
    if cancellation.is_cancelled() {
        bail!("agent cancelled before assistant turn");
    }
    let calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Validate the entire batch before persisting the assistant message.
    // Otherwise a malformed later call can strand earlier tool IDs in history.
    let mut seen_ids = std::collections::HashSet::new();
    for call in &calls {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing tool call id"))?;
        if !seen_ids.insert(id) {
            bail!("duplicate tool call id: {id}");
        }
    }
    history.push(message.clone());
    persist(history).await?;
    if calls.is_empty() {
        let content = message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("(no content)");
        return Ok(AsyncOutcome::Complete(content.to_owned()));
    }
    let mut was_cancelled = false;
    for call in calls {
        if cancellation.is_cancelled() {
            was_cancelled = true;
        }
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing tool call id"))?;
        let name = call.pointer("/function/name").and_then(Value::as_str);
        let mut interrupted = false;
        let result = match name {
            _ if was_cancelled => Err(anyhow::anyhow!("agent cancelled before tool execution")),
            Some(name) => {
                let raw = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                match serde_json::from_str::<Value>(raw) {
                    Ok(args) => tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => {
                            interrupted = true;
                            Err(anyhow::anyhow!("agent cancelled during tool execution"))
                        },
                        result = dispatch(name, &args) => result,
                    },
                    Err(error) => Err(error.into()),
                }
            }
            None => Err(anyhow::anyhow!("missing tool name")),
        };
        let output = match result {
            Ok(value) => value,
            Err(error) => format!("ERROR: {error:#}"),
        };
        history.push(json!({"role":"tool","tool_call_id":id,"content":output}));
        persist(history).await?;
        if interrupted {
            was_cancelled = true;
        }
    }
    if was_cancelled {
        bail!("agent cancelled; outstanding tool results persisted");
    }
    Ok(AsyncOutcome::NeedsModel)
}

/// Executes provider turns until completion, cancellation, or the bounded step limit.
pub(crate) async fn run<P, M, D>(
    history: &mut Vec<Value>,
    max_steps: usize,
    cancellation: &Cancellation,
    mut persist: P,
    mut model: M,
    mut dispatch: D,
) -> Result<String>
where
    P: for<'a> FnMut(&'a [Value]) -> BoxFuture<'a, ()>,
    M: for<'a> FnMut(&'a [Value]) -> BoxFuture<'a, Value>,
    D: for<'a> FnMut(&'a str, &'a Value) -> BoxFuture<'a, String>,
{
    if max_steps == 0 {
        bail!("max_steps must be positive");
    }
    for _ in 0..max_steps {
        if cancellation.is_cancelled() {
            bail!("agent cancelled");
        }
        let response = tokio::select! {
            biased;
            _ = cancellation.cancelled() => bail!("agent cancelled during model request"),
            result = model(history) => result?,
        };
        match process_turn(history, response, cancellation, &mut persist, &mut dispatch).await? {
            AsyncOutcome::Complete(content) => return Ok(content),
            AsyncOutcome::NeedsModel => {}
        }
    }
    bail!("maximum agent steps reached")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    fn persist<'a>(_: &'a [Value]) -> BoxFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn tool<'a>(_: &'a str, _: &'a Value) -> BoxFuture<'a, String> {
        Box::pin(async { Ok("ok".into()) })
    }
    fn call(name: &str, arguments: &str) -> Value {
        json!({"role":"assistant","tool_calls":[{"id":"one","function":{"name":name,"arguments":arguments}}]})
    }

    #[tokio::test]
    async fn invalid_later_call_does_not_persist_partial_assistant_turn() {
        let mut history = vec![];
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"valid","function":{"name":"read_file","arguments":"{}"}},
            {"function":{"name":"read_file","arguments":"{}"}}
        ]});
        let result = process_turn(&mut history, message, &Cancellation::new(), persist, tool).await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing tool call id"));
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn duplicate_call_ids_rejected_before_persistence() {
        let mut history = vec![];
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"same","function":{"name":"read_file","arguments":"{}"}},
            {"id":"same","function":{"name":"read_file","arguments":"{}"}}
        ]});
        let result = process_turn(&mut history, message, &Cancellation::new(), persist, tool).await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("duplicate tool call id"));
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn completion_does_not_execute_tools() {
        let mut history = vec![];
        let outcome = process_turn(
            &mut history,
            json!({"role":"assistant","content":"done"}),
            &Cancellation::new(),
            persist,
            tool,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, AsyncOutcome::Complete(ref value) if value == "done"));
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn malformed_json_is_recorded_without_dispatch() {
        let mut history = vec![];
        process_turn(
            &mut history,
            call("read_file", "{"),
            &Cancellation::new(),
            persist,
            tool,
        )
        .await
        .unwrap();
        assert_eq!(history[1]["tool_call_id"], "one");
        assert!(history[1]["content"]
            .as_str()
            .unwrap()
            .starts_with("ERROR:"));
    }

    #[tokio::test]
    async fn missing_tool_name_is_recorded() {
        let mut history = vec![];
        let message =
            json!({"role":"assistant","tool_calls":[{"id":"one","function":{"arguments":"{}"}}]});
        process_turn(&mut history, message, &Cancellation::new(), persist, tool)
            .await
            .unwrap();
        assert!(history[1]["content"]
            .as_str()
            .unwrap()
            .contains("missing tool name"));
    }

    #[tokio::test]
    async fn cancelled_before_turn_does_not_mutate_history() {
        let cancellation = Cancellation::new();
        cancellation.cancel();
        let mut history = vec![];
        assert!(process_turn(
            &mut history,
            call("read_file", "{}"),
            &cancellation,
            persist,
            tool
        )
        .await
        .is_err());
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn cancellation_interrupts_pending_model() {
        let cancellation = Cancellation::new();
        let signal = cancellation.clone();
        let mut history = vec![];
        let task = tokio::spawn(async move {
            run(
                &mut history,
                3,
                &signal,
                persist,
                |_| Box::pin(async { std::future::pending::<Result<Value>>().await }),
                tool,
            )
            .await
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        assert!(task.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn cancelled_tool_has_persisted_matching_result() {
        let cancellation = Cancellation::new();
        let signal = cancellation.clone();
        let task = tokio::spawn(async move {
            let mut history = vec![];
            let result = process_turn(
                &mut history,
                call("read_file", "{}"),
                &signal,
                persist,
                |_, _| Box::pin(async { std::future::pending::<Result<String>>().await }),
            )
            .await;
            (result, history)
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        let (result, history) = task.await.unwrap();
        assert!(result.is_err());
        assert_eq!(history.len(), 2);
        assert_eq!(history[1]["tool_call_id"], "one");
        assert!(history[1]["content"]
            .as_str()
            .unwrap()
            .contains("cancelled"));
    }

    #[tokio::test]
    async fn cancellation_records_results_for_all_remaining_calls() {
        let cancellation = Cancellation::new();
        let signal = cancellation.clone();
        let task = tokio::spawn(async move {
            let mut history = vec![];
            let message = json!({"role":"assistant","tool_calls":[
                {"id":"first","function":{"name":"read_file","arguments":"{}"}},
                {"id":"second","function":{"name":"write_file","arguments":"{}"}}
            ]});
            let result = process_turn(&mut history, message, &signal, persist, |_, _| {
                Box::pin(async { std::future::pending::<Result<String>>().await })
            })
            .await;
            (result, history)
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        let (result, history) = task.await.unwrap();
        assert!(result.is_err());
        assert_eq!(history.len(), 3);
        assert_eq!(history[1]["tool_call_id"], "first");
        assert_eq!(history[2]["tool_call_id"], "second");
        for message in &history[1..] {
            assert!(message["content"].as_str().unwrap().contains("cancelled"));
        }
    }

    #[tokio::test]
    async fn cancellation_after_first_tool_skips_remaining_dispatches() {
        let cancellation = Cancellation::new();
        let signal = cancellation.clone();
        let dispatched = Arc::new(AtomicUsize::new(0));
        let observed = dispatched.clone();
        let mut history = vec![];
        let message = json!({"role":"assistant","tool_calls":[
            {"id":"first","function":{"name":"read_file","arguments":"{}"}},
            {"id":"second","function":{"name":"write_file","arguments":"{}"}},
            {"id":"third","function":{"name":"read_file","arguments":"{}"}}
        ]});
        let result = process_turn(&mut history, message, &signal, persist, move |_, _| {
            let signal = cancellation.clone();
            let observed = observed.clone();
            Box::pin(async move {
                observed.fetch_add(1, Ordering::SeqCst);
                signal.cancel();
                Ok("first completed".into())
            })
        })
        .await;
        assert!(result.is_err());
        assert_eq!(dispatched.load(Ordering::SeqCst), 1);
        assert_eq!(history.len(), 4);
        assert_eq!(history[1]["tool_call_id"], "first");
        assert_eq!(history[1]["content"], "first completed");
        assert_eq!(history[2]["tool_call_id"], "second");
        assert_eq!(history[3]["tool_call_id"], "third");
        for entry in &history[2..] {
            assert!(entry["content"].as_str().unwrap().contains("cancelled"));
        }
    }

    #[tokio::test]
    async fn cancellation_interrupts_pending_tool() {
        let cancellation = Cancellation::new();
        let signal = cancellation.clone();
        let task = tokio::spawn(async move {
            let mut history = vec![];
            process_turn(
                &mut history,
                call("read_file", "{}"),
                &signal,
                persist,
                |_, _| Box::pin(async { std::future::pending::<Result<String>>().await }),
            )
            .await
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        assert!(task.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn model_step_limit_is_enforced() {
        let mut history = vec![];
        let result = run(
            &mut history,
            2,
            &Cancellation::new(),
            persist,
            |_| Box::pin(async { Ok(call("read_file", "{}")) }),
            tool,
        )
        .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("maximum agent steps"));
        assert_eq!(history.len(), 4);
    }

    #[tokio::test]
    async fn zero_steps_rejected() {
        let mut history = vec![];
        assert!(run(
            &mut history,
            0,
            &Cancellation::new(),
            persist,
            |_| Box::pin(async { Ok(json!({"role":"assistant","content":"done"})) }),
            tool
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn persistence_failure_prevents_dispatch() {
        let counter = Arc::new(AtomicUsize::new(0));
        let seen = counter.clone();
        let mut history = vec![];
        let result = process_turn(
            &mut history,
            call("write_file", "{}"),
            &Cancellation::new(),
            |_| Box::pin(async { bail!("disk failure") }),
            move |_, _| {
                seen.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok("written".into()) })
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn model_and_tool_roundtrip() {
        let mut history = vec![];
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let answer = run(
            &mut history,
            3,
            &Cancellation::new(),
            persist,
            move |_| {
                let turn = seen.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    if turn == 0 {
                        Ok(call("read_file", "{}"))
                    } else {
                        Ok(json!({"role":"assistant","content":"finished"}))
                    }
                })
            },
            tool,
        )
        .await
        .unwrap();
        assert_eq!(answer, "finished");
        assert_eq!(history.len(), 3);
    }
}

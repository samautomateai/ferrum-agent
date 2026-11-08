//! Process-wide Tokio runtime ownership, separate from Boa worker execution.
//! The synchronous compatibility facade must reject calls from Tokio contexts.
use anyhow::{bail, Context, Result};
use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

pub fn runtime() -> Result<&'static Runtime> {
    match RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrum-supervisor")
            .enable_all()
            .build()
            .map_err(|error| format!("Tokio supervisor runtime initialization failed: {error}"))
    }) {
        Ok(runtime) => Ok(runtime),
        Err(error) => anyhow::bail!("{error}"),
    }
}

pub fn run_sync<F: Future>(future: F) -> Result<F::Output> {
    if tokio::runtime::Handle::try_current().is_ok() {
        bail!("synchronous worker facade cannot be called from a Tokio runtime");
    }
    Ok(runtime()?.block_on(future))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn concurrent_initialization_uses_one_runtime() {
        let handles: Vec<_> = (0..16)
            .map(|_| std::thread::spawn(|| runtime().unwrap() as *const Runtime as usize))
            .collect();
        let pointers: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
        assert!(pointers.iter().all(|pointer| *pointer == pointers[0]));
    }

    #[test]
    fn runtime_is_shared_across_callers() {
        let first = runtime().unwrap() as *const Runtime;
        let second = runtime().unwrap() as *const Runtime;
        assert_eq!(first, second);
    }

    #[test]
    fn sync_bridge_runs_async_deadline() {
        let value = run_sync(async {
            tokio::time::timeout(Duration::from_secs(1), async { 42_u8 })
                .await
                .unwrap()
        })
        .unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn bounded_channel_rejects_excess_requests() {
        let result = run_sync(async {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(1);
            tx.try_send(1).unwrap();
            assert!(matches!(
                tx.try_send(2),
                Err(tokio::sync::mpsc::error::TrySendError::Full(2))
            ));
            assert_eq!(rx.recv().await, Some(1));
        });
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_nested_runtime_bridge() {
        let rejected = run_sync(async { run_sync(async { 1_u8 }).unwrap_err() }).unwrap();
        assert!(rejected.to_string().contains("cannot be called"));
    }
}

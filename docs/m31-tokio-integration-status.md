# M3.1 Tokio transport integration status (PR #10)

Status: **draft / unverified**. The production `ExtensionWorker` synchronous facade now delegates to the shared Tokio supervisor. The old per-worker writer and reader threads have been removed from the facade. This is not yet validated by CI.

## Implemented on branch
- Register the Tokio transport and lifecycle modules in the production binary.
- Preserve the synchronous `ExtensionWorker` API for load, invoke, emit and shutdown.
- Use the process-wide runtime bridge; reject reentrant calls from Tokio contexts.
- Translate supervisor outcomes into sanitized `RuntimeFailure` codes. Unknown outcomes remain non-retryable and are never automatically replayed.
- Retain an actor JoinHandle and capture its runtime handle for cleanup from synchronous Drop contexts.
- Add real-worker concurrency, repeated start/stop, deadline and synchronous-drop regression tests.

## Blockers before merging
1. CI must compile the new facade and pass rustfmt, clippy, security checks, >=90% line coverage, Linux amd64/arm64 and Windows amd64. Latest runs have been action_required or cancelled; no green evidence.
2. Prove that the supervisor always reaps the direct child after cancellation, shutdown timeout, and Drop; runtime shutdown can still interrupt an async reaper. Do not claim guaranteed cleanup yet.
3. Add subprocess fault-injection for broken pipe, malformed and truncated responses, and blocked shutdown.
4. Validate actual CLI extension compatibility and no regression in event and invoke behavior.
5. M3.2 still owns Windows Job Object and Linux process-group/cgroup descendant containment; killing a direct child does not imply tree cleanup.

Do not merge until these are resolved; do not relax quality gates or add Docker requirements.

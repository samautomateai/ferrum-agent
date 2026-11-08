# M3.1 experimental Tokio transport (PR #5)

These modules are staged source only and deliberately **not registered in src/main.rs**.
The existing synchronous ExtensionWorker remains active. Do not enable these modules
without first completing and testing the integration.

Outstanding blockers:
- Replace hard kill on normal shutdown with protocol handshake and bounded grace.
- Connect WorkerLifecycle to spawn, request, cancellation and shutdown.
- Handle cancelled callers and guarantee actor/child cleanup; reject late replies.
- Establish correct NotStarted vs Unknown classification for enqueue races.
- Add process-tree containment for Windows and Linux, or document platform limits.
- Add real subprocess fault-injection tests and validate 90% coverage.
- Compile, format, clippy, audit, and test on Linux amd64/arm64 and Windows amd64.
- Do not auto-replay any request whose side effects might have occurred.

## Shutdown contract follow-up

Shutdown now propagates the handshake/reaping result rather than unconditionally
acknowledging success. A response must be valid JSON with a result field and
no protocol error; child exit must succeed. On timeout the actor attempts to
kill and reap the direct child. Caller cancellation, process-tree containment,
full lifecycle integration and real subprocess tests remain unverified.

## Cancellation and deadline handling

The actor checks whether a queued caller has abandoned its response channel before
writing to the child. This prevents queued cancelled requests from executing.
Once IPC has begun, a caller timeout remains an Unknown outcome and MUST NOT
trigger replay. Shutdown now uses the caller's original deadline for enqueue,
handshake and process exit, rather than resetting the five-second clock.
Direct-child reaping after a failed shutdown can still exceed the caller deadline;
process-tree containment and comprehensive cancellation fault tests remain open.

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

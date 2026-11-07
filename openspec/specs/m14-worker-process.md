# M1.4 — Extension subprocess boundary

## Requirements
- Extension registration and invocation MUST execute in a child process, not the agent process.
- Child requests and responses MUST be JSON and size bounded.
- The parent MUST kill and reap a worker that exceeds a configurable wall-clock deadline.
- The agent MUST reject unknown tools and collisions with built-in tools.
- Each invocation starts a new process; mutable JS state is **not** retained across invocations.
- The worker MUST NOT claim to provide OS sandboxing, memory quotas, or capability isolation.
- CI MUST retain 90% line coverage and the existing quality gates.

## Acceptance
- A normal Promise-returning handler produces a JSON result.
- A syntax or runtime error returns an explicit failure.
- A runaway worker can be killed without killing the parent.
- Linux ARM64/AMD64 and Windows AMD64 builds require manual verification.

## Deferred
- Persistent per-extension workers and stateful hooks.
- Async host I/O, permissions, filesystem sandbox, process-group termination.
- OS memory/CPU limits, child-process tree cleanup and crash restart policy.

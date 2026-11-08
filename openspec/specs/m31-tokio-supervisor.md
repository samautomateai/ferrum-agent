# M3.1 follow-up — Tokio worker supervisor

Status: proposed. Scope: replace internal synchronous worker transport and lifecycle management incrementally; preserve the existing synchronous ExtensionWorker public interface and Boa process isolation.

## Requirements
- R1: Exactly one Tokio runtime owned by the agent process; never create one per worker. The synchronous facade MUST NOT block from within an existing Tokio runtime; detect and reject unsupported reentrant calls with a typed error.
- R2: Supervisor owns each child, stdin/stdout tasks, cancellation token, bounded command channel, lifecycle state and task JoinHandles. Lifecycle: Starting, Ready, Busy, Stopping, Stopped, Failed. Transitions must be explicit and testable.
- R3: Serialize requests per worker, correlate request and response, and use one monotonic deadline for enqueue, write, flush and read. No unbounded channels, spawned blocking writes, or unbounded frame allocation. Enforce existing 2 MiB frame limit.
- R4: Cancellation or timeout after a request might have been sent yields Unknown outcome, retryable=false. Only provably unsubmitted requests may return NotStarted. NEVER replay side-effecting requests automatically.
- R5: Graceful shutdown sends shutdown request with a bounded grace period; on failure kill and reap the process. Drop MUST not leave orphaned children or detached tasks. If synchronous drop cannot await, use an owned runtime reaper with a bounded fallback.
- R6: Windows process-tree containment requires Job Objects and Linux process-group/cgroup containment in M3.2. M3.1 must explicitly document that killing the direct child does not guarantee descendant cleanup.
- R7: Maintain sanitized public errors; internal causes never appear in user-facing output. Instrument lifecycle with tracing but do not log request bodies, responses, secrets, or extension source.
- R8: Keep current extension worker protocol and external behavior compatible. No async Boa engine migration.

## TDD acceptance matrix
- Normal handshake/invoke/event/shutdown, concurrent independent workers, and sequential same-worker requests.
- Cancellation before enqueue, during blocked stdin write, after write and before response, and during shutdown.
- Broken pipe, malformed/oversized/truncated frame, child exit, EOF, process spawn failure, and delayed response.
- Verify side-effect outcome Unknown after uncertain send, no replay, bounded queue backpressure and bounded elapsed time.
- Repeated start/stop cycles and child reaping; zero lingering task ownership after stop. Verify on Linux AMD64/ARM64 and Windows AMD64.
- Keep >=90% line coverage, clippy -D warnings, rustfmt, quality and security gates. Run OS platform builds only after those gates pass.

## Delivery slices
1. Add ADR and compile-time Tokio dependencies, runtime owner, lifecycle model and unit tests without changing existing worker path.
2. Implement async process transport with bounded IPC, deadline and cancellation tests; run alongside existing worker implementation.
3. Switch synchronous facade to Tokio supervisor, verify compatibility, remove legacy thread transport, validate cross-platform cleanup.
4. Add structured tracing and fault-injection regression tests; finish acceptance traceability.

Each slice requires green CI before the next; no weakening thresholds or unsafe fallback.

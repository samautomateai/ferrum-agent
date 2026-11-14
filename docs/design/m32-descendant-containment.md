# M3.2 — Descendant containment design proposal

Status: design proposal, no implementation authorized before review.
Related: [requirements](../requirements/m32-descendant-containment.md).

## Architecture
Retain M3.1 WorkerSupervisor and process-owner lifecycle. Introduce an OS-specific `ProcessContainment` abstraction created **before** the worker executes extension code. The containment handle is owned by the runtime-independent process owner and survives actor cancellation.

Linux candidate: start worker in a dedicated process group/session and send signals to that group, then reap the direct child. Group signaling alone cannot prove all escaped descendants have exited; tests must state this limitation. A delegated cgroup v2 option can provide stronger membership-based cleanup but needs permissions and explicit deployment support.

Windows candidate: create a Job Object with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE and assign the worker before resuming execution. Assignment after normal spawn creates an unacceptable race. Evaluate Windows API creation flags, suspended start, inheritance and nested job behavior before choosing a crate or FFI.

## Fail-closed startup
1. Validate containment support and create owner/handles.
2. Spawn suspended or otherwise prevent worker-controlled execution until containment assignment is complete.
3. Assign containment, verify success, then resume worker.
4. On any error, terminate/reap and return NotStarted; never expose a partially started worker.

## Teardown
Normal handshake remains unchanged. On forced shutdown, Drop, actor fault or runtime teardown, terminate containment, await direct-child reap and separately verify descendant exit in tests. Retain idempotent cleanup and fixed-field diagnostics.

## Implementation slices
1. Decide Linux and Windows containment primitives; ADR and negative tests for fail-closed startup.
2. Implement Linux containment and process-tree fixture tests.
3. Implement Windows Job Object containment and process-tree fixture tests.
4. Runtime-teardown, concurrency and fault injection; quality/coverage/CI evidence and sign-off.

## Risk register
- Process-group escape and orphaned descendants on Linux.
- Windows spawn-to-assignment race.
- Existing job restrictions and handle inheritance.
- PID reuse during process-tree observation.
- Platform CI may lack cgroup delegation.

No production implementation should proceed until these risks have explicit acceptance criteria.

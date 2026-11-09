# M3.1 requirement acceptance matrix

Status: **working evidence inventory, not approval**. Source: [requirements](m31-worker-supervision.md) and [design](../design/m31-worker-supervision.md).

| ID | OpenSpec | Evidence present | Missing evidence / decision | State |
| --- | --- | --- | --- | --- |
| M31-R1 | R1 | `src/supervisor_runtime.rs` unit tests | Confirm sync facade integration | Implemented, not accepted |
| M31-R2 | R2 | `src/worker_supervisor.rs`, actor observer tests | Runtime teardown and no leaked tasks | Partial |
| M31-R3 | R3 | `src/async_ipc.rs`, frame and deadline tests | Blocked stdin, queue saturation; cancelled queued-request regression pending CI | Partial |
| M31-R4 | R4 | `tests/tokio_supervisor.rs` timeout, crash follow-up and no-replay tests | Deterministic post-write cancellation fixture | Partial |
| M31-R5 | R5 | Actor cleanup and Linux `/proc` liveness tests | Windows OS liveness; runtime teardown | Partial |
| M31-R6 | R8 | Real worker handshake/invoke/event/shutdown tests | Full CLI compatibility coverage | Partial |
| M31-R7 | R7 | Sanitized error contract | Structured tracing and secret-redaction tests | Partial |
| M31-R8 | R6 | Explicit M3.2 deferral | Confirm handoff during sign-off | Proposed |
| M31-N1 | R2-R3 | Bounded channel/frame/deadline | Blocked I/O and shutdown latency | Partial |
| M31-N2 | Acceptance matrix | CI platform jobs | Windows liveness assertions | Partial |
| M31-N3 | Acceptance matrix | CI quality/test jobs | Actual measured line coverage >=90% | Unverified |
| M31-N4 | Acceptance matrix | Crash/malformed/timeout/spawn-failure tests | Broken pipe, EOF, truncated response, blocked shutdown and teardown | Partial |

Do not mark any row accepted without explicit maintainer sign-off and linked evidence. Current CI baseline: [42592057581](https://github.com/samautomateai/ferrum-agent/actions/runs/42592057581); inspect job results before claiming this run is fully green.

## Cohesive fault-contract batch (pending CI)

- Missing executable must return `NotStarted(NotFound)` without spawning an actor.
- Crashed worker must complete cleanup and reject follow-up requests without silently restarting; Linux also verifies PID disappearance.
- Invalid pre-dispatch frame must return `NotStarted(InvalidInput)` without poisoning or replacing a healthy child.
- These tests do **not** close blocked I/O, truncated response, runtime teardown, Windows OS-liveness or tracing acceptance gaps.

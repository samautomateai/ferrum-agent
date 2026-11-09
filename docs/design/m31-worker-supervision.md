# M3.1 — Worker supervision technical design

Status: **proposed; maintainer approval required**. Revision: 2 (2026-11-09).
Inputs: [requirements](../requirements/m31-worker-supervision.md), [OpenSpec](../../openspec/specs/m31-tokio-supervisor.md).

## Selected approach (subject to approval)

`ExtensionWorker` keeps its synchronous public API. `supervisor_runtime` owns one shared Tokio runtime. Each `WorkerSupervisor` owns a bounded mpsc command sender, an actor JoinHandle and a runtime handle. A dedicated process-owner thread owns the direct child and waits for its exit on an independent current-thread Tokio runtime. The actor owns framed stdin/stdout transport and a shared handle to the process owner. Each request is serialized and uses a monotonic deadline.

```text
CLI / ExtensionWorker (sync)
          |
  shared Tokio runtime bridge
          |
  WorkerSupervisor -- bounded mpsc --> actor
          |                           |-- framed stdin/stdout
          |                           |-- lifecycle state
          |                           +-- ProcessOwner handle
          |                                      |
          |                              independent thread/runtime
          |                                      |-- Child handle
          |                                      +-- kill / wait / reap
          +-- exit observer <---------+-- actor completion
```

## Requirement-to-design trace

| Requirement | Design decision / verification |
| --- | --- |
| M31-R1 | Process-wide `OnceLock<Runtime>`; reject nested `run_sync` |
| M31-R2 | Independent process owner owns child; actor owns IPC; bounded mpsc |
| M31-R3 | 2 MiB frame limit and single deadline in `FramedIpc` |
| M31-R4 | Distinguish `NotStarted` and `Unknown`; no retry in facade |
| M31-R5 | Shutdown handshake; independent process owner performs child wait/kill across shared-runtime teardown; OS-level verification still required |
| M31-R6 | Preserve JSON line protocol and synchronous facade |
| M31-R7 | Sanitized public errors and fixed-field JSON lifecycle diagnostics; review production logging policy |
| M31-R8 | Direct child only; descendants deferred to M3.2 |

## Nonfunctional requirement trace

| Requirement | Design and verification |
| --- | --- |
| M31-N1 | Bounded IPC, bounded queue, deadline-aware cleanup and blocked read/write tests |
| M31-N2 | Linux and Windows platform matrix; Windows direct-child liveness covered in CI |
| M31-N3 | Existing quality and coverage gates; CI coverage artifact exists; exact percentage must be recorded |
| M31-N4 | Fault-injection tests for malformed, truncated, EOF, broken pipe, shutdown and teardown; incomplete |

## Failure behavior

- Rejected before dispatch: `NotStarted`; worker remains usable.
- Potentially transmitted request times out or pipe closes: `Unknown`; actor must stop and never replay.
- Graceful shutdown: send shutdown frame, validate acknowledgement, wait for child exit within grace.
- Shutdown timeout: kill child and await reap; caller deadline stays bounded.
- Drop: close command channel; if the shared runtime tears down, the actor drops its process-owner handle and signals termination. The independent owner thread continues to wait/reap. Runtime construction failure and OS-level proof remain explicit review risks.
- Worker exit/crash: close actor, reap direct child, reject further requests.

## Alternatives and decisions pending

1. **Async reaper on shared runtime (rejected):** runtime teardown can abort cleanup.
2. **Dedicated process-owner thread (implemented provisionally):** independent runtime keeps waiting after shared-runtime teardown; incurs one OS thread per worker, requiring scalability and shutdown review.
3. **Shared independent process reaper service (deferred):** reduces thread count but adds ownership and queue complexity.

Maintainer approval of the implemented choice and residual risks is pending.

## Security, compatibility and observability

No request/response bodies, source, API keys or environment values in tracing. Preserve protocol and error contracts. Direct-child termination is not a sandbox; descendant containment belongs to M3.2.

## Test and rollout plan

1. Prove baseline normal invoke/event/shutdown and failure semantics.
2. Inject broken pipe, EOF, truncated output, blocked writes and blocked shutdown with deterministic worker fixtures.
3. Assert OS-level direct-child disappearance after shutdown, timeout and Drop on all supported OSes.
4. Simulate runtime teardown and verify no detached child/reaper; add bounded latency assertions.
5. Record coverage and all six CI job results; update readiness checkpoint and request maintainer sign-off.

Rollback: revert supervisor integration to the last verified worker transport only through a reviewed change preserving API behavior.

## Approval record

- Requirements approval: **pending**
- Design approval: **pending**
- Accepted risks: **pending**

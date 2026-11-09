# M3.1 — Worker supervision technical design

Status: **proposed; maintainer approval required**. Revision: 1 (2026-11-09).
Inputs: [requirements](../requirements/m31-worker-supervision.md), [OpenSpec](../../openspec/specs/m31-tokio-supervisor.md).

## Selected approach (subject to approval)

`ExtensionWorker` keeps its synchronous public API. `supervisor_runtime` owns one shared Tokio runtime. Each `WorkerSupervisor` owns a bounded mpsc command sender, an actor JoinHandle and a runtime handle. The actor owns the spawned direct child and framed stdin/stdout transport. Each request is serialized and uses a monotonic deadline.

```text
CLI / ExtensionWorker (sync)
          |
  shared Tokio runtime bridge
          |
  WorkerSupervisor -- bounded mpsc --> actor
          |                           |-- Child handle (kill_on_drop)
          |                           |-- framed stdin/stdout
          |                           |-- lifecycle state
          +-- exit observer <---------+-- wait/reap then notify
```

## Requirement-to-design trace

| Requirement | Design decision / verification |
| --- | --- |
| M31-R1 | Process-wide `OnceLock<Runtime>`; reject nested `run_sync` |
| M31-R2 | Actor owns child and IPC; JoinHandle retained; bounded mpsc |
| M31-R3 | 2 MiB frame limit and single deadline in `FramedIpc` |
| M31-R4 | Distinguish `NotStarted` and `Unknown`; no retry in facade |
| M31-R5 | Shutdown handshake, kill and `Child::wait`; runtime teardown remains an **unresolved design gap** |
| M31-R6 | Preserve JSON line protocol and synchronous facade |
| M31-R7 | Sanitize public errors; structured tracing **not implemented** |
| M31-R8 | Direct child only; descendants deferred to M3.2 |

## Nonfunctional requirement trace

| Requirement | Design and verification |
| --- | --- |
| M31-N1 | Bounded IPC, bounded queue, deadline-aware cleanup; blocked I/O fixtures pending |
| M31-N2 | Linux and Windows platform matrix; Windows direct-child liveness pending |
| M31-N3 | Existing quality and coverage gates; measured coverage evidence pending |
| M31-N4 | Fault-injection tests for malformed, truncated, EOF, broken pipe, shutdown and teardown; incomplete |

## Failure behavior

- Rejected before dispatch: `NotStarted`; worker remains usable.
- Potentially transmitted request times out or pipe closes: `Unknown`; actor must stop and never replay.
- Graceful shutdown: send shutdown frame, validate acknowledgement, wait for child exit within grace.
- Shutdown timeout: kill child and await reap; caller deadline stays bounded.
- Drop: close command channel and retain actor ownership through a reaper. **Current asynchronous reaper may be interrupted by runtime teardown**; must be fixed or accepted as a documented blocker before M3.1 completion.
- Worker exit/crash: close actor, reap direct child, reject further requests.

## Alternatives and decisions pending

1. **Async reaper on shared runtime (current):** low overhead; unsafe to claim guaranteed reaping during runtime shutdown.
2. **Dedicated bounded synchronous fallback:** stronger runtime-teardown behavior; must avoid blocking Drop indefinitely and account for Windows.
3. **Independent process reaper service:** robust ownership but more complexity; justify only if simpler fallback fails.

Do not silently select option 2 or 3 without explicit design approval.

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

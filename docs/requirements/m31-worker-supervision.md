# M3.1 — Worker supervision requirements

Status: **draft; maintainer agreement required**. Baseline revision: 1 (2026-11-09). Owner/approver: **pending**.
Related: [design](../design/m31-worker-supervision.md), [OpenSpec](../../openspec/specs/m31-tokio-supervisor.md), [checkpoint](../ferrum-readiness-checkpoint.md).

## Problem and outcome

A synchronous extension facade must safely manage independent JavaScript worker processes without per-worker Tokio runtimes, unbounded IPC, ambiguous retry semantics or orphaned direct children. The goal is deterministic behavior on Linux AMD64, Linux ARM64 and Windows AMD64.

## Scope

- In: worker start, protocol handshake, serialized invoke/event, bounded framing, monotonic deadline, cancellation, graceful and forced shutdown, direct-child reaping, diagnostics and tests.
- Out: descendant process-tree containment (M3.2), untrusted extension sandboxing, Boa async migration, Pi compatibility beyond the existing protocol, Docker.

## Functional requirements

- **M31-R1** One process-wide Tokio runtime; synchronous facade rejects reentrant runtime calls.
- **M31-R2** Supervisor owns the child, IPC handles, bounded queue and actor lifecycle. No unowned worker tasks.
- **M31-R3** Frame maximum 2 MiB; bounded queue; sequential requests per worker; one monotonic deadline covers enqueue/write/read.
- **M31-R4** Never automatically replay a request that might have executed. Expose `Unknown` versus `NotStarted` distinctly.
- **M31-R5** Normal shutdown acknowledges worker exit; failure, timeout, cancellation and Drop must terminate and reap the **direct child**, including when the runtime is tearing down.
- **M31-R6** Preserve existing extension protocol and sync API for registration, invoke, event and shutdown.
- **M31-R7** Expose sanitized errors and payload-free lifecycle diagnostics, without logging secrets or extension source.
- **M31-R8** Explicitly separate direct-child guarantees from M3.2 descendant containment.

## Nonfunctional and acceptance

- **M31-N1** No unbounded queue, IPC frame, blocking I/O task or indefinite cleanup wait.
- **M31-N2** Cross-platform tests on Linux AMD64/ARM64 and Windows AMD64; verify OS process liveness rather than only actor completion.
- **M31-N3** Maintain rustfmt, Clippy warnings-as-errors, security checks and >=90% measured line coverage; record actual coverage, not assumptions.
- **M31-N4** Tests cover malformed/truncated/oversized frames, EOF/broken pipe, blocked stdin and shutdown, cancellation, worker crash, repeated cycles and runtime teardown.

## Acceptance / sign-off

- [ ] Requirements approved (approver/date/revision pending).
- [ ] All M31-R1..R8 and M31-N1..N4 mapped to executable evidence.
- [ ] Full cross-platform CI and measured coverage linked.
- [ ] Remaining risks and M3.2 handoff accepted explicitly.

## Open questions

1. What is the maximum tolerated Drop cleanup latency when the shared runtime is stopping?
2. Which Windows process-query mechanism will establish direct-child disappearance without extra production dependencies?
3. Should diagnostics be structured via `tracing` now or a minimal event sink first?

These are **not approved decisions**; implementation should not invent acceptance thresholds.

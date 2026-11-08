# M3 — Runtime Reliability, Diagnostics and Resource Governance

Status: **PROPOSED — design only; implementation requires review and approval.**
Base: M2 merged to main. No production-code changes in this proposal.

## Goal and invariants
Ferrum SHALL fail predictably, isolate recoverable failures, protect user data and secrets, and produce actionable local diagnostics across Linux amd64/arm64 and Windows amd64.

- M3-01: All external failures SHALL have stable machine-readable error codes, retryability classification, causal chains, and sanitized user messages. Unexpected panics are defects, not ordinary control flow.
- M3-02: A supervisor SHALL own cancellation, task lifetimes, bounded queues, deadlines, child-process trees, and orderly shutdown. Cancellation MUST NOT replay side-effecting tools.
- M3-03: Resource budgets SHALL cap prompt/context bytes, IPC frames, log queues, tool output, concurrency, and wall time. Hard CPU/memory limits SHALL only be claimed when OS enforcement exists; otherwise fail closed or document limitations.
- M3-04: Persistent Boa workers SHALL be treated as trusted but fallible subprocesses, not sandboxes. Crashes and timeouts SHALL be isolated from the parent. Worker restart MUST NOT automatically replay tool calls.
- M3-05: SQLite session writes SHALL be transactional and recoverable. JSON exports SHALL use atomic replacement where supported, with durable-write guarantees stated per platform. Crash recovery MUST NOT invent successful tool results.
- M3-06: Structured tracing SHALL include session/run/request/tool correlation IDs, duration, error codes, and bounded resource counters. Prompts, tool bodies, environment variables, tokens, and credentials MUST NOT be logged by default.
- M3-07: Diagnostic events SHALL be local-first, bounded, versioned, and exportable with explicit user consent. OpenTelemetry export is optional and off by default.
- M3-08: Panic and fatal-exit handling SHALL produce a sanitized crash report with crash ID, binary version, OS/architecture, timestamp, error category and safe stack metadata when available.
- M3-09: Windows minidumps and Linux core dumps SHALL be optional, disabled by default, and documented as potentially containing raw secrets. Restrictive file permissions, storage location, size/retention controls and user consent are mandatory. No claim of reliable in-process dump generation after OOM/abort; OS or out-of-process capture may be necessary.
- M3-10: Diagnostic symbol artifacts (Windows PDB; Linux split debug symbols/build IDs) SHALL be generated and associated with exact release binaries without publishing sensitive dumps.
- M3-11: CLI SHALL provide offline `doctor`, `health`, and `diagnostics` commands with stable JSON mode and safe human-readable output.
- M3-12: Fault injection SHALL exercise killed worker, panic, malformed IPC, interrupted session write, disk full, network timeout, cancellation, excessive output, and startup recovery. Tests MUST assert no unintended replay and no secret disclosure.
- M3-13: CI SHALL preserve >=90% line coverage, fmt/clippy, cargo audit/deny, secret scan, workflow security, and Linux amd64/arm64 + Windows amd64 builds.

## Phases and deliverables
### M3.0 — Design freeze (this change)
Deliver error taxonomy, threat model, telemetry data contract, platform capability matrix, resource-budget policy, ADRs, and test matrix. No implementation.
### M3.1 — Supervisor and containment
Implement cancellation/deadline propagation, worker-tree termination, cleanup, error types, bounded queues. Tests before code.
### M3.2 — Logging and observability
Introduce `tracing` spans, redaction policy, rotating bounded local sinks, correlation IDs, local metrics, opt-in exporter.
### M3.3 — Crash and session recovery
Implement panic/fatal metadata, recovery checks, opt-in platform dumps, symbol mapping and redacted support bundles.
### M3.4 — Reliability verification
Fault-injection suite, performance baseline/regression budgets, end-to-end multi-platform matrix and release evidence.

## Preliminary acceptance scenarios
1. Killing a Boa worker during a write returns a distinct failed/unknown-outcome status; parent remains responsive; no replay.
2. SIGINT/console cancellation stops new work, drains bounded diagnostics and reaps owned children within configured grace period, then force-kills remaining children.
3. A log entry containing an API key or prompt body never emits that value; diagnostic bundle scan finds no canary secrets.
4. A crash during SQLite transaction leaves the last committed session readable; no partial success recorded.
5. Network timeouts classify as retryable only for idempotent operations; retry budgets and jitter prevent storms.
6. A user without dump opt-in receives no full-memory dump from Ferrum; documentation notes OS-level crash dump settings are separate.
7. All platform builds pass; security/quality/coverage gates stay unchanged.

## Open design decisions requiring M3.0 review
- Budget defaults and override policy (trusted/untrusted workspace distinction).
- Child-process-tree cleanup primitives: Windows Job Objects; Linux process groups/cgroups as available.
- Which crash dump capture mechanisms can be shipped as a single binary, and which require OS setup.
- Logging rotation and filesystem permission behavior across platforms.
- Telemetry cardinality, retention, schema compatibility and exporter authentication.
- Whether extension permissions/sandboxing are a separate M4 milestone; no sandbox claim in M3.

## Out of scope
- Full hostile-code sandboxing or guaranteed protection from OS OOM killer.
- Automatic replay of side-effecting tools.
- Remote telemetry collection enabled by default.
- Rewriting the agent architecture or adding new Pi/OMP API parity in M3.

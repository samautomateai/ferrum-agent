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

## M3.0 technical review — baseline observations (2026-11-08)
Source-reviewed against `src/worker.rs`, `src/extensions.rs`, `src/main.rs`, and existing M1.4–M1.6 OpenSpec. These are **design findings**, not evidence of a runtime defect or test execution.

1. `worker.rs` has a 2 MiB IPC frame cap, per-request `recv_timeout`, and `Child::kill/wait`. It does **not** currently establish Windows Job Objects, Linux process-group ownership, CPU/RSS limits, or bounded lifetime of any grandchildren. The stdout reader is a detached thread; lifecycle ownership needs an explicit contract.
2. Worker `stderr` is discarded. A crash can therefore lose the most useful diagnostic evidence; M3 must capture a bounded stderr ring without mixing it into stdout protocol.
3. A `recv_timeout` does not necessarily bound a blocking `stdin.write_all/flush`; M3.1 must account for end-to-end deadlines, not only reply wait.
4. `main.rs` uses `anyhow` for operational errors and a blocking HTTP client with a 180-second timeout. Cancellation and typed failure classification are not yet end-to-end.
5. `extensions.rs` caps source (256 KiB), argument/result (1 MiB), and Promise pumps (32). These are valuable existing soft limits, but **not** heap or CPU isolation. M3 must preserve them while adding enforceable worker limits where supported.
6. README contains historical unverified-build statements despite M2 CI having passed. Documentation cleanup should be tracked separately from reliability functionality.

## Proposed decisions for review (not yet frozen)
| ID | Decision | Initial policy |
|---|---|---|
| D1 | Error model | Stable `FERRUM-<DOMAIN>-<CODE>` public codes; internal source chains retained locally but sanitized at CLI boundary; explicit `retryable` and `outcome_unknown` fields |
| D2 | Supervisor | One parent-owned supervisor; per-worker cancellation and process-tree ownership; no automatic side-effect replay |
| D3 | Limits | Preserve current 2 MiB IPC and 1 MiB JS result caps; configurable budgets validated at startup; defaults and hard enforcement documented per platform |
| D4 | Worker isolation | Windows Job Object with kill-on-close; Linux process group for cleanup; cgroups v2 where delegated for hard memory limits, otherwise clearly mark as unenforced |
| D5 | Logging | `tracing` JSON/human output, stderr by default; opt-in file sink with restrictive permissions and bounded rotation; no prompt/response body logging |
| D6 | Crash evidence | Sanitized metadata on by default; full-memory dump off by default; Windows minidump and Linux core capture treated as platform integration, not portable in-process promises |
| D7 | Telemetry | Local counters and duration histograms first; OpenTelemetry export disabled by default; stable low-cardinality dimensions, no prompt/session IDs as metric labels |
| D8 | Session safety | SQLite transactions and WAL retained; explicit `unknown_outcome` state after interrupted side effects; JSON export uses temp-file + rename with documented fsync/Windows limitations |
| D9 | Retry policy | Only explicitly idempotent network requests may retry with exponential backoff/jitter and bounded attempt/time budget; never replay extension writes |
| D10 | Support bundles | Explicit CLI opt-in; archive allowlist, size and retention caps, canary secret tests; no full dumps included unless separately authorized |

## Implementation dependency order
`typed errors + redaction contract` → `supervisor + cancellation + child ownership` → `bounded diagnostics + tracing` → `session recovery + crash metadata` → `opt-in dumps + symbol pipeline` → `fault injection and release gates`.

Do not build dump generation before the data-handling and redaction policies are reviewed.

## TDD verification matrix
| Failure injected | Required observable outcome | Platform |
|---|---|---|
| Worker exits nonzero mid-request | Parent remains alive; stable error code; tool outcome marked unknown when side effects possible | All |
| Worker spawns grandchild then times out | Owned process tree terminated and reaped; no orphan left | Linux amd64/arm64, Windows amd64 |
| Worker stdout frame exceeds 2 MiB | Protocol rejection, bounded memory, worker cleanup | All |
| Worker stderr floods | Bounded ring and no deadlock | All |
| Stalled stdin write | Overall deadline enforced, worker terminated | All |
| SIGINT / Windows console interrupt | Graceful cancellation then bounded forced shutdown | Linux/Windows |
| Crash during SQLite transaction | Previous committed state readable; no fabricated success | All |
| Disk full during diagnostic write | Core agent remains operational where possible; explicit diagnostic failure | All |
| Secret canary in prompt/env/tool output | Absent from logs, diagnostic metadata and default support bundle | All |
| OOM / memory budget exceeded | Parent survives worker resource failure **where OS-enforced**; otherwise limitation documented | OS-dependent |
| User does not enable dumps | No Ferrum-triggered full-memory dump; OS dump settings remain independent | All |
| LLM 429 / timeout | Retry only when policy allows; bounded attempts and clear final error | All |

## Review exit criteria for M3.0
- Every D1–D10 decision approved or revised with rationale.
- Numeric default budgets and supported override behavior explicitly defined.
- Per-OS capability and privilege matrix documented; no blanket hard-memory-safety claim.
- Telemetry event schema and redaction threat model reviewed.
- Acceptance tests mapped to M3.1–M3.4 and CI platforms.
- OpenSpec and ADR updated to **Accepted** before production implementation.

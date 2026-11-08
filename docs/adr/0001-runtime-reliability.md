# ADR-0001: Local-first reliability supervisor and diagnostic boundaries

Status: **Proposed** (M3.0)

## Context
Ferrum combines Rust, persistent Boa child processes, LLM HTTP calls, and SQLite/JSON sessions. Rust's type safety does not prevent OOM, deadlocks, panics, lost subprocesses, or disclosure through logs and dumps. Existing worker deadlines are not equivalent to an OS sandbox.

## Decision proposed
1. Central runtime supervisor owns task cancellation, deadlines, child process lifecycle and bounded work queues.
2. Errors are typed at subsystem boundaries and mapped to stable public codes; panic hooks are last-resort diagnostics.
3. `tracing` is the common instrumentation API. Redaction is enforced at emission boundaries and reviewed by canary tests; arbitrary payload debug formatting is prohibited.
4. Local-only diagnostics are the default. Export and full-memory dump collection require explicit opt-in.
5. Full dumps are handled by OS-supported or out-of-process mechanisms, not promised as reliable from a crashing process.
6. Session recovery distinguishes committed outcomes, known failures and unknown outcomes. Side effects are never silently retried.

## Consequences
More infrastructure and testing before feature expansion; improved failure containment and supportability. Platform-specific limits and dump handling remain explicit instead of being hidden behind a false portability abstraction.

## Alternatives deferred
- One global catch-all error handler: loses retry and safety semantics.
- In-process Boa as an isolation boundary: cannot enforce robust memory isolation.
- Always-on crash dumps or cloud telemetry: unacceptable default exposure of secrets.

# Ferrum Agent — Delivery and Release Readiness Checkpoint

> Baseline: 2026-11-09. This is the authoritative *working checklist*, not a claim of release readiness.
> Update it in the **same commit or PR** as each material implementation change. Check a box only when its acceptance criteria are satisfied and link evidence (test, CI run, PR, benchmark or release artifact).
> Current baseline: PR #10, commit `c0d55c8`, CI run [42584693887](https://github.com/samautomateai/ferrum-agent/actions/runs/42584693887) — all six jobs green. This proves configured CI checks, not the remaining runtime fault-injection requirements.
> Scope: standalone Rust-native coding agent first; Pi/OMP compatibility and advanced orchestration thereafter. Docker packaging is parked.

## Requirements and design governance

- [ ] Approve [M3.1 requirements](requirements/m31-worker-supervision.md) and [M3.1 design](design/m31-worker-supervision.md) before claiming milestone sign-off.
- [ ] Adopt the [lightweight BMAD-inspired requirements/design workflow](development/requirements-and-design.md) as the project-wide baseline; maintainer approval pending.
- [ ] Keep requirement IDs, OpenSpec clauses, tests and checkpoint evidence traceable in each cohesive implementation PR.

## Status conventions and update protocol

- `[x]` = acceptance demonstrated and evidence linked. `[ ]` = incomplete, unverified, or not yet scoped.
- In every implementation PR: update relevant boxes, add an evidence link, note any new risks, and preserve uncompleted items.
- Do not equate code presence with behavior verified, or CI success with a stable release.
- Do not lower coverage/security thresholds to mark an item complete.
- Each milestone exits only after its acceptance matrix passes on Linux AMD64, Linux ARM64 and Windows AMD64 where applicable.
- For changes affecting behavior, write or update OpenSpec first and tests before implementation.

## M0 — Standalone agent foundation

- [x] Basic OpenAI-compatible LLM chat and tool-dispatch foundation exists. Evidence: `README.md`, `src/`.
- [x] SQLite sessions and JSON import/export implementation exists. Evidence: `README.md` M0.2.
- [x] SSE parsing and basic tool execution implementation exists. Evidence: `README.md` M0.
- [ ] Incremental terminal streaming, reliable reconnect/retry and explicit provider failure handling verified end-to-end.
- [ ] Session recovery, migration, atomicity, privacy and corrupted-store regression suite complete.
- [ ] Configuration precedence, environment/secrets redaction and offline diagnostics documented and tested.
- [ ] Production-grade read/edit/write safety, path-race resistance and bounded output verified.

## M1 — Native tools and extension runtime

- [x] Boa trusted extension host, worker process and lifecycle protocol implemented. Evidence: `README.md` M1.1–M1.6, `src/worker.rs`.
- [x] Bounded repository search, optimistic edits and read-only Git inspection implemented. Evidence: `README.md` M1.7–M1.9.
- [ ] Native ripgrep/fd integration or equivalent benchmarked search; Git-aware filtering and bounded results.
- [ ] Controlled shell execution with approval policy, deadlines, process-tree cleanup and output limits.
- [ ] Git staging, patch, commit and safe mutation workflows with explicit user authorization.
- [ ] Extension registration, event, invoke, shutdown and state persistence validated through real CLI tests.
- [ ] Stable versioned extension API, compatibility fixtures and extension developer documentation.

## M2 — Pi / OMP ecosystem compatibility

- [ ] Publish upstream API and behavior inventory (tools, commands, hooks, sessions, extensions, prompts, themes).
- [ ] Define supported/unsupported compatibility matrix with pinned upstream versions.
- [ ] Implement and verify representative Pi extension fixtures, including lifecycle and async behavior.
- [ ] Provide provider adapters and tool schema interoperability without copying upstream internals.
- [ ] Run regression/compatibility tests against pinned upstream fixtures in CI.
- [ ] Document explicit limitations and migration guide; never claim complete parity without evidence.

## M3 — Tokio supervision, cancellation and isolation

### M3.1 — Supervisor integration (PR #10)

- [x] Production synchronous worker facade delegates to shared Tokio supervisor. Evidence: PR #10, `src/worker.rs`.
- [x] Strict formatting, quality, tests and Linux AMD64/ARM64 + Windows CI passed at baseline. Evidence: [run 42581802633](https://github.com/samautomateai/ferrum-agent/actions/runs/42581802633).
- [x] Bounded IPC, deadline, crash, repeat-cycle and cancellation regression tests exist. Evidence: `tests/tokio_supervisor.rs`.
- [ ] Prove direct child **termination and reaping** on normal shutdown, timeout, cancellation and Drop using process-liveness assertions; avoid sleep-only tests.
- [ ] Prove no leaked actor tasks on shutdown timeout or runtime teardown; handle runtime shutdown before asynchronous reaper completion.
- [ ] Fault-inject broken pipe, EOF, malformed/truncated responses, blocked stdin and blocked shutdown.
- [ ] Verify full real-CLI invoke/event/shutdown behavior and non-replay of side-effecting calls.
- [ ] Structured lifecycle tracing with secrets and payloads redacted.
- [ ] Update `docs/m31-tokio-integration-status.md` with fresh evidence and measured coverage; remove stale 'CI unverified' statements.
- [ ] Mark PR #10 ready and merge only after the above acceptance items are satisfied.

### M3.2 — OS-level containment

- [ ] Windows Job Objects contain descendant process tree; tested with nested child fixtures.
- [ ] Linux process-group/cgroup strategy contains descendants on AMD64 and ARM64.
- [ ] Enforce capability permissions, environment minimization and workspace/filesystem boundaries.
- [ ] Document trusted/untrusted extension threat model and residual risks.
- [ ] Verify cancellation, deadline, kill/reap and cleanup under adversarial process behavior.

## M4 — Agentic execution and developer workflows

- [ ] Reliable multi-step planning/execution loop, tool error recovery and resumable tasks.
- [ ] Context-window accounting, compaction, session branching and provider/model fallback policies.
- [ ] MCP client with permission boundaries and bounded tool results.
- [ ] Skill discovery and loading with provenance and trust checks.
- [ ] OpenSpec-first SDD and TDD workflow enforcement, with CI evidence.
- [ ] Subagents and parallel task orchestration with isolation, cancellation and budget limits.
- [ ] Persistent project memory and audit trail with explicit privacy/retention controls.

## M5 — Cross-platform distribution and UX

- [ ] Versioned, reproducible Linux AMD64, Linux ARM64 and Windows AMD64 release binaries.
- [ ] Interactive terminal UX, incremental output, approvals, cancellation and session selection.
- [ ] Install/update/rollback strategy, checksums/signatures and supply-chain provenance.
- [ ] End-user setup, provider configuration, troubleshooting and migration documentation.
- [ ] Portable packaging; no Docker requirement for the first standalone release.

## M6 — Production hardening and release

- [ ] All mandatory CI, lint, audit, security and >=90% measured line coverage gates pass.
- [ ] End-to-end smoke tests on supported operating systems with real worker subprocesses.
- [ ] Benchmark startup, memory, latency, concurrency and failure recovery; record baselines.
- [ ] Threat model, dependency audit, secret-leak tests and adversarial workspace tests.
- [ ] Versioned changelog, compatibility policy, support boundaries and release checklist.
- [ ] Release candidate soak testing, rollback test and signed/checksummed artifacts.

## Release targets

### v0.1 — Reliable standalone coding agent

- [ ] M0, essential M1 native tools, M3.1/M3.2 containment and essential M5 distribution meet acceptance criteria.
- [ ] Useful independent coding workflow works without Pi/OMP extensions.
- [ ] Explicit limitations documented; release candidate passes M6 safety and reliability gates.

### v1.0 — Full ecosystem-ready agent

- [ ] v0.1 criteria remain green.
- [ ] M2 compatibility matrix meets explicitly published supported scope.
- [ ] M4 agentic workflows and extension/MCP permission boundaries meet acceptance criteria.
- [ ] M5/M6 release, security and operational evidence complete.

## Immediate next implementation sequence

1. M3.1: add observable process-ID/liveness fixture and assert direct-child reaping for shutdown/timeout/Drop.
2. M3.1: add broken-pipe, EOF, truncated-response and blocked-shutdown subprocess fault injection.
3. M3.1: exercise actual CLI extension invoke/event/shutdown, capture coverage and tracing evidence.
4. Update this checklist and M3.1 status with test links in the same commits; keep PR #10 draft until all M3.1 exit criteria are met.
5. Merge M3.1; then scope M3.2 OS containment before enabling untrusted extensions.

## Checkpoint log

Next cohesive verification unit (pending CI): Linux `/proc/<pid>` liveness assertions after graceful shutdown, Drop, and timed-out side-effecting calls. This covers Linux direct children only; Windows process-liveness, runtime teardown, blocked IPC, structured tracing and M3.2 descendant containment remain outstanding.


M3.1 additional fault coverage: timeout after dispatched side-effecting invocation must close the actor without replay; malformed JSON must terminate the worker and complete the actor cleanup observer. Tests passed in CI run 42589940863. OS-level liveness, runtime teardown, blocked stdin and structured tracing remain unchecked.


Verified by CI 42589940863: supervisor now exposes direct-child PID and an actor-cleanup observer; tests assert shutdown, crash, timeout and Drop actor completion, plus event/invoke continuity. These prove actor cleanup completion, **not independently OS-verified reaping or descendant containment**. Keep the full reaping criterion unchecked until OS-liveness evidence and runtime-teardown guarantees exist.


| Date | Commit / PR | Evidence | Decision / remaining risk |
| --- | --- | --- | --- |
| 2026-11-09 | PR #10, `4b1e8f5` | Malformed JSON protocol regression added in `tests/tokio_supervisor.rs`; CI pending. Worker rejection and no replay expected; reaping remains unproven. |\n| 2026-11-09 | PR #10, `ffec46d` | `tests/tokio_supervisor.rs`: shutdown-after-crash bounded regression added; CI verification pending. Does **not** establish child reaping. |\n| 2026-11-09 | PR #10, `c0d55c8` | [CI 42581802633](https://github.com/samautomateai/ferrum-agent/actions/runs/42581802633) | Six CI jobs green; direct-child reaping, fault injection and runtime-teardown guarantees still unproven. |

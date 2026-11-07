# Ferrum

Ferrum is an independent Rust-native coding agent inspired by Pi and OMP. **Not a fork and not yet feature-compatible.**

> Experimental M1.9 source: Rust compilation, tests, security checks and 90% coverage are not verified.

## Requirements
Rust stable, Cargo, and optionally Go Task.

## Build
```sh
cargo test
cargo build --release
```

## Run
```sh
export OPENAI_BASE_URL='https://your-gateway/v1'
export OPENAI_API_KEY='your-key'
export OPENAI_MODEL='your-model'
cargo run -- --session .ferrum-session.json --workspace . 'Find the entry point of this project'
# Opt-in writes:
cargo run -- --allow-write --workspace . 'Update the README'
```

Default operation is read-only. There is no shell execution in M0.

## Known limitations
- SSE Chat Completions streaming is enabled by default; use `--no-stream` for incompatible gateways.
- Streaming events are accumulated before displaying a response; incremental terminal token rendering is not yet implemented.
- Optional `--session path.json` persists messages, including prompts and tool outputs, in plaintext. Without it prompts are independent. Do not commit session files.
- No JS runtime, provider adapters, Pi extension compatibility or OMP code has been imported.
- Workspace confinement is canonical-path-based, not a race-resistant sandbox.
- Not compiled or tested in the artifact creation environment (Rust unavailable).

## Roadmap
M0.1: SSE parser and basic JSON session persistence implemented but uncompiled; next mock-server integration tests, atomic writes, output caps.
M1: QuickJS bridge, extension registration and fixtures.
M2: port/adapt OMP provider coverage, native tools and upstream compatibility suite.

## M0.2 session persistence

SQLite is the preferred store for local sessions. JSON remains available as a legacy
single-file session format. Do not specify `--session` and `--session-db` together.

```bash
cargo run -- --workspace . --session-db .ferrum.sqlite --session-id demo 'Inspect this project'
cargo run -- --session-db .ferrum.sqlite --session-id demo --export-json demo.json
cargo run -- --session-db .ferrum.sqlite --session-id imported --import-json demo.json
```

The JSON import/export format is an array of OpenAI chat messages, **not** Pi's
native JSONL session tree format. Pi JSONL compatibility is future work.
SQLite uses WAL mode and transactions, but only one writer at a time is supported.
Sessions can contain secrets and sensitive prompts; protect the database and exports.

M0.2 also changes write/edit tools to use temporary files and same-directory rename.
This is not a complete security boundary: workspace symlink races and Windows replace
semantics require further hardening. Do not run with `--allow-write` on untrusted
workspaces until those gaps are addressed.

Build/test status: source changes prepared; Cargo is unavailable in the packaging
runtime, so compilation and integration tests have not been executed here.

## M0.3: test-driven development and coverage
All changes must follow red-green-refactor. `task coverage` uses `cargo-llvm-cov` and fails below 90% line coverage. CI runs coverage on Linux and fmt/clippy/tests on Linux and Windows. The repository includes mock HTTP provider tests and negative tests for tools, paths, sessions and SSE. **Coverage has not been measured locally**; a failing coverage gate is expected until uncovered paths are tested.

Install coverage tooling with `cargo install cargo-llvm-cov --locked` and run `task coverage`.

## M0.4 reliability increment

- Reject absolute paths and lexical `..` traversal before filesystem canonicalization.
- Reject SSE responses lacking a successful `finish_reason`, even if `[DONE]` arrives.
- Limit incoming SSE line data to 8 MiB per completion.
- Add regression tests for absolute paths, symlink escape, SQLite reopen, and malformed stream completion.
- **Security boundary:** canonicalization is still susceptible to concurrent symlink replacement (TOCTOU); do not run untrusted code or grant writes to sensitive directories. Native shell execution remains disabled.

### Test-first acceptance gate

Run `task check` and `task coverage` before considering the milestone complete. A configured 90% gate is not proof of 90% achieved. No exemption or threshold reduction is allowed for production source; add tests or simplify code until the measured threshold passes. Record the actual measured percentage in the milestone report.

### Current validation status

Source inspection and ZIP integrity only; Rust compilation, unit tests, Clippy, and measured coverage remain **unverified** in this environment. This is a development snapshot, not a release.

## Quality gates (M0+)

See [quality policy](docs/quality-policy.md). Run `task quality` to execute mandatory local gates. This revision adds gate configuration, **not a verified green build**. `Cargo.lock` must be generated and committed before `--locked` gates can pass. The source-length checker currently reports existing violations requiring refactoring.

## M1.1 Boa trusted extension host (pending manual verification)

`src/extensions.rs` now implements a synchronous JSON tool handler bridge.
It is intentionally not connected to the agent tool dispatcher yet. The host
is for **trusted code only** and does not provide execution time or heap limits.
See `openspec/specs/m11-boa-tool-bridge.md` for acceptance criteria.

## M1.2 — Trusted extension tool dispatch

Use `--extension examples/review.js` to advertise `review_path` to the model.
The agent routes the returned tool call to Boa and persists its JSON result.
**Trusted local code only:** no async, cancellation, resource sandbox, or Pi extension API parity yet.
M0.8 quality/coverage and platform verification remain pending manual checks.

## M1.3 — Promise microtask support (unverified)

Boa extension tools may now return a JSON-serializable value or a Promise resolved
by Boa's internal microtask queue. Rejections and permanently pending promises
are reported as tool errors. This is **not** true async host I/O, cancellation,
wall-clock timeout enforcement, or a sandbox. Do not run untrusted extensions.
See `openspec/specs/m13-boa-promises.md` for scope and acceptance tests.

## M1.4 extension worker (unverified build)

Extension registration and invocation now run via a fresh `ferrum --extension-worker`
subprocess with JSON over stdin/stdout. `--extension-timeout 5` controls the
wall-clock deadline. This is **process separation, not a security sandbox**:
workers inherit host environment and filesystem access. Use only trusted
extensions until OS-level restrictions and capability permissions are added.
Each invocation reloads the extension, so JS global state does not persist.

M0.8 compilation, Cargo.lock, 90% coverage, and security gates remain manual
release blockers. `task quality` must pass before publishing a binary.

## M1.5 — persistent Boa workers (verification pending)

Boa extension handlers now run in a long-lived subprocess per extension. The worker keeps
JavaScript global state between calls and exchanges bounded newline-delimited JSON frames.
Per-invocation deadlines stop workers on failure; side-effecting calls are **never replayed**.
The design takes inspiration from OMP's extensible agent/tool model but does not copy OMP code.
This is **not an OS sandbox**. Only trusted extensions may run.

**Manual release blockers:** Cargo compilation, all Rust tests, >=90% line coverage,
Cargo.lock, Clippy, dependency/license/security scans, and Linux/Windows build verification.

## M1.6 — Extension lifecycle protocol (experimental)

Persistent Boa workers now accept optional `event` and `shutdown` fields in their JSONL
requests. `pi.on(event, callback)` registers a callable event handler; event dispatch
waits for Promise microtasks. A shutdown request acknowledges and exits the worker.
See `openspec/specs/m16-extension-lifecycle.md` for limitations and acceptance gates.
**This is not yet wired to agent-level session lifecycle broadcasts.**

### M1.7: bounded native repository search

The built-in `grep` now uses a deterministic, ignore-aware Rust walker and bounded output
(100 matches / 32 KiB). Results are workspace-relative and line-numbered. This is the first
slice of OMP-inspired native tool work; benchmarking, edit improvements and Git-aware tools
are still pending. Rust build/test verification remains required before release.

## M1.8: optimistic exact-match edits

`edit_file` accepts optional `expected_sha256` (SHA-256 of the exact original file bytes).
It rejects stale hashes, ambiguous matches, no-ops, and edits exceeding 1 MiB.
Untouched CRLF and UTF-8 bytes are preserved. This is **not** a race-free filesystem
security boundary; see `openspec/specs/m18-reliable-edit.md`.

## M1.9 — Read-only Git inspection

The agent now advertises `git_status` and `git_diff` tools, invoking the installed Git
executable without a shell. `git_diff` accepts `staged: true` and optional workspace-relative
`path`. Git tools require `--workspace` to point to the repository root. Output is capped at
64 KiB. No Git mutations, patch application, commits or staging are exposed. Git subprocess
execution does not yet have a deadline. **Rust build and tests are unverified**; this remains
a development snapshot until all release gates pass.

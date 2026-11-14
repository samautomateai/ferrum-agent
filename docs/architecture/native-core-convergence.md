# Native core convergence: M0 reconciliation (draft)

Status: proposed; source inspection only, not an accepted milestone.
Scope: retain Ferrum's existing Rust implementation while establishing a native, independently testable agent core and a common execution authorization boundary.

## Baseline evidence

- `src/main.rs` currently owns CLI parsing, blocking model requests, session handling, tool dispatch and the turn loop.
- `src/extensions.rs` implements a limited trusted Boa host; it does not provide Node/Bun or full Pi/OpenCode API compatibility.
- `src/main.rs::dispatch_tool` sends extension tools to a worker before routing native tools; no common pre-dispatch authorization is visible there.
- `Cargo.lock` is committed. Any documentation claiming it is absent is stale.
- The repository has progressed beyond its README's M1.9 narrative: worker supervisor and Tokio modules are registered in `main.rs`. Audit the latest implementation before replacing worker infrastructure.
- No local Cargo, CI or coverage checks were run during this assessment. Do not infer a green baseline.

## Architecture decisions

1. Keep one Rust agent core, with adapters for model, session, tool registry and event sink. Do not create new crates until boundaries have tests.
2. Add a common `ToolAuthorization` decision before *every* native or extension dispatch. Classify requested capabilities; do not infer trust from extension registration.
3. Preserve existing Boa worker support as a trusted-script backend. Introduce one versioned compatibility-worker protocol with ecosystem adapters for Pi and OpenCode in later milestones.
4. Never describe process isolation as a security sandbox. A worker that inherits host environment and filesystem access is trusted-only until OS isolation is demonstrated.
5. Preserve existing session format and compatibility while extracting the agent loop. Crash recovery must not replay side-effecting tool calls.

## Implementation slices (test first)

### Slice A: baseline and contract tests

- Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --all-targets`, and `task quality` where prerequisites exist.
- Capture actual results and coverage in a dated report; fix stale documentation only after checking the current branch.
- Add dispatch tests for unknown tool, native read, native write, extension tool, duplicate name and worker failure.

### Slice B: central authorization

- Define `ToolRequest { session_id, origin, tool_name, arguments, workspace }` and `Decision { Allow, Deny(reason), RequireApproval(capabilities) }`.
- Default to deny for unknown operations; preserve the existing opt-in `--allow-write` behavior during migration.
- Evaluate authorization *before* invoking a worker or native tool.
- For noninteractive mode, `RequireApproval` fails closed unless an explicit approved policy applies.
- Record decisions and invocation IDs without logging credentials or unredacted tool payloads.

### Slice C: native agent-loop extraction

- Move turn progression and tool-call normalization out of CLI into a library module with injected provider, dispatcher, store and event sink.
- Keep CLI behavior stable; use deterministic mock provider and tool dispatcher tests.
- Introduce cancellation and bounded event streaming as separate verified changes rather than converting all I/O in one commit.

### Slice D: compatibility-worker contract

- Define a versioned IPC envelope with request ID, protocol version, extension identity, capability declaration and structured errors.
- Cover initialize, register, invoke, event, shutdown, oversized frames, timeout and worker crash with mock-worker tests.
- Do not claim Pi/OpenCode parity until representative upstream extensions pass compatibility tests.

## Acceptance gates

- Existing tests remain green on Linux and Windows.
- All built-in and extension tool calls pass the same authorization interface.
- A denied extension invocation does not reach the worker.
- Agent loop can run prompt → tool → response with a deterministic provider and no network.
- Cancelled or interrupted side-effecting calls are never silently replayed.
- Coverage threshold and security checks are measured, not merely configured.
- No general shell execution or untrusted extension execution is enabled without reviewed containment.

## Out of scope

- Rewriting Ferrum from scratch.
- Full Pi/OpenCode compatibility.
- Arbitrary shell execution without sandbox/approval.
- Desktop, VS Code and multi-agent orchestration.

## Next PR

Implement Slice A and the minimal central authorization boundary in one cohesive change, with regression tests. Avoid simultaneously rewriting the model provider and worker supervisor.

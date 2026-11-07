# M1.1 — Trusted Boa synchronous tool execution

## Status
Implementation drafted; Rust build, tests and coverage require manual verification.
M0.8 remains a release blocker until quality gates are verified.

## Requirements
- R1: registerTool requires a unique nonblank name and callable execute handler.
- R2: a loaded extension can execute its own registered handler with JSON input.
- R3: invalid or unknown tools, thrown JS errors, and undefined results fail explicitly.
- R4: JSON argument encoding prevents JavaScript source injection.
- R5: extension source, input and output have size caps.
- R6: contexts are isolated from one another for registrations.

## Explicit exclusions
- No promise/async execution or job queue.
- No cancellation, time budgets, heap quotas or process isolation.
- No TypeScript transpilation or Node.js module resolution.
- No integration with agent tool schema / dispatcher yet.
- Not safe to execute untrusted extensions.

## Acceptance
- cargo fmt --all -- --check
- cargo clippy --locked --all-targets -- -D warnings
- cargo test --locked --all-targets
- cargo llvm-cov --locked --all-targets --fail-under-lines 90
- task quality
- Manual confirmation on Linux AMD64, ARM64 and Windows AMD64.

## Next milestone
M1.2: wire extension tools into agent dispatcher and tool schemas, then add
async promise support with cancellation and resource limits before allowing
third-party extensions.

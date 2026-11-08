# M3.1 — TDD implementation plan (not yet authorized)

Status: PLANNED; blocked on M3.0 decisions and acceptance.

## Red-green-refactor sequence
1. Add error-contract tests: stable code, sanitized display, retryability, and `unknown` outcome for interrupted side-effecting tools.
2. Add supervisor state-machine tests: reject new work during drain, idempotent stop, bounded queue overflow, cancellation propagation.
3. Add process-tree integration tests: timeout with grandchild; cleanup on Linux amd64/arm64 and Windows amd64; distinguish OS capability availability.
4. Add IPC deadline tests: stalled stdin write, oversized frame, malformed response, broken pipe, stdout closure, stderr flood.
5. Add worker failure tests: nonzero exit, panic, safe restart for **future** calls only, no replay of interrupted invocation.
6. Add resource-enforcement tests: configured cap applied or explicit unavailable error; no misleading successful enforcement report.
7. Add session interruption tests: SQLite last committed transaction intact, JSON export failure does not overwrite previous valid export.
8. Run existing `task quality` and >=90% coverage; attach measured results and platform evidence.

## Evidence required per change
- Failing test committed before implementation where practical; CI artifacts show test result.
- Code review includes security analysis of secret handling and process ownership.
- Benchmarks record startup, idle RSS, peak worker RSS, time-to-cancel and cleanup latency.
- No feature is marked complete solely because code compiled.

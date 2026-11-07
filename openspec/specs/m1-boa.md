# M1: Boa extension runtime (RED/GREEN acceptance contract)

## Status
M1.0 prototype implemented, **manual compilation and verification required**.
M0.8 remains blocked on Rust compilation, Cargo.lock, coverage, and security scans.

## Scope
- Use the pure-Rust `boa_engine` interpreter.
- Discover declarations from trusted JavaScript extension snippets in isolated contexts.
- Return a serializable manifest of registered tools, commands and events.
- Reject invalid declarations and source larger than 256 KiB.
- Unit-test success, failures, size bounds, and independent contexts.

## Explicit non-goals / safety limits
- The prototype does **not** execute extension tool handlers or commands.
- No TypeScript, ESM loader, npm/Node APIs, async hooks, Pi compatibility, or permissions yet.
- Source-size limits are not CPU/memory limits. **Do not run untrusted code**.
- No claim of hardened isolation, 90% measured coverage, or successful compilation.

## Exit criteria
- `cargo generate-lockfile` and commit Cargo.lock.
- `cargo fmt --all` and `task quality` green on supported CI targets.
- >=90% line coverage measured, no security-gate bypasses.
- Validate Boa license and transitive dependencies using `cargo deny`.
- Before enabling untrusted extensions: out-of-process isolation, CPU/memory limits,
  capability restrictions, and crash/timeout tests.
